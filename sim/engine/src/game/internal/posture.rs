//! Body posture changes share the ordinary mutation and emit facts only for changed members.

use super::{Execution, Fact, Game, InstructionOutcome, Result, invalid, unsupported};
use crate::ir::{Instruction, Posture, PostureEvent, Selector};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Posture instructions mutate the same authoritative Game."
)]
impl Game {
    pub(super) fn internal_posture(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        objects: &Selector,
        posture: Posture,
    ) -> Result<()> {
        let members = execution.objects(self, objects)?;
        let acted = posture == Posture::Acted;
        let mut planned = Vec::new();
        for member in members {
            let id = execution.object_id(member)?;
            let object = self.object(id)?;
            if object.generation != member.generation.0 {
                continue;
            }
            if object.zone != "field" {
                return Err(unsupported("body posture requires a public field object"));
            }
            let before = object
                .state
                .get("acted")
                .and_then(serde_json::Value::as_bool)
                .ok_or_else(|| invalid("invalid field posture"))?;
            if before != acted {
                planned.push((
                    id.to_owned(),
                    PostureEvent {
                        object: member,
                        before: if acted {
                            Posture::Standing
                        } else {
                            Posture::Acted
                        },
                        after: posture,
                    },
                ));
            }
        }
        let ids = planned.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>();
        self.change_posture(&ids, acted, &execution.frame().cause)?;
        let changes = planned
            .into_iter()
            .map(|(_, event)| event)
            .collect::<Vec<_>>();
        execution
            .facts
            .extend(changes.iter().copied().map(Fact::Posture));
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Posture(changes));
        Ok(())
    }
}
