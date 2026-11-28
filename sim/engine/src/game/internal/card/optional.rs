//! Optional declaration is separate from its all-or-nothing payment.
use super::play::PlayState;
use super::{AbilitySource, Execution, Game, Result, invalid, unsupported};
use crate::game::internal::{
    Answer, CostPaymentPurpose, DecisionToken, InputDomain, InstructionOutcome, PlayDeclaration,
    QuestionStage, Wait,
};
use crate::ir::events::discard as discard_header;
use crate::ir::reveal_cost as reveal_payment;
use crate::ir::shuffle;
use crate::ir::x_pp;
use crate::ir::{
    Ability, AbilityKind, AbilityOrigin, Action, Binding, Bool, Cardinality, ChoiceDomain, Control,
    CostSpec, DamageAmount, DamageFlags, DecisionId, DecisionKind, DiscardEvent, EventPattern,
    Expr, InstructionContext, Int, KeywordId, MoveDestination, MoveReason, MovementEvent, Nat,
    Node, ObjectOperand, PaymentOptionality, PaymentResult, PaymentTime, PlaySlot, PlayerId,
    PlayerRef, PlayerRelation, Program, ReceiptId, RecipientSet, RepeatBody, Resource,
    ResourceOperation, RuntimeReceiptId, SelectionView, Selector, TriggerCondition, Visibility,
    WellFormedCardProgram, ZoneKind, ZoneRef, optional_payment,
};
use core::slice::from_ref;

#[derive(Clone, Debug)]
pub(super) struct OptionalState {
    pub(super) costs: CostSpec,
    pub(super) required: bool,
    pub(super) receipt: ReceiptId,
    pub(super) accepted: Option<bool>,
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct ResolutionPayment {
    pub(in crate::game::internal) costs: CostSpec,
    pub(in crate::game::internal) on_paid: Program,
    pub(in crate::game::internal) receipt: ReceiptId,
}

pub(super) fn check_costs(control: &Control, origin: &AbilitySource) -> Result<()> {
    let Control::Pay {
        at,
        costs,
        optional,
        ..
    } = control
    else {
        return Err(unsupported("optional payment control"));
    };
    let required = *optional == PaymentOptionality::Required;
    // Play-time declarations are always answered by the controller, so only a resolution
    // payment may name another payer.
    let foreign = *at == PaymentTime::ResolutionChoice;
    let CostSpec::Ordered { steps } = costs else {
        return Err(unsupported("optional cost alternatives"));
    };
    if steps.is_empty() {
        return Err(unsupported("empty optional payment"));
    }
    for step in steps {
        let instruction = &step.instruction;
        let payer = instruction.actor == (PlayerRef::Controller {})
            || (foreign
                && super::super::alternatives::participant(&instruction.actor)
                && matches!(&instruction.action, Action::Resource { player, .. }
                    if *player == instruction.actor));
        if step.materials.is_some()
            || instruction.precondition.is_some()
            || instruction.source != *origin
            || !payer
            || instruction.context != InstructionContext::Cost
            || !(matches!(&instruction.action, Action::Resource { player,
            resource: Resource::PpCurrent, operation: ResourceOperation::Pay,
            amount } if *player == instruction.actor && x_pp::cost_amount(amount))
                || (foreign && super::super::resource::maximum_pp_cost(&instruction.action))
                || (required && super::super::life::is_cost(&instruction.action))
                || (required
                    && matches!(
                        reveal_payment::objects(&instruction.action),
                        Some(Selector::Bound { .. })
                    ))
                || (required && source_banish(&instruction.action))
                || (required
                    && super::super::movement::own_deck_top_banish(&instruction.action).is_some())
                || super::hand_payment::check_cost(&instruction.action)
                || shuffle::payment(&instruction.action)
                || super::material::source_field_cost(&instruction.action)
                || super::super::counters::is_cost(&instruction.action)
                || super::material::cost_objects(&instruction.action)
                    .is_some_and(|objects| matches!(objects, Selector::Bound { .. })))
        {
            return Err(unsupported("optional payment source or cost"));
        }
    }
    Ok(())
}

pub(super) fn check_card(card: &WellFormedCardProgram) -> Result<()> {
    let exchange = card
        .card()
        .card_play_costs
        .iter()
        .any(|cost| cost.node.exchange.is_some());
    for ability in &card.card().abilities {
        if let Some(body) = &ability.node.body {
            if has_resolution_payment(body)
                && !(ability.node.kind == AbilityKind::Spell
                    && resolution_costs(body, &earth_rite_cost))
                && !(ability.node.kind == AbilityKind::Triggered
                    && matches!(ability.node.source.origin, AbilityOrigin::Printed { .. })
                    && (((matches!(
                        ability.node.active_zones.as_slice(),
                        [ZoneKind::Field | ZoneKind::Cemetery]
                    ) || super::triggers::phase_in_zone(&ability.node, ZoneKind::Cemetery))
                        && resolution_costs(body, &source_resolution_cost))
                        || hand_pp_payment(&ability.node).is_some()))
                && !(ability.node.kind == AbilityKind::Triggered
                    && ability.node.active_zones == [ZoneKind::Field]
                    && resolution_costs(body, &super::hand_payment::resolution_cost))
                && !(ability.node.kind == AbilityKind::Triggered
                    && matches!(
                        &ability.node.source.origin,
                        AbilityOrigin::Keyword {
                            keyword: KeywordId::Lastword,
                            ..
                        }
                    )
                    && resolution_costs(body, &maximum_pp))
            {
                return Err(unsupported(
                    "resolution source payment needs a field auto ability",
                ));
            }
            if let Some((costs, _)) = optional_payment::declaration(body) {
                let play = ability
                    .node
                    .play
                    .as_ref()
                    .ok_or_else(|| unsupported("payment without play parameters"))?;
                let CostSpec::Ordered { steps } = costs else {
                    return Err(unsupported("payment alternatives"));
                };
                for step in steps {
                    if source_banish(&step.instruction.action)
                        && !cemetery_self_banish(&ability.node)
                    {
                        return Err(unsupported(
                            "self banish head cost needs a cemetery entry trigger",
                        ));
                    }
                    if super::material::source_field_cost(&step.instruction.action) {
                        return Err(unsupported("source costs require a resolution payment"));
                    }
                    let supported = matches!(step.instruction.action, Action::Resource { .. })
                        || (optional_payment::required(body)
                            && super::super::life::is_cost(&step.instruction.action))
                        || super::hand_payment::check_cost(&step.instruction.action)
                        || shuffle::payment(&step.instruction.action)
                        || play.parameters.iter().any(|decision| {
                            reveal_payment::field_parameter(&ability.node, decision)
                                && reveal_payment::selected(
                                    &step.instruction.action,
                                    from_ref(decision),
                                )
                        })
                        || super::material::check_cost(&step.instruction.action, play)
                        || super::super::counters::check_cost(&step.instruction.action, play);
                    if !supported {
                        return Err(unsupported("payment material parameter"));
                    }
                }
                if optional_payment::required(body) && ability.node.kind != AbilityKind::Triggered {
                    return Err(unsupported("required head payment outside a trigger"));
                }
            }
            let modes = super::play::has_mode_payments(body)?;
            if (modes && (exchange || optional_payment::declaration(body).is_some()))
                || (optional_payment::declaration(body).is_some()
                    && (exchange || ability.node.kind == AbilityKind::Activated))
            {
                return Err(unsupported(
                    "optional declaration with exchange or activation",
                ));
            }
            check_body(body, true)?;
        }
    }
    Ok(())
}

fn source_banish(action: &Action) -> bool {
    matches!(action, Action::Banish { objects: Selector::Objects { objects } }
        if objects == &[ObjectOperand::SourceHolder {}])
}

/// The cemetery entry uses its saved physical source, never another copy with the same name.
pub(super) fn cemetery_self_banish(ability: &Ability) -> bool {
    if ability.kind != AbilityKind::Triggered
        || ability.active_zones != [ZoneKind::Cemetery]
        || !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        || !matches!(
            ability.trigger.as_ref().map(|trigger| &trigger.on),
            Some(TriggerCondition::Event {
                pattern: EventPattern::EnterField { .. }
            })
        )
    {
        return false;
    }
    let Some(body) = &ability.body else {
        return false;
    };
    let Some((CostSpec::Ordered { steps }, _)) = optional_payment::declaration(body) else {
        return false;
    };
    optional_payment::required(body)
        && matches!(steps.as_slice(), [step] if step.materials.is_none()
            && step.instruction.precondition.is_none()
            && step.instruction.context == InstructionContext::Cost
            && step.instruction.actor == (PlayerRef::Controller {})
            && step.instruction.source == ability.source
            && source_banish(&step.instruction.action))
}

fn positive_pp(costs: &CostSpec) -> bool {
    player_pp(costs) == Some(&PlayerRef::Controller {})
}

fn maximum_pp(costs: &CostSpec) -> bool {
    matches!(costs, CostSpec::Ordered { steps }
        if matches!(steps.as_slice(), [step]
            if super::super::resource::maximum_pp_cost(&step.instruction.action)))
}

fn player_pp(costs: &CostSpec) -> Option<&PlayerRef> {
    let CostSpec::Ordered { steps } = costs else {
        return None;
    };
    let [step] = steps.as_slice() else {
        return None;
    };
    let instruction = &step.instruction;
    (step.materials.is_none()
        && instruction.precondition.is_none()
        && instruction.context == InstructionContext::Cost
        && super::super::alternatives::participant(&instruction.actor)
        && matches!(&instruction.action, Action::Resource {
            player, resource: Resource::PpCurrent, operation: ResourceOperation::Pay,
            amount: Expr::Literal(Int(n)),
        } if *player == instruction.actor && *n > 0))
    .then_some(&instruction.actor)
}

fn discard_return(program: &Program) -> Option<Binding<DiscardEvent>> {
    let [step] = program.steps.as_slice() else {
        return None;
    };
    let Node::Instruction { instruction } = &step.node else {
        return None;
    };
    let Action::Move {
        objects: Selector::Objects { objects },
        movement,
        reason: MoveReason::Put,
    } = &instruction.action
    else {
        return None;
    };
    if movement.to
        != (MoveDestination::Shared {
            zone: ZoneRef {
                player: PlayerRef::Controller {},
                kind: ZoneKind::Field,
            },
        })
    {
        return None;
    }
    let [
        ObjectOperand::FollowEventMove {
            event: MovementEvent::Discard { event },
        },
    ] = objects.as_slice()
    else {
        return None;
    };
    Some(*event)
}

fn discard_target_damage(ability: &Ability, program: &Program) -> bool {
    let [step] = program.steps.as_slice() else {
        return false;
    };
    let Node::Instruction { instruction } = &step.node else {
        return false;
    };
    let Action::Damage {
        recipients:
            RecipientSet::Objects {
                objects: Selector::Bound { reference },
            },
        amount: DamageAmount::Uniform {
            amount: Expr::Literal(Int(1..)),
        },
        flags: DamageFlags::Ability,
        ..
    } = &instruction.action
    else {
        return false;
    };
    let Some([target]) = ability.play.as_ref().map(|play| play.parameters.as_slice()) else {
        return false;
    };
    let ChoiceDomain::Objects {
        binding,
        constraint,
        target_roles: None,
    } = &target.domain
    else {
        return false;
    };
    let [group] = constraint.groups.as_slice() else {
        return false;
    };
    target.actor == (PlayerRef::Controller {})
        && target.visibility == (Visibility::Public {})
        && matches!(
            target.kind,
            DecisionKind::PlayParameter {
                slot: PlaySlot::Targets,
                ..
            }
        )
        && binding == reference
        && constraint.cardinality
            == (Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            })
        && group.cardinality == constraint.cardinality
        && constraint.aggregate_bounds.is_empty()
        && matches!(&group.selector, Selector::Zone {
            zone: ZoneKind::Field, view: SelectionView::Current {}, players, ..
        } if players.relation == PlayerRelation::Opponents
            && players.relative_to == (PlayerRef::Controller {}))
}

enum HandPpPayment<'ability> {
    Return,
    Targeted(&'ability CostSpec),
}

fn hand_pp_payment(ability: &Ability) -> Option<HandPpPayment<'_>> {
    if ability.active_zones != [ZoneKind::Hand] {
        return None;
    }
    let event = discard_header::self_event(ability.trigger.as_ref()?)?;
    let body = ability.body.as_ref()?;
    let [step] = body.steps.as_slice() else {
        return None;
    };
    let (payment, guarded) = if let Node::Control {
        control:
            Control::If {
                condition,
                then_body,
                else_body: None,
            },
    } = &step.node
    {
        if !optional_payment::public_pp_guard(condition) {
            return None;
        }
        let [payment] = then_body.steps.as_slice() else {
            return None;
        };
        (payment, true)
    } else {
        (step, false)
    };
    let Node::Control {
        control:
            Control::Pay {
                at: PaymentTime::ResolutionChoice,
                optional: PaymentOptionality::Optional,
                costs,
                on_paid,
                ..
            },
    } = &payment.node
    else {
        return None;
    };
    if !positive_pp(costs) {
        return None;
    }
    if discard_return(on_paid) == Some(event) {
        Some(HandPpPayment::Return)
    } else if !guarded && discard_target_damage(ability, on_paid) {
        Some(HandPpPayment::Targeted(costs))
    } else {
        None
    }
}

pub(super) fn targeted_discard_pp_cost(ability: &Ability) -> Option<&CostSpec> {
    match hand_pp_payment(ability)? {
        HandPpPayment::Targeted(costs) => Some(costs),
        HandPpPayment::Return => None,
    }
}

fn earth_rite_material(costs: &CostSpec) -> Option<Binding<Selector>> {
    let CostSpec::Ordered { steps } = costs else {
        return None;
    };
    let [step] = steps.as_slice() else {
        return None;
    };
    if step.keyword != Some(KeywordId::EarthRite) {
        return None;
    }
    super::super::counters::selected_cost(&step.instruction.action)
}

fn earth_rite_cost(costs: &CostSpec) -> bool {
    earth_rite_material(costs).is_some()
}

fn source_resolution_cost(costs: &CostSpec) -> bool {
    earth_rite_cost(costs)
        || player_pp(costs).is_some()
        || maximum_pp(costs)
        || super::hand_payment::resolution_cost(costs)
        || matches!(costs, CostSpec::Ordered { steps } if !steps.is_empty()
            && steps.iter().all(|cost| super::material::source_field_cost(&cost.instruction.action)))
}

fn resolution_costs(program: &Program, check: &impl Fn(&CostSpec) -> bool) -> bool {
    program.steps.iter().all(|step| match &step.node {
        Node::Control {
            control:
                Control::Pay {
                    at: PaymentTime::ResolutionChoice,
                    costs,
                    on_paid,
                    ..
                },
        } => check(costs) && resolution_costs(on_paid, check),
        Node::Control {
            control:
                Control::If {
                    then_body,
                    else_body,
                    ..
                },
        } => {
            resolution_costs(then_body, check)
                && else_body
                    .as_ref()
                    .is_none_or(|body| resolution_costs(body, check))
        }
        Node::Control {
            control:
                Control::Sequence { body }
                | Control::May { body, .. }
                | Control::Repeat {
                    body: RepeatBody::Program { body },
                    ..
                },
        } => resolution_costs(body, check),
        Node::Control {
            control: Control::Mode { modes, .. },
        } => modes.iter().all(|mode| resolution_costs(&mode.body, check)),
        _ => true,
    })
}

pub(super) fn check_body(program: &Program, root: bool) -> Result<()> {
    for (index, step) in program.steps.iter().enumerate() {
        match &step.node {
            Node::Control {
                control:
                    control @ Control::Pay {
                        at: PaymentTime::ResolutionChoice,
                        on_paid,
                        ..
                    },
            } => {
                check_control(control)?;
                check_body(on_paid, false)?;
            }
            Node::Control {
                control: control @ Control::Pay { .. },
            } => {
                if !root
                    || index != 0
                    || !matches!(control, Control::Pay { on_paid, .. } if on_paid.steps.is_empty())
                {
                    return Err(unsupported("one root optional declaration"));
                }
                check_control(control)?;
            }
            // A fixed repeat runs its body inline, so the body keeps every payment restriction.
            Node::Control {
                control:
                    Control::Sequence { body }
                    | Control::May { body, .. }
                    | Control::Repeat {
                        body: RepeatBody::Program { body },
                        ..
                    },
            } => check_body(body, false)?,
            Node::Control {
                control:
                    Control::If {
                        then_body,
                        else_body,
                        ..
                    },
            } => {
                check_body(then_body, false)?;
                if let Some(body) = else_body {
                    check_body(body, false)?;
                }
            }
            Node::Control {
                control: Control::Mode { modes, .. },
            } => {
                for mode in modes {
                    if let Some((_, _, on_paid)) = optional_payment::mode_declaration(&mode.body) {
                        check_body(on_paid, false)?;
                    } else {
                        check_body(&mode.body, false)?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn has_resolution_payment(program: &Program) -> bool {
    program.steps.iter().any(|step| match &step.node {
        Node::Control {
            control:
                Control::Pay {
                    at: PaymentTime::ResolutionChoice,
                    ..
                },
        } => true,
        Node::Control {
            control:
                Control::Sequence { body }
                | Control::May { body, .. }
                | Control::Repeat {
                    body: RepeatBody::Program { body },
                    ..
                },
        } => has_resolution_payment(body),
        Node::Control {
            control:
                Control::If {
                    then_body,
                    else_body,
                    ..
                },
        } => {
            has_resolution_payment(then_body)
                || else_body.as_ref().is_some_and(has_resolution_payment)
        }
        Node::Control {
            control: Control::Mode { modes, .. },
        } => modes.iter().any(|mode| has_resolution_payment(&mode.body)),
        _ => false,
    })
}

pub(super) fn take(body: &mut Program) -> Option<OptionalState> {
    let (costs, receipt) = optional_payment::declaration(body)?;
    let state = OptionalState {
        required: optional_payment::required(body),
        costs: costs.clone(),
        receipt,
        accepted: None,
    };
    body.steps.remove(0);
    Some(state)
}

pub(super) fn mode_state(body: &Program) -> Option<OptionalState> {
    let (costs, receipt, _) = optional_payment::mode_declaration(body)?;
    Some(OptionalState {
        costs: costs.clone(),
        required: false,
        receipt,
        accepted: None,
    })
}

pub(in crate::game::internal) fn accept(
    execution: &mut Execution,
    player: PlayerId,
    accepted: bool,
) -> Result<bool> {
    let Some(payment) = execution.play.as_mut().and_then(|play| {
        play.optional
            .iter_mut()
            .find(|payment| payment.accepted.is_none())
    }) else {
        return Ok(false);
    };
    if payment.accepted.replace(accepted).is_some() {
        return Err(invalid("repeated optional declaration"));
    }
    let declaration = PlayDeclaration::OptionalCost {
        player,
        receipt: payment.receipt,
        required: payment.required,
        accepted,
    };
    execution.play_declarations.push(declaration);
    Ok(true)
}

pub(in crate::game::internal) fn check_control(control: &Control) -> Result<()> {
    if matches!(control, Control::Pay {
        at: PaymentTime::ResolutionChoice,
        costs,
        optional: PaymentOptionality::Required,
        ..
    } if earth_rite_cost(costs))
    {
        return Ok(());
    }
    if let Control::Pay {
        at: PaymentTime::ResolutionChoice,
        costs: costs @ CostSpec::Ordered { steps },
        optional: PaymentOptionality::Optional,
        on_paid,
        ..
    } = control
    {
        if (positive_pp(costs) && discard_return(on_paid).is_some())
            || player_pp(costs).is_some()
            || maximum_pp(costs)
            || super::hand_payment::resolution_cost(costs)
            || (!steps.is_empty()
                && steps
                    .iter()
                    .all(|step| super::material::source_field_cost(&step.instruction.action)))
        {
            return Ok(());
        }
        return Err(unsupported(
            "resolution payment only supports source field costs or player PP",
        ));
    }
    if !matches!(control, Control::Pay { at: PaymentTime::PlayParameter,
        optional, on_paid, .. } if on_paid.steps.is_empty() || *optional == PaymentOptionality::Optional)
    {
        return Err(unsupported("optional payment timing or continuation"));
    }
    Ok(())
}

impl Game {
    pub(in crate::game::internal) fn begin_internal_payment(
        &mut self,
        execution: &mut Execution,
        control: Control,
    ) -> Result<()> {
        let Control::Pay {
            at: PaymentTime::ResolutionChoice,
            costs,
            on_paid,
            receipt,
            optional,
        } = control
        else {
            return Err(unsupported("resolution payment timing"));
        };
        if optional == PaymentOptionality::Required {
            let reference = earth_rite_material(&costs)
                .ok_or_else(|| unsupported("declared Earth Rite payment"))?;
            let accepted = !execution
                .objects(self, &Selector::Bound { reference })?
                .is_empty();
            return self.resolve_internal_payment(execution, &costs, on_paid, receipt, accepted);
        }
        // The payer answers its own payment; source costs stay with the effect controller.
        let payer = match player_pp(&costs) {
            Some(payer) => execution.player(self, payer)?,
            None => execution.context.controller,
        };
        self.internal_wait(
            execution,
            DecisionId(receipt.0),
            payer,
            InputDomain::OptionalCost,
            Wait::ResolutionPayment(ResolutionPayment {
                costs,
                on_paid,
                receipt,
            }),
        )?;
        execution.question_stage = Some(QuestionStage::Pay);
        Ok(())
    }

    pub(in crate::game::internal) fn answer_resolution_payment(
        &mut self,
        execution: &mut Execution,
        payment: ResolutionPayment,
        accepted: bool,
    ) -> Result<()> {
        if accepted && super::hand_payment::resolution_cost(&payment.costs) {
            match self.internal_hand_cost_materials(execution, &payment.costs) {
                Ok(true) => {
                    let Some((_, Wait::CostDiscard(wait))) = &mut execution.waiting else {
                        return Err(invalid("missing resolution hand cost question"));
                    };
                    wait.resolution_payment = Some(Box::new(payment));
                    execution.question_stage = Some(QuestionStage::Pay);
                    return Ok(());
                }
                Ok(false) => return Err(invalid("missing resolution hand cost")),
                Err(crate::EngineFailure::IllegalPlay(_)) => {
                    return self.resolve_internal_payment(
                        execution,
                        &payment.costs,
                        payment.on_paid,
                        payment.receipt,
                        false,
                    );
                }
                Err(error) => return Err(error),
            }
        }
        self.resolve_internal_payment(
            execution,
            &payment.costs,
            payment.on_paid,
            payment.receipt,
            accepted,
        )
    }

    pub(in crate::game::internal) fn resolve_internal_payment(
        &mut self,
        execution: &mut Execution,
        costs: &CostSpec,
        on_paid: Program,
        receipt: ReceiptId,
        accepted: bool,
    ) -> Result<()> {
        let mut complete = false;
        let mut receipts = vec![];
        if accepted {
            let mut probe = self.clone();
            let mut candidate = execution.clone();
            let previous = probe.card_runtime.active_source;
            probe.card_runtime.active_source = execution.ability;
            match probe.internal_card_payment(
                &mut candidate,
                costs,
                false,
                Some(CostPaymentPurpose::Resolution { receipt }),
            ) {
                Ok(()) => {
                    probe.card_runtime.active_source = previous;
                    *self = probe;
                    *execution = candidate;
                    complete = true;
                    let CostSpec::Ordered { steps } = costs else {
                        return Err(unsupported("resolution costs"));
                    };
                    receipts.extend(steps.iter().map(|step| RuntimeReceiptId {
                        execution: execution.id,
                        slot: step.instruction.receipt,
                    }));
                    execution.prepend(on_paid);
                }
                Err(crate::EngineFailure::IllegalPlay(_)) => {}
                Err(error) => return Err(error),
            }
        }
        execution.receipts.insert(
            receipt,
            InstructionOutcome::ExchangePayment(PaymentResult {
                complete: Bool(complete),
                steps: receipts,
            }),
        );
        Ok(())
    }
    pub(super) fn decline_internal_head_payment(
        &mut self,
        actor: PlayerId,
        token: DecisionToken,
        answer: &Answer,
    ) -> Result<bool> {
        let Some(execution) = self.typed.as_ref() else {
            return Ok(false);
        };
        if !execution.declines_required_head(answer) {
            return Ok(false);
        }
        let Some((view, _)) = &execution.waiting else {
            return Err(invalid("missing head payment question"));
        };
        if view.actor != actor || view.token != token {
            return Err(invalid("wrong actor or stale head payment answer"));
        }
        let id = execution
            .trigger_occurrence
            .ok_or_else(|| invalid("head payment without occurrence"))?;
        let checkpoint = execution.id;
        let origin = execution
            .origin
            .as_ref()
            .ok_or_else(|| invalid("head payment without rollback origin"))?
            .as_ref()
            .clone();
        let published = self.published_knowledge();
        self.restore_player_state(origin, &published)?;
        self.internal_cancel_occurrence(id, checkpoint);
        Ok(true)
    }

    pub(super) fn internal_optional_decision(
        &mut self,
        execution: &mut Execution,
        play: &mut PlayState,
    ) -> Result<bool> {
        let Some(payment) = play
            .optional
            .iter_mut()
            .find(|payment| payment.accepted.is_none())
        else {
            return Ok(false);
        };
        if execution.spell
            && !payment.required
            && super::hand_payment::single_additional(&payment.costs)
        {
            // A standalone discard can only use the hand present at declaration.
            let mut probe = execution.clone();
            match self.internal_probe_hand_payments(&mut probe, &[&payment.costs]) {
                Ok(()) => {}
                Err(crate::EngineFailure::IllegalPlay(_)) => {
                    // Only player answers are recorded as declarations.
                    payment.accepted = Some(false);
                    return self.internal_optional_decision(execution, play);
                }
                Err(error) => return Err(error),
            }
        }
        self.internal_wait(
            execution,
            DecisionId(payment.receipt.0),
            execution.context.controller,
            InputDomain::OptionalCost,
            Wait::OptionalCost,
        )?;
        Ok(true)
    }

    pub(super) fn internal_optional_payment(
        &mut self,
        execution: &mut Execution,
        play: &PlayState,
    ) -> Result<()> {
        for payment in &play.optional {
            let complete = payment
                .accepted
                .ok_or_else(|| invalid("missing optional declaration"))?;
            let mut receipts = vec![];
            if complete {
                self.internal_card_payment(
                    execution,
                    &payment.costs,
                    false,
                    Some(CostPaymentPurpose::Optional {
                        receipt: payment.receipt,
                        required: payment.required,
                    }),
                )?;
                let CostSpec::Ordered { steps } = &payment.costs else {
                    return Err(unsupported("optional costs"));
                };
                receipts.extend(steps.iter().map(|step| RuntimeReceiptId {
                    execution: execution.id,
                    slot: step.instruction.receipt,
                }));
            }
            execution.receipts.insert(
                payment.receipt,
                InstructionOutcome::ExchangePayment(PaymentResult {
                    complete: Bool(complete),
                    steps: receipts,
                }),
            );
        }
        Ok(())
    }
}

impl Execution {
    pub(super) fn declines_required_head(&self, answer: &Answer) -> bool {
        self.nested.is_none()
            && matches!(answer, Answer::OptionalCost(false))
            && matches!(self.waiting, Some((_, Wait::OptionalCost)))
            && self
                .play
                .as_ref()
                .is_some_and(|play| play.optional.iter().any(|payment| payment.required))
    }
}
