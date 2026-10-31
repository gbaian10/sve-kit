//! Body posture changes share the ordinary mutation and emit facts only for changed members.

use super::{Execution, Fact, Game, InstructionOutcome, Result, invalid, unsupported};
use crate::ir::{
    Action, ActionPattern, Bool, DurationEnd, Edge, Effect, Expr, InstallationSubjects,
    InstallationTime, Instruction, ObjectRef, PermissionSubject, Phase, Polarity, Posture,
    PostureEvent, Selector, StandScope, TimeAnchor,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::game) struct StandLock {
    object: u64,
    generation: u64,
    player: u32,
    after: u64,
}

pub(super) fn check_install(action: &Action) -> Result<()> {
    let Action::InstallEffect {
        subjects: InstallationSubjects::Objects { affected, .. },
        capture: InstallationTime::InstructionStart,
        effect,
        duration,
    } = action
    else {
        return Err(unsupported("no-stand installation"));
    };
    let Effect::Permission {
        subject: PermissionSubject::Objects { objects },
        action:
            ActionPattern::Stand {
                objects: action_objects,
                scope: StandScope::RuleStart { player },
            },
        polarity: Polarity::Forbid,
        condition: Expr::Literal(Bool(true)),
        duration: inner,
        ..
    } = effect.as_ref()
    else {
        return Err(unsupported("rule-start permission"));
    };
    if *objects
        != (Selector::Bound {
            reference: *affected,
        })
        || objects != action_objects
        || inner != duration
        || !matches!(&duration.end, DurationEnd::NextPhaseEdge { player: end_player, phase: Phase::Start, edge: Edge::End } if end_player == player)
        || duration.anchor != (TimeAnchor::Installation {})
        || duration.usage.is_some()
        || !duration.intervals.is_empty()
    {
        return Err(unsupported("next start permission lifetime"));
    }
    Ok(())
}

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
        self.internal_posture_members(execution, instruction, members, posture)
    }

    pub(super) fn internal_posture_members(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        members: Vec<ObjectRef>,
        posture: Posture,
    ) -> Result<()> {
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

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Rule stand restrictions are separate from effect posture changes."
)]
impl Game {
    pub(super) fn internal_install_no_stand(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check_install(&instruction.action)?;
        let Action::InstallEffect {
            subjects: InstallationSubjects::Objects { objects, .. },
            duration,
            ..
        } = &instruction.action
        else {
            return Err(unsupported("no-stand subjects"));
        };
        let DurationEnd::NextPhaseEdge { player, .. } = &duration.end else {
            return Err(unsupported("no-stand expiry"));
        };
        let player = execution.player(self, player)?.seat;
        let after = self.state.schedule.current()?;
        let mut installed = Vec::new();
        for member in execution.objects(self, objects)? {
            let object = self.object(execution.object_id(member)?)?;
            if object.generation != member.generation.0 || object.zone != "field" {
                continue;
            }
            self.state.ir_no_stand.push(StandLock {
                object: member.id.0,
                generation: member.generation.0,
                player,
                after,
            });
            installed.push(member);
        }
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Installed(installed),
        );
        Ok(())
    }

    pub(in crate::game) fn internal_rule_stand_blocked(&self, id: &str) -> Result<bool> {
        let object = self.object(id)?;
        let Some(number) = self.state.ir_objects.get(id) else {
            return Ok(false);
        };
        let player = super::player_id(self.active())?.seat;
        let turn = self.state.schedule.current()?;
        Ok(self.state.ir_no_stand.iter().any(|lock| {
            lock.object == *number
                && lock.generation == object.generation
                && lock.player == player
                && turn > lock.after
        }))
    }

    pub(in crate::game) fn internal_finish_rule_stand(&mut self) -> Result<()> {
        let player = super::player_id(self.active())?.seat;
        let turn = self.state.schedule.current()?;
        self.state
            .ir_no_stand
            .retain(|lock| lock.player != player || turn <= lock.after);
        Ok(())
    }
}
