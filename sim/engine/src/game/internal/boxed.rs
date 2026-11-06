//! Box keeps its cutoff separate from grants obtained later (CR 5.31.2.1).
use super::duration::EffectEnd;
use super::grant::GrantOrigin;
use super::{Execution, Game, InstructionOutcome, Result, Selector, unsupported};
use crate::ir::events::timing::opponent;
use crate::ir::{
    Action, BoxPilotMode, DurationEnd, ExecutionRef, Instruction, ObjectOperand, TimeAnchor,
    TimestampBasis,
};
use alloc::collections::BTreeSet;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::game) struct BoxState {
    generation: u64,
    effects: Vec<BoxEffect>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct BoxEffect {
    end: EffectEnd,
    origins: BTreeSet<GrantOrigin>,
    racing_rush: bool,
    printed: (String, usize),
}

pub(super) fn check(action: &Action) -> Result<()> {
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

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Box shares the existing authority and effect lifetime."
)]
impl Game {
    fn box_state(&self, holder: &str) -> Option<&BoxState> {
        let object = self.state.objects.get(holder)?;
        self.state
            .ir_boxed
            .get(holder)
            .filter(|state| object.zone == "field" && state.generation == object.generation)
    }

    pub(in crate::game) fn internal_boxed(&self, holder: &str) -> bool {
        self.box_state(holder)
            .is_some_and(|state| !state.effects.is_empty())
    }

    pub(in crate::game) fn internal_box_suppresses_printed(
        &self,
        holder: &str,
        card: &str,
        face: usize,
    ) -> bool {
        self.box_state(holder).is_some_and(|state| {
            state
                .effects
                .iter()
                .any(|effect| effect.printed.0 == card && effect.printed.1 == face)
        })
    }

    pub(in crate::game::internal) fn internal_box_suppresses(
        &self,
        holder: &str,
        origin: &GrantOrigin,
    ) -> bool {
        self.box_state(holder).is_some_and(|state| {
            state
                .effects
                .iter()
                .any(|effect| effect.origins.contains(origin))
        })
    }

    pub(in crate::game::internal) fn internal_box_suppresses_racing(&self, holder: &str) -> bool {
        self.box_state(holder)
            .is_some_and(|state| state.effects.iter().any(|effect| effect.racing_rush))
    }

    pub(super) fn internal_box(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check(&instruction.action)?;
        let Action::BoxPilot {
            subject: ObjectOperand::Selected { selection },
            mode: BoxPilotMode::Boxed { duration, .. },
        } = &instruction.action
        else {
            return Err(unsupported("boxed instruction"));
        };
        let members = execution.objects(
            self,
            &Selector::Bound {
                reference: *selection,
            },
        )?;
        let [reference] = members.as_slice() else {
            return Err(unsupported("one boxed target"));
        };
        let id = execution.object_id(*reference)?.to_owned();
        let object = self.object(&id)?;
        let mut boxed = vec![];
        if object.zone == "field" && object.generation == reference.generation.0 {
            let effect = BoxEffect {
                end: EffectEnd::at(self, execution, &duration.end)?,
                // Underlying grants remain captured when another box currently suppresses them.
                origins: self.internal_box_origins(&id)?,
                racing_rush: self.internal_racing_rush(&id)?,
                // Evolution gains the new print's abilities after this cutoff (CR 5.16.2.2).
                printed: self.internal_effective_print(&id)?,
            };
            let state = self.state.ir_boxed.entry(id).or_insert_with(|| BoxState {
                generation: reference.generation.0,
                effects: vec![],
            });
            if state.generation != reference.generation.0 {
                *state = BoxState {
                    generation: reference.generation.0,
                    effects: vec![],
                };
            }
            state.effects.push(effect);
            boxed.push(*reference);
        }
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Boxed(boxed));
        Ok(())
    }

    pub(in crate::game) fn internal_boxes_begin_turn(&mut self) -> Result<()> {
        let turn = self.state.schedule.current()?;
        let player = super::player_id(self.active())?.seat;
        for state in self.state.ir_boxed.values_mut() {
            for effect in &mut state.effects {
                effect.end.begin(turn, player);
            }
        }
        Ok(())
    }

    pub(in crate::game) fn internal_boxes_end_turn(&mut self, turn: u64) {
        self.state.ir_boxed.retain(|_, state| {
            state
                .effects
                .retain(|effect| effect.end != EffectEnd::Turn(turn));
            !state.effects.is_empty()
        });
    }
}
