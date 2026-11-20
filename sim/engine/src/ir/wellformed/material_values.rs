use super::{Checked, Checker, WellFormedReason};
use crate::ir::{
    Action, Cardinality, ChoiceDomain, Decision, DecisionKind, Instruction, InstructionContext,
    MoveDestination, MoveReason, MovementReceipt, Nat, NumericProperty, ObjectOperand, OwnerZone,
    PlaySlot, PlayerRef, PlayerRelation, Posture, Selector, Visibility, ZoneKind, receipt_reads,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Paid material reads share the closed program receipt environment."
)]
impl Checker {
    pub(super) fn record_field_material(&mut self, decision: &Decision) {
        if !matches!(
            decision.kind,
            DecisionKind::PlayParameter {
                slot: PlaySlot::CostMaterials,
                ..
            }
        ) {
            return;
        }
        let ChoiceDomain::Objects {
            constraint,
            binding,
            target_roles: None,
        } = &decision.domain
        else {
            return;
        };
        let [group] = constraint.groups.as_slice() else {
            return;
        };
        if decision.actor == (PlayerRef::Controller {})
            && decision.visibility == (Visibility::Public {})
            && constraint.cardinality == group.cardinality
            && (constraint.cardinality
                == (Cardinality {
                    minimum: Nat(0),
                    maximum: None,
                })
                || (constraint.cardinality.minimum.0 > 0
                    && constraint.cardinality.maximum == Some(constraint.cardinality.minimum)))
            && own_zone(&group.selector, ZoneKind::Field)
        {
            self.env.field_act_materials.insert(binding.key());
        }
        let single = Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        };
        if constraint.cardinality == single && group.cardinality == single {
            if own_zone(&group.selector, ZoneKind::Field) {
                self.env.field_materials.insert(binding.key());
            }
            if own_zone(&group.selector, ZoneKind::Cemetery) {
                self.env.cemetery_materials.insert(binding.key());
            }
        }
    }

    pub(super) fn material_act_cost(
        &self,
        instruction: &Instruction,
        context: InstructionContext,
    ) -> bool {
        context == InstructionContext::Cost
            && instruction.precondition.is_none()
            && instruction.actor == (PlayerRef::Controller {})
            && matches!(&instruction.action, Action::Posture { objects: Selector::Bound { reference }, posture: Posture::Acted }
                if self.env.field_act_materials.contains(&reference.key()))
    }

    pub(super) fn record_material_move(
        &mut self,
        instruction: &Instruction,
        context: InstructionContext,
    ) {
        if context != InstructionContext::Cost || instruction.precondition.is_some() {
            return;
        }
        if matches!(
            &instruction.action,
            Action::Reveal {
                objects: Selector::Bound { .. }
            } | Action::Banish {
                objects: Selector::Bound { .. }
            }
        ) {
            self.env.name_payments.insert(instruction.receipt);
        }
        if matches!(&instruction.action, Action::Banish { objects: Selector::Bound { reference } }
            if self.env.cemetery_materials.contains(&reference.key()))
        {
            self.env.single_material_moves.insert(instruction.receipt);
        }
        if let Action::Move {
            objects: Selector::Bound { reference },
            movement,
            reason: MoveReason::Put,
        } = &instruction.action
            && self.env.field_materials.contains(&reference.key())
            && movement.to
                == (MoveDestination::Owners {
                    zone: OwnerZone::Cemetery,
                })
            && movement.position.is_none()
            && movement.order.is_none()
            && movement.initial_posture.is_none()
            && movement.entry_effects.is_empty()
        {
            self.env.single_material_moves.insert(instruction.receipt);
        }
    }

    pub(super) fn paid_material_number(
        &self,
        receipt: &MovementReceipt,
        property: NumericProperty,
    ) -> Checked {
        if !matches!(
            (receipt, property),
            (
                MovementReceipt::Move { .. },
                NumericProperty::Attack | NumericProperty::Cost | NumericProperty::OriginalCost
            ) | (MovementReceipt::Banish { .. }, NumericProperty::Attack)
        ) {
            return self.reject();
        }
        self.movement_receipt(receipt)?;
        if receipt_reads::paid_movement_id(receipt)
            .is_some_and(|id| self.env.single_material_moves.contains(&id))
        {
            Ok(())
        } else {
            Err(self.error(WellFormedReason::UnavailableReceipt))
        }
    }
}

fn own_zone(selector: &Selector, zone: ZoneKind) -> bool {
    if let Selector::Difference { input, except } = selector {
        return matches!(except.as_ref(), Selector::Objects { objects } if objects == &[ObjectOperand::SourceHolder {}])
            && own_zone(input, zone);
    }
    matches!(selector, Selector::Zone { players, zone: selected, .. }
        if zone == *selected && players.relation == PlayerRelation::SelfPlayer
            && players.relative_to == (PlayerRef::Controller {}))
}
