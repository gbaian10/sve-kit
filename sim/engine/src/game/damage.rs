#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing and constructed write maps."
)]
use super::internal::{DamageOccurrence, SourceSnapshot};
use super::player::events::ActionDetails;
use super::{BTreeMap, Frame, Game, int, string};
use crate::ir::{DamageEvent, DamageFlags, SnapshotId};
use crate::{EngineFailure, Result, invalid};
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub(super) struct DamageReplacement {
    pub(super) reference: Value,
    pub(super) delta: i64,
    pub(super) multiply: Option<i64>,
    pub(super) cap: Option<(i64, i64)>,
    pub(super) prevent: bool,
    pub(super) consumption: Option<u64>,
}

impl Game {
    /// The generation of a battle hit's source: both fighters stay on the field while battle
    /// damage is dealt, so the generation it has now is the one that fought.
    fn battle_source_generation(&self, hit: &Value) -> Option<u64> {
        (hit["battle"] == true)
            .then(|| self.object(string(&hit["source"])).ok())
            .flatten()
            .map(|object| object.generation)
    }

    pub(super) fn apply_damage(
        &mut self,
        actual: &[Value],
        frame: &Frame,
        facts: Option<&[DamageOccurrence]>,
    ) -> Result<()> {
        let at = SnapshotId(self.state.ir_epoch);
        let combat = if facts.is_none() {
            self.internal_combat_damage_occurrences(actual, at)?
        } else {
            None
        };
        let facts = facts.or(combat.as_deref());
        // Public history needs the battle fact, not an ability's full source snapshot.
        let battle_events = if facts.is_none() && actual.iter().all(|hit| hit["battle"] == true) {
            Some(self.internal_battle_damage_events(actual, at)?)
        } else {
            None
        };
        let history =
            self.internal_capture_damage_history(actual, facts, battle_events.as_deref())?;
        let player_batch = self.player_event_mark();
        let group = self.group();
        let mut life_changes = Vec::new();
        for (hit_index, hit) in actual.iter().enumerate() {
            let id = string(&hit["target"]);
            let source = string(&hit["source"]);
            let amount = int(&hit["amount"]);
            let (before, after) = if let Some(seat) = id.strip_suffix(".leader") {
                let before = int(&self.player(seat)?.leader["life"]);
                let life = before.saturating_sub(amount);
                self.change_life(seat, life)?;
                life_changes.push((seat.to_owned(), before, life));
                self.bump(&format!("{seat}.leader_damaged"), 1);
                (before, life)
            } else {
                let before = int(&self.object(id)?.state["hp"]);
                self.numeric_damage(id, amount)?;
                let follower = self.object_type(id)?.contains("フォロワー");
                let object = self.object_mut(id)?;
                if amount > 0 && follower {
                    object.state["damaged_this_turn"] = json!(true);
                }
                object.state["damage"] = json!(
                    int(&object.state["damage"])
                        .checked_add(amount)
                        .ok_or_else(|| invalid("damage overflow"))?
                );
                (before, int(&object.state["hp"]))
            };
            let source_generation = facts
                .and_then(|facts| facts.get(hit_index))
                .map(DamageOccurrence::source_generation)
                .or_else(|| {
                    (source == frame.source)
                        .then_some(frame.source_generation)
                        .flatten()
                })
                .or_else(|| self.battle_source_generation(hit));
            let player_source = self.player_source_before(source, source_generation);
            let player_step = self.emit(
                json!({"kind":"ダメージ","source":source,"target":id,"amount":amount,"before":before,"after":after}),
                &frame.cause,
                group,
            );
            self.player_source_after(&player_step, player_source);
            let event = facts
                .and_then(|facts| facts.get(hit_index))
                .map(|fact| &fact.event)
                .or_else(|| battle_events.as_ref()?.get(hit_index));
            self.player_damage_details(&player_step, hit, event);
        }
        self.seal_player_event_batch(player_batch);
        if !actual.is_empty() {
            self.require_ability_sources("event damage subscriptions")?;
            self.require_ability_sources("event deal_damage subscriptions")?;
        }
        self.internal_record_damage_history(history);
        self.internal_damage_events(actual, facts, frame, at)?;
        self.group();
        self.life_change_triggers(
            &life_changes,
            &frame.cause,
            !actual.is_empty() && actual.iter().all(|hit| hit["battle"] == true),
            at,
        )?;
        Ok(())
    }

    fn player_damage_details(&mut self, step: &str, hit: &Value, event: Option<&DamageEvent>) {
        if let Some(event) = event {
            self.player_step_actor(step, &format!("P{}", event.controller.seat));
        }
        let flags = event.map_or_else(
            || match (hit["battle"] == true, hit["attack"] == true) {
                (true, true) => DamageFlags::AttackCombat,
                (true, false) => DamageFlags::Combat,
                (false, true) => DamageFlags::Attack,
                (false, false) => DamageFlags::Ability,
            },
            |event| event.flags,
        );
        self.enrich_player_step(step, None, &ActionDetails::Damage(flags), &[]);
    }

    fn installed_damage_replacements(
        &self,
        hit: &Value,
        consumed: &BTreeMap<u64, i64>,
        source_snapshot: Option<&SourceSnapshot>,
    ) -> Result<Vec<DamageReplacement>> {
        Ok(self
            .internal_prevention_references(hit, source_snapshot)?
            .into_iter()
            .map(|reference| DamageReplacement {
                reference: reference["reference"].clone(),
                delta: int(&reference["delta"]),
                multiply: reference["multiply"].as_i64(),
                cap: reference["cap"]
                    .as_array()
                    .map(|_| (int(&reference["cap"][0]), int(&reference["cap"][1]))),
                prevent: reference["prevent"] == true,
                consumption: reference["remaining"]
                    .as_u64()
                    .filter(|remaining| *remaining > 0)
                    .and_then(|_| reference["reference"]["effect"].as_u64()),
            })
            .filter(|replacement| {
                replacement.consumption.is_none_or(|effect| {
                    self.internal_prevention_remaining(effect)
                        .is_ok_and(|remaining| {
                            u64::try_from(*consumed.get(&effect).unwrap_or(&0))
                                .is_ok_and(|used| remaining > used)
                        })
                })
            })
            .collect::<Vec<_>>())
    }

    pub(super) fn damage_replacements_with_source(
        &self,
        hit: &Value,
        consumed: &BTreeMap<u64, i64>,
        source_snapshot: Option<&SourceSnapshot>,
    ) -> Result<Vec<DamageReplacement>> {
        let result = self.installed_damage_replacements(hit, consumed, source_snapshot)?;
        self.require_ability_sources("damage replacement")?;
        if result.iter().enumerate().any(|(index, entry)| {
            result.iter().take(index).any(|prior| {
                prior.reference == entry.reference
                    && (prior.consumption.is_some() || entry.consumption.is_some())
            })
        }) {
            return Err(EngineFailure::Unsupported(
                "limited damage replacements need distinct occurrence references".into(),
            ));
        }
        Ok(result)
    }

    pub(super) fn commit_damage_uses(&mut self, consumed: &BTreeMap<u64, i64>) -> Result<()> {
        let plans = consumed
            .iter()
            .map(|(&effect, &count)| {
                let count = u64::try_from(count).map_err(invalid)?;
                let remaining = self
                    .internal_prevention_remaining(effect)?
                    .checked_sub(count)
                    .ok_or_else(|| invalid("damage prevention budget exhausted"))?;
                Ok((effect, remaining))
            })
            .collect::<Result<Vec<_>>>()?;
        for (effect, remaining) in plans {
            self.internal_prevention_set_remaining(effect, remaining)?;
        }
        Ok(())
    }
}

impl DamageReplacement {
    pub(super) fn fold_order(mut amount: i64, mut ordered: Vec<Self>) -> Result<(i64, Vec<u64>)> {
        let mut uses = Vec::new();
        while amount > 0 {
            let Some(index) = ordered
                .iter()
                .position(|replacement| replacement.applies(amount))
            else {
                break;
            };
            let replacement = ordered.remove(index);
            if let Some(use_index) = replacement.consumption {
                uses.push(use_index);
            }
            amount = replacement.apply(amount)?;
        }
        Ok((amount.max(0), uses))
    }

    pub(super) fn applies(&self, amount: i64) -> bool {
        self.cap.is_none_or(|(threshold, _)| amount >= threshold)
    }
    pub(super) fn apply(&self, amount: i64) -> Result<i64> {
        if self.prevent {
            Ok(0)
        } else if let Some((_, value)) = self.cap {
            Ok(value)
        } else if let Some(factor) = self.multiply {
            amount
                .checked_mul(factor)
                .ok_or_else(|| invalid("damage replacement overflow"))
        } else {
            amount
                .checked_add(self.delta)
                .ok_or_else(|| invalid("damage replacement overflow"))
        }
    }
}
