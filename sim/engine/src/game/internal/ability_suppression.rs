//! Later grants survive both Box and ability removal (CR 5.31.2.1, 10.9.1.6).
use super::duration::EffectEnd;
use super::grant::GrantOrigin;
use super::{Execution, Game, InstructionOutcome, Result, Selector, unsupported};
use crate::ir::ability_suppression::supported;
use crate::ir::events::timing::opponent;
use crate::ir::{
    Action, BoxPilotMode, Duration, DurationEnd, ExecutionRef, Instruction, ObjectOperand,
    ObjectRef, TimeAnchor, TimestampBasis,
};
use alloc::collections::BTreeSet;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::game) struct SuppressionState {
    generation: u64,
    effects: Vec<Suppression>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Suppression {
    end: EffectEnd,
    boxed: bool,
    origins: BTreeSet<GrantOrigin>,
    racing_rush: bool,
    printed: (String, usize),
}

pub(super) fn check_box(action: &Action) -> Result<()> {
    let Action::BoxPilot {
        subject: ObjectOperand::Selected { .. },
        mode: BoxPilotMode::Boxed { duration, cutoff },
    } = action
    else {
        return Err(unsupported("boxed formal target"));
    };
    if duration.anchor != (TimeAnchor::Installation {})
        || duration.usage.is_some()
        || !duration.intervals.is_empty()
        || !matches!(&duration.end, DurationEnd::NextTurnEnd {player} if *player == opponent())
        || *cutoff
            != (TimestampBasis::ProperPlay {
                execution: ExecutionRef::Current {},
            })
    {
        return Err(unsupported("boxed next selected opponent turn end"));
    }
    Ok(())
}

pub(super) fn check_loss(action: &Action) -> Result<()> {
    if !supported(action) {
        return Err(unsupported("singleton ability removal cutoff and duration"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Ability suppression shares the existing authority and effect lifetime."
)]
impl Game {
    fn suppression_state(&self, holder: &str) -> Option<&SuppressionState> {
        let object = self.state.objects.get(holder)?;
        self.state
            .ir_ability_suppression
            .get(holder)
            .filter(|state| object.zone == "field" && state.generation == object.generation)
    }

    pub(in crate::game) fn internal_boxed(&self, holder: &str) -> bool {
        self.suppression_state(holder)
            .is_some_and(|state| state.effects.iter().any(|effect| effect.boxed))
    }

    pub(in crate::game) fn internal_suppresses_printed(
        &self,
        holder: &str,
        card: &str,
        face: usize,
    ) -> bool {
        self.suppression_state(holder).is_some_and(|state| {
            state
                .effects
                .iter()
                .any(|effect| effect.printed.0 == card && effect.printed.1 == face)
        })
    }

    pub(in crate::game::internal) fn internal_suppresses_grant(
        &self,
        holder: &str,
        origin: &GrantOrigin,
    ) -> bool {
        self.suppression_state(holder).is_some_and(|state| {
            state
                .effects
                .iter()
                .any(|effect| effect.origins.contains(origin))
        })
    }

    pub(in crate::game::internal) fn internal_suppresses_racing(&self, holder: &str) -> bool {
        self.suppression_state(holder)
            .is_some_and(|state| state.effects.iter().any(|effect| effect.racing_rush))
    }

    pub(super) fn internal_box(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check_box(&instruction.action)?;
        let Action::BoxPilot {
            subject: ObjectOperand::Selected { selection },
            mode: BoxPilotMode::Boxed { duration, .. },
        } = &instruction.action
        else {
            return Err(unsupported("boxed instruction"));
        };
        let affected = self.suppress_abilities(
            execution,
            &Selector::Bound {
                reference: *selection,
            },
            duration,
            true,
        )?;
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Boxed(affected));
        Ok(())
    }

    pub(super) fn internal_remove_abilities(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check_loss(&instruction.action)?;
        let Action::RemoveAbility {
            subjects, duration, ..
        } = &instruction.action
        else {
            return Err(unsupported("ability removal instruction"));
        };
        let affected = self.suppress_abilities(execution, subjects, duration, false)?;
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Installed(affected));
        Ok(())
    }

    fn suppress_abilities(
        &mut self,
        execution: &Execution,
        subjects: &Selector,
        duration: &Duration,
        boxed: bool,
    ) -> Result<Vec<ObjectRef>> {
        let members = execution.objects(self, subjects)?;
        let [reference] = members.as_slice() else {
            return Err(unsupported("one current field suppression target"));
        };
        let id = execution.object_id(*reference)?.to_owned();
        let object = self.object(&id)?;
        let mut affected = vec![];
        if object.zone == "field" && object.generation == reference.generation.0 {
            let effect = Suppression {
                end: EffectEnd::at(self, execution, &duration.end)?,
                boxed,
                // Underlying grants remain captured when another box currently suppresses them.
                origins: self.internal_suppression_origins(&id)?,
                racing_rush: self.internal_racing_rush(&id)?,
                // Evolution gains the new print's abilities after this cutoff (CR 5.16.2.2).
                printed: self.internal_effective_print(&id)?,
            };
            let state = self
                .state
                .ir_ability_suppression
                .entry(id)
                .or_insert_with(|| SuppressionState {
                    generation: reference.generation.0,
                    effects: vec![],
                });
            if state.generation != reference.generation.0 {
                *state = SuppressionState {
                    generation: reference.generation.0,
                    effects: vec![],
                };
            }
            state.effects.push(effect);
            affected.push(*reference);
        }
        Ok(affected)
    }

    pub(in crate::game) fn internal_suppressions_begin_turn(&mut self) -> Result<()> {
        let turn = self.state.schedule.current()?;
        let player = super::player_id(self.active())?.seat;
        for state in self.state.ir_ability_suppression.values_mut() {
            for effect in &mut state.effects {
                effect.end.begin(turn, player);
            }
        }
        Ok(())
    }

    pub(in crate::game) fn internal_suppressions_end_turn(&mut self, turn: u64) {
        self.state.ir_ability_suppression.retain(|_, state| {
            state
                .effects
                .retain(|effect| effect.end != EffectEnd::Turn(turn));
            !state.effects.is_empty()
        });
    }
}
