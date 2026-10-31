//! Frozen numeric layers ordered by authority proper-play facts.

#![expect(
    clippy::indexing_slicing,
    reason = "Numeric cache writes target constructed object maps."
)]

use super::duration::{EffectEnd, TurnInterval};
use super::{Execution, Fact, Game, InstructionOutcome, Result, invalid, unsupported};
use crate::ir::{
    Action, Bool, CharacteristicPatch, CommitId, ContinuousLayer, Control, Duration, DurationEnd,
    Effect, EffectId, EffectReference, EffectTarget, ExecutionId, ExecutionRef, Expr,
    InstallationSubjects, Instruction, Int, Interval, Node, NonEmpty, NumericChange, ObjectOperand,
    ObjectRef, Program, RuntimeHolder, Selector, Stat, StatChangeEvent, StatChangeReason,
    StatDelta, StatOperation, TimeAnchor, TimestampBasis, ZoneKind,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Modifier {
    effect: u64,
    timestamp: u64,
    execution: u64,
    end: EffectEnd,
    main_live: bool,
    intervals: Vec<TurnInterval>,
    enabled: bool,
    attack: Option<Change>,
    life: Option<Change>,
}

impl NumericState {
    fn effective(&self, damage: i64) -> Result<(i64, i64, i64)> {
        let mut effects = self
            .effects
            .iter()
            .filter(|effect| {
                effect.enabled
                    && (effect.main_live
                        || effect.intervals.iter().copied().any(TurnInterval::active))
            })
            .collect::<Vec<_>>();
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
            Node::Instruction { instruction } => match &instruction.action {
                Action::Modify { patch, .. } => patches.push(patch),
                Action::InstallEffect { effect, .. } => {
                    if let Effect::Continuous { patch, .. } = effect.as_ref() {
                        patches.push(patch);
                    }
                }
                _ => {}
            },
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
            Node::Control {
                control: Control::Mode { modes, .. },
            } => {
                for mode in modes {
                    collect_patches(&mode.body, patches);
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
    match action {
        Action::Modify {
            patch,
            expires,
            source,
            ..
        } => {
            check_duration(expires)?;
            if !matches!(source, EffectReference::CurrentInstallation {}) {
                return Err(unsupported("numeric effect reference"));
            }
            check_patch(patch)
        }
        Action::InstallEffect {
            subjects,
            effect,
            duration,
            ..
        } => {
            let Effect::Continuous {
                target,
                condition,
                patch,
                layer,
                timestamp,
                active_zones,
                duration: inner,
                ..
            } = effect.as_ref()
            else {
                return Ok(());
            };
            let InstallationSubjects::Objects { affected, .. } = subjects else {
                return Err(unsupported("continuous installation subjects"));
            };
            if target
                != &(EffectTarget::Dynamic {
                    selector: Selector::Bound {
                        reference: *affected,
                    },
                })
                || *layer != ContinuousLayer::Numeric
                || *timestamp
                    != (TimestampBasis::ProperPlay {
                        execution: ExecutionRef::Current {},
                    })
                || !active_zones.is_empty()
                || inner != duration
                || !matches!(condition, None | Some(Expr::Literal(_)))
                || !literal_adds(patch)
            {
                return Err(unsupported("continuous frozen constant Add shape"));
            }
            check_duration(duration)
        }
        _ => Ok(()),
    }
}

fn literal_adds(patch: &CharacteristicPatch) -> bool {
    match patch {
        CharacteristicPatch::Numeric {
            change: NumericChange::Add {
                amount: Expr::Literal(_),
            },
            ..
        } => true,
        CharacteristicPatch::Combined { patches } => {
            !patches.is_empty() && patches.iter().all(literal_adds)
        }
        _ => false,
    }
}

fn check_duration(duration: &Duration) -> Result<()> {
    if !matches!(
        duration.end,
        DurationEnd::NoTemporalExpiry {}
            | DurationEnd::EndResolution {}
            | DurationEnd::CurrentTurnEnd {}
            | DurationEnd::NextTurnEnd { .. }
            | DurationEnd::SourceZoneExit {
                source: ObjectOperand::SourceHolder {},
                zone: ZoneKind::Field
            }
    ) || duration.anchor != (TimeAnchor::Installation {})
        || duration.usage.is_some()
        || (!duration.intervals.is_empty()
            && (duration.end != (DurationEnd::CurrentTurnEnd {})
                || duration
                    .intervals
                    .iter()
                    .any(|interval| !matches!(interval, Interval::DuringNextTurn { .. }))))
    {
        return Err(unsupported("numeric duration"));
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
    fn numeric_state(&self, id: &str) -> Option<&NumericState> {
        self.state.ir_numeric.get(id).filter(|state| {
            self.state
                .objects
                .get(id)
                .is_some_and(|object| object.generation == state.generation)
        })
    }

    pub(in crate::game) fn numeric_base_replace_allowed(&self, id: &str) -> Result<()> {
        if self.numeric_state(id).is_some() {
            return Err(unsupported(
                "prototype kind or base replacement with numeric layers",
            ));
        }
        Ok(())
    }

    /// Legacy prototype arithmetic saturates, so only checked-safe operands may meet checked layers.
    pub(in crate::game) fn numeric_prototype_operand(&self, id: &str, safe: bool) -> Result<()> {
        if !safe && self.numeric_state(id).is_some() {
            return Err(unsupported(
                "saturating prototype arithmetic with numeric layers",
            ));
        }
        Ok(())
    }

    // Timed Add expiry follows the project inference in target-evolution-semantics §3,
    // which covers rule evolution only; effect-text evolution keeps the untimed-Add limit.
    pub(in crate::game) fn numeric_evolution_allowed(
        &self,
        id: &str,
        rule_evolution: bool,
    ) -> Result<()> {
        if self.numeric_state(id).is_some_and(|state| {
            state.effects.iter().any(|effect| {
                !(effect.end == EffectEnd::NoTemporalExpiry
                    || (rule_evolution
                        && matches!(effect.end, EffectEnd::Turn(_) | EffectEnd::NextTurn { .. })))
                    || !effect.intervals.is_empty()
                    || matches!(effect.attack, Some(Change::Set(_)))
                    || matches!(effect.life, Some(Change::Set(_)))
            })
        }) {
            return Err(unsupported("evolution outside fixed Add numeric layers"));
        }
        Ok(())
    }

    /// Add-only layers commute with a base delta; Set evolution still needs a rule decision.
    pub(in crate::game) fn numeric_add_base(
        &mut self,
        id: &str,
        attack: i64,
        life: i64,
    ) -> Result<()> {
        let object = self.object(id)?;
        let damage = number(&object.state, "max_hp")?
            .checked_sub(number(&object.state, "hp")?)
            .ok_or_else(|| invalid("numeric damage overflow"))?;
        let mut state = self
            .numeric_state(id)
            .cloned()
            .unwrap_or_else(|| NumericState {
                generation: object.generation,
                attack: super::super::int(&object.state["power"]),
                life: super::super::int(&object.state["max_hp"]),
                effects: Vec::new(),
            });
        if state.effects.iter().any(|effect| {
            matches!(effect.attack, Some(Change::Set(_)))
                || matches!(effect.life, Some(Change::Set(_)))
        }) {
            return Err(unsupported("base delta while Set is active"));
        }
        state.attack = add(state.attack, attack)?;
        state.life = add(state.life, life)?;
        let after = state.effective(damage)?;
        if !state.effects.is_empty() {
            self.state.ir_numeric.insert(id.into(), state);
        }
        self.internal_numeric_cache(id, after)
    }

    pub(in crate::game) fn numeric_damage(&mut self, id: &str, amount: i64) -> Result<()> {
        let object = self.object(id)?;
        let maximum = number(&object.state, "max_hp")?;
        let damage = maximum
            .checked_sub(number(&object.state, "hp")?)
            .and_then(|damage| damage.checked_add(amount))
            .ok_or_else(|| invalid("numeric damage overflow"))?;
        let after = if let Some(state) = self.numeric_state(id) {
            state.effective(damage)?
        } else {
            (
                number(&object.state, "power")?,
                maximum
                    .checked_sub(damage)
                    .ok_or_else(|| invalid("numeric life overflow"))?,
                maximum,
            )
        };
        self.internal_numeric_cache(id, after)
    }

    pub(in crate::game) fn internal_numeric_active(&self) -> bool {
        self.internal_cost_active()
            || !self.prepared.is_empty()
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

    pub(super) fn internal_modify(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        self.internal_modify_options(execution, instruction, true, true, None)
    }

    pub(super) fn internal_modify_chosen(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        subjects: Vec<ObjectRef>,
    ) -> Result<()> {
        self.internal_modify_options(execution, instruction, true, true, Some(subjects))
    }

    pub(super) fn internal_install_continuous(
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
            return Err(unsupported("continuous installation"));
        };
        let Effect::Continuous {
            patch, condition, ..
        } = effect.as_ref()
        else {
            return Err(unsupported("continuous effect"));
        };
        let mut lowered = instruction.clone();
        lowered.action = Action::Modify {
            subjects: objects.clone(),
            patch: patch.clone(),
            expires: duration.clone(),
            source: EffectReference::CurrentInstallation {},
        };
        let enabled = !matches!(condition, Some(Expr::Literal(Bool(false))));
        self.internal_modify_options(execution, &lowered, false, enabled, None)?;
        let Some(InstructionOutcome::Modified { subjects, .. }) =
            execution.receipts.remove(&instruction.receipt)
        else {
            return Err(invalid("missing continuous installation outcome"));
        };
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Installed(subjects));
        Ok(())
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Freezing, planning and committing form one atomic numeric instruction."
    )]
    fn internal_modify_options(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        emit: bool,
        enabled: bool,
        chosen: Option<Vec<ObjectRef>>,
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
        let subjects = match chosen {
            Some(members) => members,
            None => execution.objects(self, subjects)?,
        };
        let (attack, life) = changes(execution, self, patch)?;
        let end = EffectEnd::at(self, execution, &expires.end)?;
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
            if self.state.continuous.iter().any(|entry| {
                entry["effect"]["op"] == "pilot"
                    && entry["generation"].as_u64() == Some(object.generation)
                    && super::super::list(&entry["applies_to"]).contains(&json!(id))
            }) {
                return Err(unsupported("numeric installation during prototype pilot"));
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
                main_live: true,
                intervals: expires
                    .intervals
                    .iter()
                    .map(|interval| TurnInterval::at(self, execution, interval))
                    .collect::<Result<Vec<_>>>()?,
                enabled,
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
            if emit && let Some(facets) = facets {
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
    pub(in crate::game) fn internal_numeric_cleanup(&mut self) -> Result<()> {
        self.internal_cost_cleanup();
        let objects = &self.state.objects;
        self.state.ir_numeric.retain(|id, state| {
            objects
                .get(id)
                .is_some_and(|object| object.generation == state.generation)
        });
        let expired = self
            .state
            .ir_numeric
            .values()
            .flat_map(|state| &state.effects)
            .filter(|effect| effect.end.source_expired(self))
            .map(|effect| effect.effect)
            .collect::<Vec<_>>();
        self.internal_numeric_expire(|effect| expired.contains(&effect.effect), None)
    }

    pub(super) fn internal_numeric_end_resolution(&mut self, execution: ExecutionId) -> Result<()> {
        self.internal_numeric_expire(
            |effect| effect.end == EffectEnd::EndResolution && effect.execution == execution.0,
            None,
        )
    }

    pub(in crate::game) fn internal_numeric_end_turn(&mut self, turn: u64) -> Result<()> {
        self.internal_cost_end_turn(turn);
        self.internal_numeric_cleanup()?;
        self.internal_numeric_expire(|effect| effect.end == EffectEnd::Turn(turn), Some(turn))
    }

    pub(in crate::game) fn internal_numeric_begin_turn(&mut self) -> Result<()> {
        let turn = self.state.schedule.current()?;
        let player = super::player_id(self.active())?.seat;
        for state in self.state.ir_numeric.values_mut() {
            for effect in &mut state.effects {
                effect.end.begin(turn, player);
                for interval in &mut effect.intervals {
                    interval.begin(turn, player);
                }
            }
        }
        let mut plans = Vec::new();
        for (id, state) in &self.state.ir_numeric {
            let object = self.object(id)?;
            let damage = number(&object.state, "max_hp")?
                .checked_sub(number(&object.state, "hp")?)
                .ok_or_else(|| invalid("numeric damage overflow"))?;
            plans.push((id.clone(), state.effective(damage)?));
        }
        for (id, after) in plans {
            self.internal_numeric_cache(&id, after)?;
        }
        Ok(())
    }

    fn internal_numeric_expire(
        &mut self,
        expired: impl Fn(&Modifier) -> bool,
        turn: Option<u64>,
    ) -> Result<()> {
        let mut plans = Vec::new();
        for (id, state) in &self.state.ir_numeric {
            let mut remaining = state.clone();
            for effect in &mut remaining.effects {
                if expired(effect) {
                    effect.main_live = false;
                }
                if let Some(turn) = turn {
                    for interval in &mut effect.intervals {
                        interval.finish(turn);
                    }
                }
            }
            remaining.effects.retain(|effect| {
                effect.main_live || effect.intervals.iter().copied().any(TurnInterval::pending)
            });
            if remaining.effects == state.effects {
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
