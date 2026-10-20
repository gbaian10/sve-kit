//! Frozen additive layers. Noncommutative and dynamic layers remain unadmitted.

#![expect(
    clippy::indexing_slicing,
    reason = "Numeric cache writes target constructed object maps."
)]

use super::{Execution, Fact, Game, InstructionOutcome, Result, invalid, unsupported};
use crate::ir::{
    Action, CharacteristicPatch, CommitId, DurationEnd, EffectId, EffectReference, ExecutionId,
    Instruction, Int, NonEmpty, NumericChange, RuntimeHolder, Stat, StatChangeEvent,
    StatChangeReason, StatDelta, StatOperation, TimeAnchor,
};
use core::iter::once;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Causal identity remains authority-only; player projections expose effective numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatChangeFact {
    /// Executing body that committed this explicit instruction.
    pub execution: ExecutionId,
    /// Successful installation identity, independent of later queries.
    pub effect: EffectId,
    /// One subject with every nonzero numeric facet.
    pub event: StatChangeEvent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::game) struct NumericState {
    generation: u64,
    attack: i64,
    life: i64,
    effects: Vec<Addition>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Addition {
    effect: u64,
    attack: i64,
    life: i64,
}

impl NumericState {
    fn effective(&self, damage: i64) -> Result<(i64, i64, i64)> {
        let (attack, life) =
            self.effects
                .iter()
                .try_fold((self.attack, self.life), |(attack, life), effect| {
                    Ok::<_, crate::EngineFailure>((
                        add(attack, effect.attack)?,
                        add(life, effect.life)?,
                    ))
                })?;
        Ok((
            attack,
            life.checked_sub(damage)
                .ok_or_else(|| invalid("numeric life overflow"))?,
            life,
        ))
    }
}

pub(super) fn check_action(action: &Action) -> Result<()> {
    if let Action::Modify {
        patch,
        expires,
        source,
        ..
    } = action
    {
        // EndResolution expiry and rule-check order lack a formal contract; only the
        // permission install keeps that lifetime.
        if !matches!(expires.end, DurationEnd::NoTemporalExpiry {})
            || !matches!(expires.anchor, TimeAnchor::Installation {})
            || expires.usage.is_some()
            || !expires.intervals.is_empty()
            || !matches!(source, EffectReference::CurrentInstallation {})
        {
            return Err(unsupported("numeric duration or effect reference"));
        }
        check_patch(patch)?;
    }
    Ok(())
}

fn check_patch(patch: &CharacteristicPatch) -> Result<()> {
    match patch {
        CharacteristicPatch::Numeric {
            change: NumericChange::Add { .. },
            ..
        } => Ok(()),
        CharacteristicPatch::Combined { patches } if !patches.is_empty() => {
            patches.iter().try_for_each(check_patch)
        }
        _ => Err(unsupported("non-additive numeric patch")),
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Numeric layers share the authoritative objects and instruction transaction."
)]
impl Game {
    pub(in crate::game) fn internal_numeric_active(&self) -> bool {
        self.state.ir_numeric.iter().any(|(id, state)| {
            self.state
                .objects
                .get(id)
                .is_some_and(|object| object.generation == state.generation)
        })
    }

    pub(super) fn internal_numeric_stat(&self, id: &str, stat: Stat) -> Result<i64> {
        let object = self.object(id)?;
        if object.zone != "field" || !self.object_type(id)?.contains("フォロワー") {
            return Err(unsupported("numeric reads require field followers"));
        }
        let attack = number(&object.state, "power")?;
        let life = number(&object.state, "hp")?;
        let effective = if let Some(state) = self
            .state
            .ir_numeric
            .get(id)
            .filter(|state| state.generation == object.generation)
        {
            let damage = number(&object.state, "max_hp")?
                .checked_sub(life)
                .ok_or_else(|| invalid("numeric damage overflow"))?;
            let computed = state.effective(damage)?;
            (computed.0, computed.1)
        } else {
            (attack, life)
        };
        Ok(match stat {
            Stat::Attack => effective.0,
            Stat::Life => effective.1,
        })
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Freezing, planning and committing form one atomic numeric instruction."
    )]
    pub(super) fn internal_modify(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        let Action::Modify {
            subjects, patch, ..
        } = &instruction.action
        else {
            return Err(unsupported("numeric action"));
        };
        let subjects = execution.objects(self, subjects)?;
        let (attack, life) = additions(execution, self, patch)?;
        let effect = EffectId(self.state.next_ir_effect);
        let mut plans = Vec::new();
        for subject in subjects {
            let id = execution.object_id(subject)?;
            let object = self.object(id)?;
            if object.generation != subject.generation.0 {
                continue;
            }
            if object.zone != "field" || !self.object_type(id)?.contains("フォロワー") {
                return Err(unsupported("numeric subjects must be field followers"));
            }
            let before_attack = number(&object.state, "power")?;
            let before_life = number(&object.state, "hp")?;
            let maximum = number(&object.state, "max_hp")?;
            let damage = maximum
                .checked_sub(before_life)
                .ok_or_else(|| invalid("numeric damage overflow"))?;
            let mut state = self
                .state
                .ir_numeric
                .get(id)
                .filter(|state| state.generation == subject.generation.0)
                .cloned()
                .unwrap_or_else(|| NumericState {
                    generation: subject.generation.0,
                    attack: before_attack,
                    life: maximum,
                    effects: Vec::new(),
                });
            state.effects.push(Addition {
                effect: effect.0,
                attack,
                life,
            });
            let after = state.effective(damage)?;
            let facets = [
                (Stat::Attack, before_attack, after.0),
                (Stat::Life, before_life, after.1),
            ]
            .into_iter()
            .filter(|(_, before, after)| before != after)
            .map(|(property, before, after)| {
                Ok(StatDelta {
                    property,
                    before: Int(before),
                    after: Int(after),
                    delta: Int(after
                        .checked_sub(before)
                        .ok_or_else(|| invalid("numeric delta overflow"))?),
                    operation: StatOperation::Add,
                })
            })
            .collect::<Result<Vec<_>>>()?;
            let mut facets = facets.into_iter();
            let facets = facets.next().map(|first| NonEmpty {
                first,
                rest: facets.collect(),
            });
            plans.push((id.to_owned(), subject, state, after, facets));
        }
        let installed = !plans.is_empty();
        if installed {
            self.state.next_ir_effect = self
                .state
                .next_ir_effect
                .checked_add(1)
                .ok_or_else(|| invalid("numeric effect identities exhausted"))?;
            self.state.ir_epoch = self
                .state
                .ir_epoch
                .checked_add(1)
                .ok_or_else(|| invalid("numeric commit identities exhausted"))?;
        }
        let installed_subjects = plans.iter().map(|(_, subject, ..)| *subject).collect();
        for (id, subject, state, after, facets) in plans {
            self.state.ir_numeric.insert(id.clone(), state);
            self.internal_numeric_cache(&id, after)?;
            if let Some(facets) = facets {
                if once(&facets.first)
                    .chain(&facets.rest)
                    .any(|facet| facet.delta.0 > 0)
                {
                    self.object_mut(&id)?.state["stats_increased_this_turn"] = json!(true);
                }
                execution.facts.push(Fact::StatChange(StatChangeFact {
                    execution: execution.id,
                    effect,
                    event: StatChangeEvent {
                        subject: RuntimeHolder::Object { object: subject },
                        facets,
                        reason: StatChangeReason::Instruction,
                        commit: CommitId(self.state.ir_epoch),
                        instruction: Some(instruction.id),
                    },
                }));
            }
        }
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Modified {
                effect: installed.then_some(effect),
                subjects: installed_subjects,
            },
        );
        Ok(())
    }

    /// Layers belong to one generation; a moved target never carries them into a new one.
    pub(super) fn internal_numeric_cleanup(&mut self) {
        let objects = &self.state.objects;
        self.state.ir_numeric.retain(|id, state| {
            objects
                .get(id)
                .is_some_and(|object| object.generation == state.generation)
        });
    }

    fn internal_numeric_cache(
        &mut self,
        id: &str,
        (attack, life, maximum): (i64, i64, i64),
    ) -> Result<()> {
        let object = self.object_mut(id)?;
        object.state["power"] = json!(attack);
        object.state["hp"] = json!(life);
        object.state["max_hp"] = json!(maximum);
        Ok(())
    }
}

fn additions(
    execution: &Execution,
    game: &Game,
    patch: &CharacteristicPatch,
) -> Result<(i64, i64)> {
    match patch {
        CharacteristicPatch::Numeric {
            stat,
            change: NumericChange::Add { amount },
        } => {
            let amount = execution.integer(game, amount)?.0;
            Ok(match stat {
                Stat::Attack => (amount, 0),
                Stat::Life => (0, amount),
            })
        }
        CharacteristicPatch::Combined { patches } => {
            patches.iter().try_fold((0, 0), |(attack, life), child| {
                let delta = additions(execution, game, child)?;
                Ok((add(attack, delta.0)?, add(life, delta.1)?))
            })
        }
        _ => Err(unsupported("non-additive numeric patch")),
    }
}

fn number(state: &serde_json::Value, field: &str) -> Result<i64> {
    state
        .get(field)
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| invalid("missing numeric characteristic"))
}

fn add(left: i64, right: i64) -> Result<i64> {
    left.checked_add(right)
        .ok_or_else(|| invalid("numeric addition overflow"))
}
