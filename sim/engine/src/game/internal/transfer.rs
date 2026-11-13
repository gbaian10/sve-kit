//! Selected followers change controller without leaving the field.
use super::{Execution, Game, InstructionOutcome, Result, seat_name, unsupported};
use crate::ir::perspective::readable;
use crate::ir::{Action, Instruction, ObjectOperand, PlayerRef, PlayerRelation, Selector};

/// The recipient is the effect controller, a procedure's player, or the one opponent of either.
fn recipient(player: &PlayerRef) -> bool {
    match player {
        PlayerRef::Controller {} | PlayerRef::Bound { .. } => true,
        PlayerRef::RequireOne { players } => {
            players.relation == PlayerRelation::Opponents && readable(&players.relative_to)
        }
        PlayerRef::FromExpression { .. }
        | PlayerRef::Owner {}
        | PlayerRef::Active {}
        | PlayerRef::ProposedDamageRecipient {}
        | PlayerRef::ProposedDamageController {} => false,
    }
}

pub(super) fn check_action(action: &Action) -> Result<()> {
    if let Action::Transfer {
        objects,
        destination_controller,
    } = action
        && (!(matches!(objects, Selector::Objects { objects } if matches!(objects.as_slice(), [ObjectOperand::Selected { .. }]))
            || matches!(objects, Selector::Bound { .. }))
            || !recipient(destination_controller))
    {
        return Err(unsupported("selected follower transfer"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Control changes use the authoritative field mutation."
)]
impl Game {
    pub(super) fn internal_transfer(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check_action(&instruction.action)?;
        let Action::Transfer {
            objects,
            destination_controller,
        } = &instruction.action
        else {
            return Err(unsupported("transfer instruction"));
        };
        let members = execution.objects(self, objects)?;
        // The recipient and its free field slot are read when each object moves (Q1766).
        let recipient = execution.player(self, destination_controller)?;
        let controller = seat_name(recipient)?;
        let mut transferred = Vec::new();
        for member in members {
            let id = execution.object_id(member)?;
            let object = self.object(id)?;
            if object.generation != member.generation.0
                || object.zone != "field"
                || object.controller == controller
                || !self.object_type(id)?.contains("フォロワー")
            {
                continue;
            }
            if self.zone_count(controller, "field") >= 5 {
                continue;
            }
            let previous_controller = object.controller.clone();
            let group = self.group();
            self.transfer_field_controller(id, controller, &execution.frame().cause, group)?;
            self.clean_equipment_rules()?;
            execution.record_transferred_controller(member, &previous_controller, controller);
            transferred.push(member);
        }
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Transferred(transferred),
        );
        Ok(())
    }
}
