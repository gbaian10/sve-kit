use super::{Checked, Checker, WellFormedReason};
use crate::ir::{
    Action, Cardinality, ChoiceDomain, Decision, DecisionKind, Instruction, InstructionContext,
    MoveDestination, MoveOutcome, MoveReason, Nat, ObjectOperand, OwnerZone, PlaySlot, PlayerRef,
    PlayerRelation, Posture, ReceiptLocation, ReceiptReference, Selector, Visibility, ZoneKind,
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
            && own_field(&group.selector)
        {
            self.env.field_act_materials.insert(binding.key());
        }
        let single = Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        };
        if constraint.cardinality == single
            && group.cardinality == single
            && own_field(&group.selector)
        {
            self.env.field_materials.insert(binding.key());
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

    pub(super) fn paid_material_attack(&self, receipt: &ReceiptReference<MoveOutcome>) -> Checked {
        self.receipt(receipt)?;
        if matches!(receipt.location(), ReceiptLocation::Local { id } if self.env.single_material_moves.contains(id))
        {
            Ok(())
        } else {
            Err(self.error(WellFormedReason::UnavailableReceipt))
        }
    }
}

fn own_field(selector: &Selector) -> bool {
    if let Selector::Difference { input, except } = selector {
        return matches!(except.as_ref(), Selector::Objects { objects } if objects == &[ObjectOperand::SourceHolder {}])
            && own_field(input);
    }
    matches!(selector, Selector::Zone { players, zone: ZoneKind::Field, .. }
        if players.relation == PlayerRelation::SelfPlayer && players.relative_to == (PlayerRef::Controller {}))
}
