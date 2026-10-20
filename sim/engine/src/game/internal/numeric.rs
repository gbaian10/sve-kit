//! Frozen numeric layers ordered by authority proper-play facts.

#![expect(
    clippy::indexing_slicing,
    reason = "Numeric cache writes target constructed object maps."
)]

use super::{Execution, Fact, Game, InstructionOutcome, Result, invalid, unsupported};
use crate::ir::{
    Action, CharacteristicPatch, CommitId, Control, DurationEnd, EffectId, EffectReference,
    ExecutionId, Expr, Instruction, Int, Node, NonEmpty, NumericChange, Program, RuntimeHolder,
    Stat, StatChangeEvent, StatChangeReason, StatDelta, StatOperation, TimeAnchor,
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
    effects: Vec<Modifier>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Change {
    Add(i64),
    Set(i64),
}

impl Change {
    fn apply(self, before: i64) -> Result<i64> {
        match self {
            Self::Add(amount) => add(before, amount),
            Self::Set(amount) => Ok(amount),
        }
    }

    const fn operation(self) -> StatOperation {
        match self {
            Self::Add(_) => StatOperation::Add,
            Self::Set(_) => StatOperation::Set,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Modifier {
    effect: u64,
    timestamp: u64,
    execution: u64,
    end: NumericEnd,
    attack: Option<Change>,
    life: Option<Change>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum NumericEnd {
    NoTemporalExpiry,
    EndResolution,
}

impl NumericState {
    fn effective(&self, damage: i64) -> Result<(i64, i64, i64)> {
        let mut effects = self.effects.iter().collect::<Vec<_>>();
        // Frozen subjects and operands cannot change another layer's applicability or operation.
        effects.sort_by_key(|effect| effect.timestamp);
        let (attack, life) =
            effects
                .into_iter()
                .try_fold((self.attack, self.life), |(attack, life), effect| {
                    Ok::<_, crate::EngineFailure>((
                        apply(effect.attack, attack)?,
                        apply(effect.life, life)?,
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

fn apply(change: Option<Change>, value: i64) -> Result<i64> {
    change.map_or(Ok(value), |change| change.apply(value))
}

// Every possible instruction shares the execution's timestamp, including dead branches.
pub(super) fn check_program(program: &Program) -> Result<()> {
    let mut patches = Vec::new();
    collect_patches(program, &mut patches);
    let shapes = patches.into_iter().map(shape).collect::<Result<Vec<_>>>()?;
    for (index, first) in shapes.iter().enumerate() {
        for second in shapes.iter().skip(index.saturating_add(1)) {
            merge(first.0, second.0)?;
            if matches!(
                (first.1, second.1),
                (Some(Change::Set(_)), Some(Change::Set(_)))
            ) {
                return Err(unsupported(
                    "same-timestamp SetLife may capture different damage",
                ));
            }
            merge(first.1, second.1)?;
        }
    }
    Ok(())
}

fn collect_patches<'program>(
    program: &'program Program,
    patches: &mut Vec<&'program CharacteristicPatch>,
) {
    for step in &program.steps {
        match &step.node {
            Node::Instruction { instruction } => {
                if let Action::Modify { patch, .. } = &instruction.action {
                    patches.push(patch);
                }
            }
            Node::Control {
                control: Control::Sequence { body },
            } => collect_patches(body, patches),
            Node::Control {
                control:
                    Control::If {
                        then_body,
                        else_body,
                        ..
                    },
            } => {
                collect_patches(then_body, patches);
                if let Some(body) = else_body {
                    collect_patches(body, patches);
                }
            }
            _ => {}
        }
    }
}

fn shape(patch: &CharacteristicPatch) -> Result<(Option<Change>, Option<Change>)> {
    match patch {
        CharacteristicPatch::Numeric { stat, change } => {
            let change = match change {
                NumericChange::Add { .. } => Change::Add(0),
                NumericChange::Set {
                    amount: Expr::Literal(Int(value)),
                } if *value >= 0 => Change::Set(*value),
                NumericChange::Set { .. } => {
                    return Err(unsupported("Set requires a nonnegative literal"));
                }
            };
            Ok(match stat {
                Stat::Attack => (Some(change), None),
                Stat::Life => (None, Some(change)),
            })
        }
        CharacteristicPatch::Combined { patches } if !patches.is_empty() => patches
            .iter()
            .try_fold((None, None), |(attack, life), child| {
                let next = shape(child)?;
                Ok((merge(attack, next.0)?, merge(life, next.1)?))
            }),
        _ => Err(unsupported("non-numeric patch")),
    }
}

fn merge(left: Option<Change>, right: Option<Change>) -> Result<Option<Change>> {
    match (left, right) {
        (None, value) | (value, None) => Ok(value),
        (Some(Change::Add(left)), Some(Change::Add(right))) => {
            Ok(Some(Change::Add(add(left, right)?)))
        }
        (Some(Change::Set(left)), Some(Change::Set(right))) if left == right => {
            Ok(Some(Change::Set(left)))
        }
        _ => Err(unsupported(
            "noncommutative numeric patches at one timestamp",
        )),
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
        if !matches!(
            expires.end,
            DurationEnd::NoTemporalExpiry {} | DurationEnd::EndResolution {}
        ) || !matches!(expires.anchor, TimeAnchor::Installation {})
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
    shape(patch).map(|_| ())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Numeric layers share the authoritative objects and instruction transaction."
)]
impl Game {
    pub(in crate::game) fn internal_numeric_active(&self) -> bool {
        !self.prepared.is_empty()
            || self.state.ir_numeric.iter().any(|(id, state)| {
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
            subjects,
            patch,
            expires,
            ..
        } = &instruction.action
        else {
            return Err(unsupported("numeric action"));
        };
        let subjects = execution.objects(self, subjects)?;
        let (attack, life) = changes(execution, self, patch)?;
        let end = match expires.end {
            DurationEnd::NoTemporalExpiry {} => NumericEnd::NoTemporalExpiry,
            DurationEnd::EndResolution {} => NumericEnd::EndResolution,
            _ => return Err(unsupported("numeric duration")),
        };
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
            let adjusted_life = match life {
                Some(Change::Set(value)) => Some(Change::Set(add(value, damage)?)),
                value => value,
            };
            state.effects.push(Modifier {
                effect: effect.0,
                timestamp: execution.timestamp,
                execution: execution.id.0,
                end,
                attack,
                life: adjusted_life,
            });
            let after = state.effective(damage)?;
            let facets = [
                (Stat::Attack, before_attack, after.0, attack),
                (Stat::Life, before_life, after.1, life),
            ]
            .into_iter()
            .filter(|(_, before, after, _)| before != after)
            .map(|(property, before, after, change)| {
                Ok(StatDelta {
                    property,
                    before: Int(before),
                    after: Int(after),
                    delta: Int(after
                        .checked_sub(before)
                        .ok_or_else(|| invalid("numeric delta overflow"))?),
                    operation: change
                        .ok_or_else(|| invalid("missing numeric operation"))?
                        .operation(),
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

    pub(super) fn internal_numeric_end_resolution(&mut self, execution: ExecutionId) -> Result<()> {
        let mut plans = Vec::new();
        for (id, state) in &self.state.ir_numeric {
            let mut remaining = state.clone();
            remaining.effects.retain(|effect| {
                effect.end != NumericEnd::EndResolution || effect.execution != execution.0
            });
            if remaining.effects.len() == state.effects.len() {
                continue;
            }
            let object = self.object(id)?;
            // Materialized max/current life carry damage, never the original base characteristics.
            let damage = number(&object.state, "max_hp")?
                .checked_sub(number(&object.state, "hp")?)
                .ok_or_else(|| invalid("numeric damage overflow"))?;
            let after = remaining.effective(damage)?;
            plans.push((id.clone(), remaining, after));
        }
        for (id, remaining, after) in plans {
            self.internal_numeric_cache(&id, after)?;
            if remaining.effects.is_empty() {
                self.state.ir_numeric.remove(&id);
            } else {
                self.state.ir_numeric.insert(id, remaining);
            }
        }
        Ok(())
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

fn changes(
    execution: &Execution,
    game: &Game,
    patch: &CharacteristicPatch,
) -> Result<(Option<Change>, Option<Change>)> {
    match patch {
        CharacteristicPatch::Numeric { stat, change } => {
            let change = match change {
                NumericChange::Add { amount } => Change::Add(execution.integer(game, amount)?.0),
                NumericChange::Set { amount } => Change::Set(execution.integer(game, amount)?.0),
            };
            Ok(match stat {
                Stat::Attack => (Some(change), None),
                Stat::Life => (None, Some(change)),
            })
        }
        CharacteristicPatch::Combined { patches } => {
            patches
                .iter()
                .try_fold((None, None), |(attack, life), child| {
                    let next = changes(execution, game, child)?;
                    Ok((merge(attack, next.0)?, merge(life, next.1)?))
                })
        }
        _ => Err(unsupported("non-numeric patch")),
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
