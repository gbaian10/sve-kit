//! Printed amulets become followers for one turn without changing physical identity.
use super::{EffectEnd, Execution, Game, InstructionOutcome, NumericState, Result, unsupported};
use crate::ir::{Action, BoxPilotMode, DurationEnd, Instruction, ObjectOperand, TimeAnchor};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct State {
    end: EffectEnd,
    kind: Option<Value>,
    attack: Value,
    life: Value,
    maximum: Value,
    damage: Value,
}

pub(in crate::game::internal) fn check_action(action: &Action) -> Result<()> {
    if let Action::BoxPilot { subject, mode } = action {
        let BoxPilotMode::Pilot { duration } = mode else {
            return Err(unsupported("boxed instruction"));
        };
        if *subject != (ObjectOperand::SourceHolder {})
            || duration.end != (DurationEnd::CurrentTurnEnd {})
            || duration.anchor != (TimeAnchor::Installation {})
            || duration.usage.is_some()
            || !duration.intervals.is_empty()
        {
            return Err(unsupported("pilot source holder until current turn end"));
        }
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Pilot shares the generation-bound numeric lifetime."
)]
impl Game {
    pub(in crate::game::internal) fn internal_pilot(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check_action(&instruction.action)?;
        let Action::BoxPilot {
            subject,
            mode: BoxPilotMode::Pilot { duration },
        } = &instruction.action
        else {
            return Err(unsupported("pilot instruction"));
        };
        let end = EffectEnd::at(self, execution, &duration.end)?;
        let subject = execution.object(self, subject)?;
        let id = execution.object_id(subject)?.to_owned();
        let object = self.object(&id)?;
        let mut piloted = vec![];
        let kind = self.object_type(&id)?.to_owned();
        if object.generation == subject.generation.0
            && object.zone == "field"
            && matches!(kind.as_str(), "アミュレット" | "アミュレット・トークン")
        {
            let printed = self.face(&id)?;
            if let (Some(attack), Some(life)) = (
                printed_number(&printed["power"]),
                printed_number(&printed["hp"]),
            ) {
                if self.numeric_state(&id).is_some() {
                    return Err(unsupported("pilot would replace a live numeric base"));
                }
                let prior = State {
                    end,
                    kind: object.state.get("card_type").cloned(),
                    attack: object.state["power"].clone(),
                    life: object.state["hp"].clone(),
                    maximum: object.state["max_hp"].clone(),
                    damage: object.state["damage"].clone(),
                };
                let state = NumericState {
                    generation: object.generation,
                    attack,
                    life,
                    effects: vec![],
                    pilot: Some(prior),
                };
                self.state.ir_numeric.insert(id.clone(), state);
                let holder = self.object_mut(&id)?;
                holder.state["card_type"] = json!(if kind == "アミュレット・トークン" {
                    "フォロワー・トークン"
                } else {
                    "フォロワー"
                });
                holder.state["damage"] = json!(0_i64);
                self.internal_numeric_cache(&id, (attack, life, life))?;
                piloted.push(subject);
            }
        }
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Piloted(piloted));
        Ok(())
    }

    pub(super) fn internal_pilot_end_turn(&mut self, turn: u64) -> Result<()> {
        let expired = self
            .state
            .ir_numeric
            .iter()
            .filter_map(|(id, state)| {
                state
                    .pilot
                    .as_ref()
                    .filter(|pilot| pilot.end == EffectEnd::Turn(turn))
                    .map(|pilot| (id.clone(), state.generation, pilot.clone()))
            })
            .collect::<Vec<_>>();
        for (id, generation, pilot) in expired {
            if self.object(&id)?.generation == generation {
                let object = self.object_mut(&id)?;
                if let Some(kind) = pilot.kind {
                    object.state["card_type"] = kind;
                } else {
                    object
                        .state
                        .as_object_mut()
                        .ok_or_else(|| super::invalid("object state is not a map"))?
                        .remove("card_type");
                }
                for (key, value) in [
                    ("power", pilot.attack),
                    ("hp", pilot.life),
                    ("max_hp", pilot.maximum),
                    ("damage", pilot.damage),
                ] {
                    object.state[key] = value;
                }
            }
            // CR 5.32 / Q1583: the next pilot starts from printed values, including life.
            self.state.ir_numeric.remove(&id);
        }
        Ok(())
    }
}

fn printed_number(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_str()?.parse().ok())
}
