use super::duration::EffectEnd;
use super::{Execution, Game, InstructionOutcome, Result, invalid, unsupported};
use crate::game::Frame;
use crate::ir::{
    Action, CostChange, CostKind, DurationEnd, Effect, EffectId, Expr, InstallationSubjects,
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
    evolution: bool,
    change: Change,
    end: EffectEnd,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum Change {
    Delta(i64),
    Set(i64),
}

fn fixed_change(change: &CostChange) -> Result<Change> {
    match change {
        CostChange::Delta {
            amount: Expr::Literal(Int(amount)),
        } => Ok(Change::Delta(*amount)),
        CostChange::Set {
            amount: Expr::Literal(Int(amount)),
        } if *amount >= 0 => Ok(Change::Set(*amount)),
        _ => Err(unsupported("installed fixed cost change")),
    }
}

pub(super) fn check_action(action: &Action) -> Result<()> {
    if let Action::InstallEffect {
        subjects,
        capture,
        effect,
        duration,
    } = action
        && let Effect::CostModifier {
            kind,
            subjects: selected,
            change,
            duration: inner,
            usage,
        } = effect.as_ref()
    {
        if matches!(subjects, InstallationSubjects::Players { .. }) {
            return super::card::other_costs::check_install(action);
        }
        let InstallationSubjects::Objects { objects, affected } = subjects else {
            return Err(unsupported("cost modifier subjects"));
        };
        let supported = match kind {
            CostKind::Play => {
                matches!(objects, Selector::MoveAfter { .. } | Selector::Bound { .. })
                    && matches!(change, CostChange::Delta { amount: Expr::Literal(Int(amount)) } if *amount <= 0)
                    && matches!(
                        duration.end,
                        DurationEnd::CurrentTurnEnd {} | DurationEnd::NoTemporalExpiry {}
                    )
            }
            CostKind::Evolution => {
                (matches!(objects, Selector::Objects { .. } | Selector::Bound { .. })
                    || super::card::selection::public_zone_selector(objects).is_ok())
                    && (matches!(
                        change,
                        CostChange::Delta {
                            amount: Expr::Literal(_)
                        }
                    ) || matches!(change, CostChange::Set { amount: Expr::Literal(Int(amount)) } if *amount >= 0))
                    && matches!(
                        duration.end,
                        DurationEnd::CurrentTurnEnd {} | DurationEnd::NoTemporalExpiry {}
                    )
            }
        };
        if !supported
            || *capture != InstallationTime::InstructionStart
            || selected
                != &(Selector::Bound {
                    reference: *affected,
                })
            || inner != duration
            || usage.is_some()
            || duration.usage.is_some()
            || !duration.intervals.is_empty()
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

pub(super) fn printed_play_cost(value: &Value) -> Result<i64> {
    let cost = match value {
        Value::String(text) => text.parse::<i64>().ok(),
        _ => value.as_i64(),
    };
    cost.filter(|cost| *cost >= 0)
        .ok_or_else(|| unsupported("missing printed card cost or nonnumeric/negative cost"))
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Cost modifiers share authoritative movement and turn occurrences."
)]
impl Game {
    pub(in crate::game) fn internal_cost_active(&self) -> bool {
        self.internal_granted_cost_active(None)
            || self
                .state
                .ir_cost
                .keys()
                .any(|id| self.internal_cost_layered(id))
    }

    pub(in crate::game) fn internal_cost_layered(&self, id: &str) -> bool {
        self.internal_granted_cost_active(Some(id))
            || self.state.ir_cost.get(id).is_some_and(|state| {
                state.effects.iter().any(|effect| !effect.evolution)
                    && self
                        .state
                        .objects
                        .get(id)
                        .is_some_and(|object| object.generation == state.generation)
            })
    }

    pub(in crate::game) fn internal_effective_cost(&self, id: &str) -> Result<i64> {
        let object = self.object(id)?;
        if !object.state["evolved_with"].is_string() && super::int(&object.state["face"]) != 0 {
            return Err(unsupported("evolved cost information"));
        }
        match object.state.get("cost") {
            Some(cost) => cost
                .as_i64()
                .ok_or_else(|| invalid("invalid effective cost")),
            None => printed_cost(&self.internal_cost_print(id)?["cost"]),
        }
        .map(|cost| cost.max(0))
    }

    pub(super) fn internal_cost_print(&self, id: &str) -> Result<&Value> {
        let object = self.object(id)?;
        if object.state["evolved_with"].is_string() {
            self.internal_effective_print(id)?;
            self.catalog.face(&object.card, 0)
        } else {
            self.face(id)
        }
    }

    pub(in crate::game) fn internal_installed_play_cost(&self, id: &str, base: i64) -> Result<i64> {
        self.installed_cost_with(id, base, CostKind::Play, None)
    }

    pub(in crate::game::internal) fn internal_installed_play_parts(
        &self,
        id: &str,
        mut base: i64,
        mut delta: i64,
    ) -> Result<(i64, i64)> {
        delta = delta
            .checked_add(self.internal_granted_play_delta(id)?)
            .ok_or_else(|| invalid("granted play cost overflow"))?;
        let generation = self.object(id)?.generation;
        if let Some(state) = self
            .state
            .ir_cost
            .get(id)
            .filter(|state| state.generation == generation)
        {
            for effect in state.effects.iter().filter(|effect| !effect.evolution) {
                match effect.change {
                    Change::Set(amount) => base = amount,
                    Change::Delta(amount) => {
                        delta = delta
                            .checked_add(amount)
                            .ok_or_else(|| invalid("play cost overflow"))?;
                    }
                }
            }
        }
        Ok((base, delta))
    }

    pub(in crate::game) fn internal_installed_evolution_cost(
        &self,
        id: &str,
        base: i64,
    ) -> Result<i64> {
        self.installed_cost_with(id, base, CostKind::Evolution, None)
    }

    fn installed_cost_with(
        &self,
        id: &str,
        mut base: i64,
        kind: CostKind,
        extra: Option<&CostChange>,
    ) -> Result<i64> {
        let generation = self.object(id)?.generation;
        let changes = self
            .state
            .ir_cost
            .get(id)
            .filter(|state| state.generation == generation)
            .into_iter()
            .flat_map(|state| state.effects.iter())
            .filter(|effect| effect.evolution == (kind == CostKind::Evolution))
            .map(|effect| effect.change)
            .chain(extra.map(fixed_change).transpose()?);
        let mut delta = if kind == CostKind::Play {
            self.internal_granted_play_delta(id)?
        } else {
            0
        };
        for change in changes {
            match change {
                Change::Delta(amount) => {
                    delta = delta
                        .checked_add(amount)
                        .ok_or_else(|| invalid("cost overflow"))?;
                }
                Change::Set(amount) => base = amount,
            }
        }
        base.checked_add(delta)
            .ok_or_else(|| invalid("cost overflow"))
    }

    pub(super) fn internal_install_cost(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        if matches!(
            &instruction.action,
            Action::InstallEffect {
                subjects: InstallationSubjects::Players { .. },
                ..
            }
        ) {
            return self.internal_install_player_cost(execution, instruction);
        }
        let Action::InstallEffect {
            subjects: InstallationSubjects::Objects { objects, .. },
            effect,
            duration,
            ..
        } = &instruction.action
        else {
            return Err(unsupported("cost installation"));
        };
        let Effect::CostModifier { kind, change, .. } = effect.as_ref() else {
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
            if object.zone
                != if *kind == CostKind::Play {
                    "ex"
                } else {
                    "field"
                }
            {
                return Err(unsupported("cost modifier target zone"));
            }
            self.installed_cost_with(id, 0, *kind, Some(change))?;
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
                evolution: *kind == CostKind::Evolution,
                change: fixed_change(change)?,
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
        self.internal_rule_carry_ready(holder)?;
        self.move_playing_card(holder, destination, frame)
    }

    pub(in crate::game) fn internal_rule_carry_ready(&self, holder: &str) -> Result<()> {
        let object = self.object(holder)?;
        object
            .generation
            .checked_add(1)
            .ok_or_else(|| invalid("rule movement generation exhausted"))?;
        if !object.state["counters"]
            .as_object()
            .is_none_or(|counters| counters.values().all(|count| count.as_i64() == Some(0)))
            || self.state.ir_numeric.contains_key(holder)
        {
            return Err(unsupported("unimplemented numeric or counter rule carry"));
        }
        Ok(())
    }

    // Every supported per-object effect crosses rule movements at this shared commit boundary.
    pub(in crate::game) fn internal_commit_rule_carry(
        &mut self,
        holder: &str,
        before: u64,
    ) -> Result<()> {
        let generation = self.object(holder)?.generation;
        if let Some(state) = self
            .state
            .ir_cost
            .get_mut(holder)
            .filter(|state| state.generation == before)
        {
            state.generation = generation;
        }
        self.internal_grants_rule_carry(holder, before)
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
        self.internal_player_cost_end_turn(turn);
        self.internal_cost_cleanup();
        self.state.ir_cost.retain(|_, state| {
            state
                .effects
                .retain(|effect| effect.end != EffectEnd::Turn(turn));
            !state.effects.is_empty()
        });
    }
}
