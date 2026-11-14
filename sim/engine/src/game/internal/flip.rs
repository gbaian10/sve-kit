//! Face-down effects reuse the ordinary flip mutation and revoke public location knowledge.

use crate::ir::public_evolve;

use super::{Execution, Game, InstructionOutcome, Result, invalid, seat_name, unsupported};
use crate::ir::{
    Action, FlipOutcome, Instruction, ResultMember, ResultMemberId, RuntimeObjectView, Selector,
    SnapshotId,
};

pub(super) fn check_action(action: &Action) -> Result<()> {
    if matches!(action, Action::FlipDown { objects } if !matches!(objects, Selector::Bound { .. }) && !public_evolve::selector(objects))
    {
        return Err(unsupported(
            "flip-down requires selected public evolve-deck cards",
        ));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Flip instructions share the authoritative Game state."
)]
impl Game {
    pub(super) fn internal_flip_down(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        objects: &Selector,
    ) -> Result<()> {
        check_action(&instruction.action)?;
        let mut planned = Vec::new();
        for member in execution.objects(self, objects)? {
            let id = execution.object_id(member)?;
            let object = self.object(id)?;
            if object.generation != member.generation.0 {
                continue;
            }
            if object.zone != "evolve_deck"
                || object.controller != seat_name(execution.context.controller)?
                || object.owner != object.controller
            {
                return Err(unsupported(
                    "flip-down requires the controller's evolve deck",
                ));
            }
            if object.state["face_up"] == true {
                planned.push((id.to_owned(), member));
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
            self.change_face_up(&id, false, &execution.frame().cause)?;
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
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Flip(FlipOutcome { successful }),
        );
        Ok(())
    }
}
