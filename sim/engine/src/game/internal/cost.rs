use super::duration::EffectEnd;
use super::{Execution, Game, InstructionOutcome, Result, invalid, unsupported};
use crate::game::Frame;
use crate::ir::{
    Action, CostChange, DurationEnd, Effect, EffectId, Expr, InstallationSubjects,
    InstallationTime, Instruction, Int, Selector, TimeAnchor,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::game) struct CostState {
    generation: u64,
    effects: Vec<CostModifier>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CostModifier {
    effect: u64,
    execution: u64,
    timestamp: u64,
    delta: i64,
    end: EffectEnd,
}

pub(super) fn check_action(action: &Action) -> Result<()> {
    if let Action::InstallEffect {
        subjects,
        capture,
        effect,
        duration,
    } = action
        && let Effect::CostModifier {
            subjects: selected,
            change,
            duration: inner,
            usage,
        } = effect.as_ref()
    {
        let InstallationSubjects::Objects {
            objects: Selector::MoveAfter { .. },
            affected,
        } = subjects
        else {
            return Err(unsupported("cost modifier needs successful movement After"));
        };
        if *capture != InstallationTime::InstructionStart
            || selected
                != &(Selector::Bound {
                    reference: *affected,
                })
            || !matches!(change, CostChange::Delta { amount: Expr::Literal(Int(amount)) } if *amount <= 0)
            || inner != duration
            || usage.is_some()
            || duration.usage.is_some()
            || !duration.intervals.is_empty()
            || duration.end != (DurationEnd::CurrentTurnEnd {})
            || duration.anchor != (TimeAnchor::Installation {})
        {
            return Err(unsupported("cost modifier shape or duration"));
        }
    }
    Ok(())
}

pub(super) fn printed_cost(value: &Value) -> Result<i64> {
    match value {
        Value::Null => Ok(0),
        Value::String(text) if text == "-" => Ok(0),
        Value::String(text) => text.parse().map_err(invalid),
        _ => value
            .as_i64()
            .ok_or_else(|| invalid("invalid printed cost")),
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Cost modifiers share authoritative movement and turn occurrences."
)]
impl Game {
    pub(in crate::game) fn internal_cost_active(&self) -> bool {
        self.state
            .ir_cost
            .keys()
            .any(|id| self.internal_cost_layered(id))
    }

    pub(in crate::game) fn internal_cost_layered(&self, id: &str) -> bool {
        self.state.ir_cost.get(id).is_some_and(|state| {
            !state.effects.is_empty()
                && self
                    .state
                    .objects
                    .get(id)
                    .is_some_and(|object| object.generation == state.generation)
        })
    }

    pub(in crate::game) fn internal_effective_cost(&self, id: &str) -> Result<i64> {
        self.internal_cost_with_delta(id, 0)
    }

    fn internal_cost_with_delta(&self, id: &str, delta: i64) -> Result<i64> {
        let object = self.object(id)?;
        if object.state["evolved_with"].is_string() || super::int(&object.state["face"]) != 0 {
            return Err(unsupported("evolved cost information"));
        }
        let base = match object.state.get("cost") {
            Some(cost) => cost
                .as_i64()
                .ok_or_else(|| invalid("invalid effective cost"))?,
            None => printed_cost(&self.face(id)?["cost"])?,
        };
        let cost = self
            .state
            .ir_cost
            .get(id)
            .filter(|state| state.generation == object.generation)
            .map_or(Ok(base), |state| {
                state.effects.iter().try_fold(base, |cost, effect| {
                    cost.checked_add(effect.delta)
                        .ok_or_else(|| invalid("effective cost overflow"))
                })
            })?;
        Ok(cost
            .checked_add(delta)
            .ok_or_else(|| invalid("effective cost overflow"))?
            .max(0))
    }

    pub(super) fn internal_install_cost(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        let Action::InstallEffect {
            subjects: InstallationSubjects::Objects { objects, .. },
            effect,
            duration,
            ..
        } = &instruction.action
        else {
            return Err(unsupported("cost installation"));
        };
        let Effect::CostModifier {
            change:
                CostChange::Delta {
                    amount: Expr::Literal(Int(delta)),
                },
            ..
        } = effect.as_ref()
        else {
            return Err(unsupported("cost change"));
        };
        let end = EffectEnd::at(self, execution, &duration.end)?;
        let mut subjects = Vec::new();
        for reference in execution.objects(self, objects)? {
            let id = execution.object_id(reference)?;
            let object = self.object(id)?;
            if object.generation != reference.generation.0 {
                continue;
            }
            if object.zone != "ex" {
                return Err(unsupported("cost modifier successful EX After"));
            }
            self.internal_cost_with_delta(id, *delta)?;
            subjects.push((id.to_owned(), reference));
        }
        let installed = EffectId(self.state.next_ir_effect);
        if !subjects.is_empty() {
            self.state.next_ir_effect = self
                .state
                .next_ir_effect
                .checked_add(1)
                .ok_or_else(|| invalid("cost effect identities exhausted"))?;
        }
        for (id, reference) in &subjects {
            let state = self
                .state
                .ir_cost
                .entry(id.clone())
                .or_insert_with(|| CostState {
                    generation: reference.generation.0,
                    effects: Vec::new(),
                });
            if state.generation != reference.generation.0 {
                *state = CostState {
                    generation: reference.generation.0,
                    effects: Vec::new(),
                };
            }
            state.effects.push(CostModifier {
                effect: installed.0,
                execution: execution.id.0,
                timestamp: execution.timestamp,
                delta: *delta,
                end,
            });
        }
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Installed(
                subjects
                    .into_iter()
                    .map(|(_, reference)| reference)
                    .collect(),
            ),
        );
        Ok(())
    }

    pub(super) fn internal_play_move(
        &mut self,
        holder: &str,
        destination: &str,
        frame: &Frame,
    ) -> Result<()> {
        let object = self.object(holder)?;
        if !matches!(
            (object.zone.as_str(), destination),
            ("hand" | "ex", "resolution") | ("resolution", "field")
        ) {
            return Err(unsupported("card play rule movement"));
        }
        object
            .generation
            .checked_add(1)
            .ok_or_else(|| invalid("play generation exhausted"))?;
        if !object.state["counters"]
            .as_object()
            .is_none_or(serde_json::Map::is_empty)
            || self.state.ir_numeric.contains_key(holder)
        {
            return Err(unsupported("unimplemented play carry"));
        }
        // Hold the full effect instances across exactly these two rule movements.
        let generation = object.generation;
        let carried = self
            .state
            .ir_cost
            .remove(holder)
            .filter(|state| state.generation == generation);
        self.move_objects(&[holder.into()], destination, None, None, frame)?;
        if let Some(mut carried) = carried {
            carried.generation = self.object(holder)?.generation;
            self.state.ir_cost.insert(holder.into(), carried);
        }
        Ok(())
    }

    pub(super) fn internal_cost_cleanup(&mut self) {
        self.state.ir_cost.retain(|id, state| {
            self.state
                .objects
                .get(id)
                .is_some_and(|object| object.generation == state.generation)
        });
    }

    pub(super) fn internal_cost_end_turn(&mut self, turn: u64) {
        self.internal_cost_cleanup();
        self.state.ir_cost.retain(|_, state| {
            state
                .effects
                .retain(|effect| effect.end != EffectEnd::Turn(turn));
            !state.effects.is_empty()
        });
    }
}
