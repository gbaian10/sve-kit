use super::{Checked, Checker, WellFormedReason};
use crate::ir::exchange as exchange_shape;
use crate::ir::reveal_cost as reveal_payment;
use crate::ir::{
    Action, Binding, Bool, ChoiceDomain, CostChange, CostSpec, CostStep, CounterOperation,
    Decision, DecisionKind, DiscardMode, Disjointness, ExecutionRef, Expr, Holder, InputMode,
    InstructionContext, Int, KeywordId, MoveDestination, MoveReason, Nat, ObjectOperand,
    ObjectProperty, OwnerZone, PlayCostExchange, PlayCostExchangeChange, PlayCostPayment, PlaySlot,
    PlayerRef, PlayerRelation, PlayerSet, Posture, SelectionSnapshot, SelectionView, Selector,
    Visibility, ZoneKind,
};
use crate::ir::{flip_cost, material_sets};

impl Checker {
    pub(super) fn additional(&mut self, payment: &PlayCostPayment) -> Checked {
        if !self.scopes.insert(payment.scope) {
            return Err(self.error(WellFormedReason::DuplicateScope));
        }
        if !self.receipts.insert(payment.receipt) {
            return Err(self.error(WellFormedReason::DuplicateReceipt));
        }
        if payment.play.limit.is_some()
            || !payment.play.captures.is_empty()
            || !matches!(&payment.play.costs, CostSpec::Ordered { steps } if !steps.is_empty())
        {
            return self.reject();
        }
        self.isolated(|checker| {
            checker.env.scope = payment.scope;
            checker.play_entry = Some(checker.env.clone());
            if reveal_payment::payment(&payment.play) {
                for decision in &payment.play.parameters {
                    checker.reveal_cost_decision(decision)?;
                }
                let CostSpec::Ordered { steps } = &payment.play.costs else {
                    return checker.reject();
                };
                for step in steps {
                    checker.instruction_in(&step.instruction, InstructionContext::Cost, false)?;
                }
                return Ok(());
            }
            for decision in &payment.play.parameters {
                checker.play_decision(decision, &PlayerRef::Controller {})?;
            }
            checker.play_costs(&payment.play.costs)
        })
    }

    pub(super) fn exchange(&mut self, exchange: &PlayCostExchange, has_set: &mut bool) -> Checked {
        match &exchange.change {
            PlayCostExchangeChange::Adjust(CostChange::Delta {
                amount: Expr::Literal(Int(value)),
            }) if *value <= 0 || exchange_shape::pp_increase(exchange) => {}
            PlayCostExchangeChange::Adjust(CostChange::Set {
                amount: Expr::Literal(Int(value)),
            }) if *value >= 0 && !*has_set => *has_set = true,
            PlayCostExchangeChange::ReplaceOriginal { condition, .. } => {
                self.isolated(|checker| checker.boolean(condition))?;
            }
            PlayCostExchangeChange::Adjust(_) => return self.reject(),
        }
        if !self.scopes.insert(exchange.scope) {
            return Err(self.error(WellFormedReason::DuplicateScope));
        }
        if !self.decisions.insert(exchange.decision) {
            return Err(self.error(WellFormedReason::DuplicateDecision));
        }
        if !self.receipts.insert(exchange.receipt) {
            return Err(self.error(WellFormedReason::DuplicateReceipt));
        }
        if let Some(route) = exchange.route
            && !self.decisions.insert(route)
        {
            return Err(self.error(WellFormedReason::DuplicateDecision));
        }
        self.isolated(|checker| checker.exchange_materials(exchange))
    }

    fn exchange_materials(&mut self, exchange: &PlayCostExchange) -> Checked {
        let Some(routes) = exchange.routes() else {
            return self.reject();
        };
        if routes.len() == 2 && !exchange_shape::pair(exchange) {
            return self.reject();
        }
        if let [(decision, _)] = routes.as_slice()
            && matches!(&decision.domain, ChoiceDomain::Objects { constraint, .. } if constraint.groups.iter().any(|group| matches!(group.selector, Selector::Zone { zone: ZoneKind::Ex, .. })))
        {
            return self.reject();
        }

        self.env.scope = exchange.scope;
        for (decision, step) in routes {
            self.exchange_route(decision, step, exchange.route.is_some())?;
        }
        Ok(())
    }

    fn exchange_route(&mut self, decision: &Decision, step: &CostStep, multiple: bool) -> Checked {
        let checker = self;
        let ChoiceDomain::Objects {
            constraint,
            binding,
            target_roles: None,
        } = &decision.domain
        else {
            return checker.reject();
        };
        let grouped = material_sets::grouped_cemetery(constraint)
            && matches!(step.instruction.action, Action::Banish { .. });
        let Some(group) = constraint.groups.first() else {
            return checker.reject();
        };
        if !grouped && (constraint.groups.len() != 1 || group.cardinality != constraint.cardinality)
        {
            return checker.reject();
        }
        let Selector::Zone {
            players,
            zone,
            view: SelectionView::Current {},
            ..
        } = &group.selector
        else {
            return checker.reject();
        };
        if players.relative_to != (PlayerRef::Controller {})
            || players.relation != PlayerRelation::SelfPlayer
            || !matches!(
                zone,
                ZoneKind::Field
                    | ZoneKind::Cemetery
                    | ZoneKind::Hand
                    | ZoneKind::Ex
                    | ZoneKind::Evolve
            )
            || constraint.cardinality.minimum.0 == 0
            || constraint.cardinality.maximum != Some(constraint.cardinality.minimum)
            || constraint.disjoint != Disjointness::Disjoint
            || (constraint.distinct_by.is_some()
                && (constraint.distinct_by != Some(ObjectProperty::Cost)
                    || multiple
                    || *zone != ZoneKind::Cemetery))
            || !constraint.aggregate_bounds.is_empty()
            || !decision.captures.is_empty()
            || decision.actor != (PlayerRef::Controller {})
            || decision.visibility != material_visibility(decision, *zone, players)
            || decision.kind
                != (DecisionKind::PlayParameter {
                    slot: PlaySlot::CostMaterials,
                    selection_snapshot: SelectionSnapshot::AtDecision,
                    play_scope: ExecutionRef::Current {},
                    input: InputMode::Inline,
                })
        {
            return checker.reject();
        }
        checker.at("materials", |checker| {
            checker.decision_header(decision)?;
            checker.answer(decision, None)
        })?;
        let selected = Selector::Bound {
            reference: *binding,
        };
        let counter = selected_counter(step, *binding, *zone, constraint.cardinality.minimum);
        let valid = flip_cost::payment(&step.instruction.action, decision)
            || counter
            || matches!(&step.instruction.action, Action::Reveal { objects }
                if *zone == ZoneKind::Hand && *objects == selected)
            || matches!(&step.instruction.action,
            Action::Move { objects, movement, reason: MoveReason::Put }
                if *zone == ZoneKind::Field && *objects == selected
                    && movement.to == (MoveDestination::Owners { zone: OwnerZone::Cemetery })
                    && movement.position.is_none() && movement.order.is_none()
                    && movement.initial_posture.is_none() && movement.entry_effects.is_empty())
            || matches!(&step.instruction.action, Action::Banish { objects }
                if matches!(zone,ZoneKind::Cemetery|ZoneKind::Ex) && *objects == selected)
            || matches!(&step.instruction.action, Action::Posture { objects, posture: Posture::Acted }
                if *zone == ZoneKind::Field && *objects == selected)
            || matches!(&step.instruction.action, Action::Discard { player: PlayerRef::Controller {}, mode: DiscardMode::All { eligible } }
                if *zone == ZoneKind::Hand && matches!(eligible, Selector::Intersection { sets }
                    if matches!(sets.as_slice(), [Selector::Zone {players: hand_players, zone: ZoneKind::Hand, view: SelectionView::Current {}, predicate, ..}, other]
                        if hand_players == players && **predicate == Expr::Literal(Bool(true)) && *other == selected)));
        if !valid
            || (step.keyword.is_some() && !counter)
            || step.materials.is_some()
            || step.instruction.precondition.is_some()
            || step.instruction.actor != (PlayerRef::Controller {})
        {
            return checker.reject();
        }
        checker.at("cost", |checker| {
            checker.instruction_in(&step.instruction, InstructionContext::Cost, false)
        })
    }
}

fn selected_counter(
    step: &CostStep,
    binding: Binding<Selector>,
    zone: ZoneKind,
    count: Nat,
) -> bool {
    matches!(&step.instruction.action,
    Action::Counter { holder: Holder::Object { object: ObjectOperand::Selected { selection } },
        kind, operation: CounterOperation::Remove {}, amount: Expr::Literal(Nat(amount)) }
        if *selection == binding && !kind.0.is_empty() && *amount > 0
            && zone == ZoneKind::Field && count == Nat(1)
            && match step.keyword {
                None => kind.0 != "stack_counter",
                Some(KeywordId::EarthRite) => kind.0 == "stack_counter",
                Some(_) => false,
            })
}

fn material_visibility(decision: &Decision, zone: ZoneKind, players: &PlayerSet) -> Visibility {
    if zone == ZoneKind::Hand
        || flip_cost::material(decision).is_some_and(|(_, before_up)| !before_up)
    {
        Visibility::Only {
            players: players.clone(),
        }
    } else {
        Visibility::Public {}
    }
}
