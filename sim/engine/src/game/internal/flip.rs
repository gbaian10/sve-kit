//! Both flip directions share mutation and the corresponding public-location knowledge change.
use super::{Execution, Game, InstructionOutcome, Result, invalid, seat_name, unsupported};
use crate::ir::{
    Action, FlipOutcome, Instruction, ObjectRef, ResultMember, ResultMemberId, RuntimeObjectView,
    Selector, SnapshotId, public_evolve,
};

pub(super) fn check_action(action: &Action) -> Result<()> {
    match action {
        Action::FlipDown { objects }
            if !matches!(objects, Selector::Bound { .. }) && !public_evolve::selector(objects) =>
        {
            Err(unsupported(
                "flip-down requires selected public evolve-deck cards",
            ))
        }
        Action::FlipUp { objects, faces }
            if !faces.is_empty() || !matches!(objects, Selector::Bound { .. }) =>
        {
            Err(unsupported("flip-up requires bound evolve-deck materials"))
        }
        _ => Ok(()),
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Flip instructions and payments share the authoritative Game state."
)]
impl Game {
    pub(super) fn internal_flip_down(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        objects: &Selector,
    ) -> Result<()> {
        check_action(&instruction.action)?;
        let members = execution.objects(self, objects)?;
        let outcome = self.internal_flip_members(execution, &members, false, false)?;
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Flip(outcome));
        Ok(())
    }

    pub(super) fn internal_flip_up(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        objects: &Selector,
    ) -> Result<()> {
        check_action(&instruction.action)?;
        let members = execution.objects(self, objects)?;
        let outcome = self.internal_flip_members(execution, &members, true, false)?;
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Flip(outcome));
        Ok(())
    }

    pub(in crate::game::internal) fn internal_flip_members(
        &mut self,
        execution: &Execution,
        members: &[ObjectRef],
        up: bool,
        strict: bool,
    ) -> Result<FlipOutcome> {
        let controller = seat_name(execution.context.controller)?;
        let mut planned = Vec::new();
        for member in members {
            let id = execution.object_id(*member)?;
            let object = self.object(id)?;
            let current = object.generation == member.generation.0;
            let owned = object.zone == "evolve_deck"
                && object.controller == controller
                && object.owner == controller;
            if strict
                && (!current
                    || !owned
                    || object.state["face_up"] == up
                    || self.object_type(id)? != "フォロワー・エボルヴ")
            {
                return Err(crate::EngineFailure::IllegalPlay(
                    "flip materials changed".into(),
                ));
            }
            if !current {
                continue;
            }
            if !owned {
                return Err(unsupported("flip requires the controller's evolve deck"));
            }
            if object.state["face_up"] != up
                && (!up || self.object_type(id)? == "フォロワー・エボルヴ")
            {
                planned.push((id.to_owned(), *member));
            }
        }
        let before = SnapshotId(self.state.ir_epoch);
        self.state.ir_epoch = self
            .state
            .ir_epoch
            .checked_add(1)
            .ok_or_else(|| invalid("IR epoch exhausted"))?;
        let after = SnapshotId(self.state.ir_epoch);
        let mut successful = Vec::new();
        for (index, (id, member)) in planned.into_iter().enumerate() {
            self.change_face_up(&id, up, &execution.frame().cause)?;
            successful.push(ResultMember {
                id: ResultMemberId(u64::try_from(index).map_err(invalid)?),
                before: Some(RuntimeObjectView {
                    object: member,
                    snapshot: before,
                }),
                after: Some(RuntimeObjectView {
                    object: member,
                    snapshot: after,
                }),
            });
        }
        Ok(FlipOutcome { successful })
    }
}
