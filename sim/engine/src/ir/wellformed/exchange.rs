use super::{Checked, Checker, WellFormedReason};
use crate::ir::{
    Action, ChoiceDomain, CostChange, CostSpec, DecisionKind, Disjointness, ExecutionRef, Expr,
    InputMode, InstructionContext, Int, MoveDestination, MoveReason, OwnerZone, PlayCostExchange,
    PlaySlot, PlayerRef, PlayerRelation, Posture, SelectionSnapshot, SelectionView, Selector,
    Visibility, ZoneKind,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Exchange has a closed cost entry separate from general payments."
)]
impl Checker {
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
        self.isolated(|checker| checker.exchange_materials(exchange))
    }

    fn exchange_materials(&mut self, exchange: &PlayCostExchange) -> Checked {
        let checker = self;
        checker.env.scope = exchange.scope;
        let decision = &exchange.materials;
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
            || !matches!(zone, ZoneKind::Field | ZoneKind::Cemetery)
            || constraint.cardinality.minimum.0 == 0
            || constraint.cardinality.maximum != Some(constraint.cardinality.minimum)
            || group.cardinality != constraint.cardinality
            || constraint.disjoint != Disjointness::Disjoint
            || constraint.distinct_by.is_some()
            || !constraint.aggregate_bounds.is_empty()
            || !decision.captures.is_empty()
            || decision.actor != (PlayerRef::Controller {})
            || decision.visibility != (Visibility::Public {})
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
        let CostSpec::Ordered { steps } = &exchange.cost else {
            return checker.reject();
        };
        let [step] = steps.as_slice() else {
            return checker.reject();
        };
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
                if *zone == ZoneKind::Cemetery && *objects == selected)
            || matches!(&step.instruction.action, Action::Posture { objects, posture: Posture::Acted }
                if *zone == ZoneKind::Field && *objects == selected);
        if !valid
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
