use super::{Checked, Checker, WellFormedReason};
use crate::ir::exchange as exchange_shape;
use crate::ir::reveal_cost as reveal_payment;
use crate::ir::{
    Action, Bool, ChoiceDomain, CostChange, CostSpec, CostStep, Decision, DecisionKind,
    DiscardMode, Disjointness, ExecutionRef, Expr, InputMode, InstructionContext, Int,
    MoveDestination, MoveReason, OwnerZone, PlayCostExchange, PlayCostPayment, PlaySlot, PlayerRef,
    PlayerRelation, Posture, SelectionSnapshot, SelectionView, Selector, Visibility, ZoneKind,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Exchange has a closed cost entry separate from general payments."
)]
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
            CostChange::Delta {
                amount: Expr::Literal(Int(value)),
            } if *value <= 0 => {}
            CostChange::Set {
                amount: Expr::Literal(Int(value)),
            } if *value >= 0 && !*has_set => *has_set = true,
            CostChange::Delta { .. } | CostChange::Set { .. } => return self.reject(),
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
            self.exchange_route(decision, step)?;
        }
        Ok(())
    }

    fn exchange_route(&mut self, decision: &Decision, step: &CostStep) -> Checked {
        let checker = self;
        let ChoiceDomain::Objects {
            constraint,
            binding,
            target_roles: None,
        } = &decision.domain
        else {
            return checker.reject();
        };
        let [group] = constraint.groups.as_slice() else {
            return checker.reject();
        };
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
                ZoneKind::Field | ZoneKind::Cemetery | ZoneKind::Hand | ZoneKind::Ex
            )
            || constraint.cardinality.minimum.0 == 0
            || constraint.cardinality.maximum != Some(constraint.cardinality.minimum)
            || group.cardinality != constraint.cardinality
            || constraint.disjoint != Disjointness::Disjoint
            || constraint.distinct_by.is_some()
            || !constraint.aggregate_bounds.is_empty()
            || !decision.captures.is_empty()
            || decision.actor != (PlayerRef::Controller {})
            || decision.visibility
                != if *zone == ZoneKind::Hand {
                    Visibility::Only {
                        players: players.clone(),
                    }
                } else {
                    Visibility::Public {}
                }
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
        let valid = matches!(&step.instruction.action,
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
            || step.keyword.is_some()
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
