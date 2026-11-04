//! Optional declaration is separate from its all-or-nothing payment.
use super::play::PlayState;
use super::{AbilitySource, Execution, Game, Result, invalid, unsupported};
use crate::game::internal::{Answer, DecisionToken, InputDomain, InstructionOutcome, Wait};
use crate::ir::reveal_cost as reveal_payment;
use crate::ir::{
    AbilityKind, AbilityOrigin, Action, Bool, Control, CostSpec, DecisionId, Expr,
    InstructionContext, Int, Node, PaymentOptionality, PaymentResult, PaymentTime, PlayerId,
    PlayerRef, Program, ReceiptId, Resource, ResourceOperation, RuntimeReceiptId, Selector,
    WellFormedCardProgram, ZoneKind, optional_payment,
};
use core::slice::from_ref;

#[derive(Clone, Debug)]
pub(super) struct OptionalState {
    pub(super) costs: CostSpec,
    pub(super) required: bool,
    pub(super) receipt: ReceiptId,
    pub(super) accepted: Option<bool>,
}

pub(super) fn check_costs(control: &Control, origin: &AbilitySource) -> Result<()> {
    let Control::Pay {
        costs, optional, ..
    } = control
    else {
        return Err(unsupported("optional payment control"));
    };
    let required = *optional == PaymentOptionality::Required;
    let CostSpec::Ordered { steps } = costs else {
        return Err(unsupported("optional cost alternatives"));
    };
    if steps.is_empty() {
        return Err(unsupported("empty optional payment"));
    }
    for step in steps {
        let instruction = &step.instruction;
        if step.materials.is_some()
            || instruction.precondition.is_some()
            || instruction.source != *origin
            || instruction.actor != (PlayerRef::Controller {})
            || instruction.context != InstructionContext::Cost
            || !(matches!(&instruction.action, Action::Resource { player: PlayerRef::Controller {},
            resource: Resource::PpCurrent, operation: ResourceOperation::Pay,
            amount: Expr::Literal(Int(amount)) } if *amount >= 0)
                || (required && super::super::life::is_cost(&instruction.action))
                || (required
                    && matches!(
                        instruction.action,
                        Action::Reveal {
                            objects: Selector::Bound { .. }
                        }
                    ))
                || super::hand_payment::check_cost(&instruction.action)
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
                && (ability.node.kind != AbilityKind::Triggered
                    || !matches!(ability.node.source.origin, AbilityOrigin::Printed { .. })
                    || ability.node.active_zones != [ZoneKind::Field])
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
                    if super::material::source_field_cost(&step.instruction.action) {
                        return Err(unsupported("source costs require a resolution payment"));
                    }
                    let supported = matches!(step.instruction.action, Action::Resource { .. })
                        || (optional_payment::required(body)
                            && super::super::life::is_cost(&step.instruction.action))
                        || super::hand_payment::check_cost(&step.instruction.action)
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
            Node::Control {
                control: Control::Sequence { body } | Control::May { body, .. },
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
            control: Control::Sequence { body } | Control::May { body, .. },
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

pub(in crate::game::internal) fn accept(execution: &mut Execution, accepted: bool) -> Result<bool> {
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
    Ok(true)
}

pub(in crate::game::internal) fn check_control(control: &Control) -> Result<()> {
    if let Control::Pay {
        at: PaymentTime::ResolutionChoice,
        costs: CostSpec::Ordered { steps },
        optional: PaymentOptionality::Optional,
        ..
    } = control
    {
        if !steps.is_empty()
            && steps
                .iter()
                .all(|step| super::material::source_field_cost(&step.instruction.action))
        {
            return Ok(());
        }
        return Err(unsupported(
            "resolution payment only supports source field costs",
        ));
    }
    if !matches!(control, Control::Pay { at: PaymentTime::PlayParameter,
        optional, on_paid, .. } if on_paid.steps.is_empty() || *optional == PaymentOptionality::Optional)
    {
        return Err(unsupported("optional payment timing or continuation"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Declaration and payment share the play transaction."
)]
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
            ..
        } = control
        else {
            return Err(unsupported("resolution payment timing"));
        };
        self.internal_wait(
            execution,
            DecisionId(receipt.0),
            execution.context.controller,
            InputDomain::OptionalCost,
            Wait::ResolutionPayment {
                costs,
                on_paid,
                receipt,
            },
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
            match probe.internal_card_payment(&mut candidate, costs, false) {
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
        if execution.nested.is_some() {
            return Ok(false);
        }
        let Some((view, Wait::OptionalCost)) = &execution.waiting else {
            return Ok(false);
        };
        if !matches!(answer, Answer::OptionalCost(false))
            || !execution
                .play
                .as_ref()
                .and_then(|play| play.optional.iter().find(|payment| payment.required))
                .is_some_and(|payment| payment.required)
        {
            return Ok(false);
        }
        if view.actor != actor || view.token != token {
            return Err(invalid("wrong actor or stale head payment answer"));
        }
        let id = execution
            .trigger_occurrence
            .ok_or_else(|| invalid("head payment without occurrence"))?;
        self.internal_cancel_occurrence(id, execution.id);
        Ok(true)
    }

    pub(super) fn internal_optional_decision(
        &mut self,
        execution: &mut Execution,
        play: &PlayState,
    ) -> Result<bool> {
        let Some(payment) = play
            .optional
            .iter()
            .find(|payment| payment.accepted.is_none())
        else {
            return Ok(false);
        };
        if payment.accepted.is_some() {
            return Ok(false);
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
                self.internal_card_payment(execution, &payment.costs, false)?;
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
