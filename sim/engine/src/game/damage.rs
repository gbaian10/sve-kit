#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing and constructed write maps."
)]
use super::internal::DamageOccurrence;
use super::legal::permutations;
use super::{BTreeMap, Frame, Game, int, list, string};
use crate::ir::SnapshotId;
use crate::{EngineFailure, Result, invalid};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arithmetic {
    Checked,
    Saturating,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum DamageUse {
    Legacy(usize),
    Installed(u64),
}

#[derive(Clone, Debug)]
pub(super) struct DamageReplacement {
    pub(super) reference: Value,
    pub(super) active: bool,
    pub(super) depends_on_amount: bool,
    pub(super) delta: i64,
    pub(super) set: Option<i64>,
    pub(super) cap: Option<(i64, i64)>,
    pub(super) prevent: bool,
    arithmetic: Arithmetic,
    pub(super) consumption: Option<DamageUse>,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Damage replacement and simultaneous application share authoritative state."
)]
impl Game {
    pub(super) fn damage_batch(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        let hits = list(&node["hits"]);
        let mut actual = Vec::new();
        let mut consumed = BTreeMap::<DamageUse, i64>::new();
        for (hit_index, hit) in hits.iter().enumerate() {
            let order_key = hit_index.to_string();
            let id = string(&hit["target"]);
            let source = string(&hit["source"]);
            if int(&hit["amount"]) <= 0 {
                continue;
            }
            if !id.ends_with(".leader") && self.object(id)?.zone != "field" {
                continue;
            }
            let replacements = self.damage_replacements(hit, &consumed)?;
            // Only prototype replacements use saturating arithmetic.
            self.numeric_prototype_operand(
                id,
                replacements.iter().all(|replacement| {
                    replacement.prevent || replacement.arithmetic == Arithmetic::Checked
                }),
            )?;
            let mut ordered = replacements.clone();
            if replacements.len() > 1
                && node["orders"][&order_key].is_null()
                && int(&hit["amount"]) > 0
            {
                let references = replacements
                    .iter()
                    .map(|replacement| replacement.reference.clone())
                    .collect::<Vec<_>>();
                let choices = permutations(&references)
                    .into_iter()
                    .map(|order| json!({"do":"order-replacements","order":order}))
                    .collect();
                self.prompt(
                    frame,
                    choices,
                    json!({"resume":"replacements","task":node,"target":order_key}),
                );
                let seat = if let Some(seat) = id.strip_suffix(".leader") {
                    seat.to_owned()
                } else {
                    self.object(id)?.controller.clone()
                };
                if let Some(prompt) = self.state.prompt.as_mut() {
                    prompt.by = seat;
                }
                return Ok(());
            }
            if let Some(order) = node["orders"][&order_key].as_array() {
                ordered = order
                    .iter()
                    .filter_map(|reference| {
                        replacements
                            .iter()
                            .find(|replacement| replacement.reference == *reference)
                            .cloned()
                    })
                    .collect();
            }
            let (amount, uses) = DamageReplacement::fold_order(int(&hit["amount"]), ordered)?;
            for index in uses {
                let count = consumed.entry(index).or_default();
                *count = count.saturating_add(1);
            }
            if amount > 0 {
                actual.push(
                    json!({"source":source,"target":id,"amount":amount,"battle":hit["battle"],"attack":hit["attack"]}),
                );
            }
        }
        // A later hit may still need input; commit charges only after the whole batch is planned.
        self.commit_damage_uses(&consumed)?;
        Self::damage_receipt(&actual, node["bind"].as_str(), frame);
        self.apply_damage(&actual, frame, None)
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
        let mut pending = self.drain_triggers(actual, &frame.cause)?;
        let group = self.group();
        let mut damaged = Vec::new();
        let mut life_changes = Vec::new();
        for hit in actual {
            let id = string(&hit["target"]);
            let source = string(&hit["source"]);
            let amount = int(&hit["amount"]);
            if let Some(seat) = id.strip_suffix(".leader") {
                let before = int(&self.player(seat)?.leader["life"]);
                let life = before.saturating_sub(amount);
                self.change_life(seat, life)?;
                life_changes.push((seat.to_owned(), before, life));
                self.bump(&format!("{seat}.leader_damaged"), 1);
            } else {
                self.numeric_damage(id, amount)?;
                let object = self.object_mut(id)?;
                object.state["damage"] = json!(
                    int(&object.state["damage"])
                        .checked_add(amount)
                        .ok_or_else(|| invalid("damage overflow"))?
                );
            }
            damaged.push(self.event_subject(id)?);
            self.emit(
                json!({"kind":"ダメージ","source":source,"target":id,"amount":amount}),
                &frame.cause,
                group,
            );
        }
        pending.extend(self.collect_subject_event(
            "damage",
            &damaged,
            &frame.cause,
            &json!({"effect_damage": !actual.iter().any(|hit|hit["battle"] == true)}),
        )?);
        for hit in actual {
            let source = string(&hit["source"]);
            let subject = self.event_subject(source)?;
            let target = string(&hit["target"]);
            let to_opposing_leader = target.strip_suffix(".leader").is_some_and(|seat| {
                self.object(source)
                    .is_ok_and(|object| object.controller != seat)
            });
            pending.extend(self.collect_subject_event("deal_damage", &[subject], &frame.cause, &json!({"target":target,"amount":hit["amount"],"battle":hit["battle"],"to_opposing_leader":to_opposing_leader}))?);
        }
        self.internal_damage_events(actual, facts, frame, at)?;
        self.enqueue(pending);
        self.life_change_triggers(
            &life_changes,
            &frame.cause,
            !actual.is_empty() && actual.iter().all(|hit| hit["battle"] == true),
            at,
        )?;
        Ok(())
    }

    fn damage_receipt(actual: &[Value], bind: Option<&str>, frame: &mut Frame) {
        frame.performed = actual
            .iter()
            .map(|hit| int(&hit["amount"]))
            .fold(0_i64, i64::saturating_add);
        if let Some(name) = bind {
            frame.bindings.insert(
                name.into(),
                actual
                    .iter()
                    .map(|hit| string(&hit["target"]).to_owned())
                    .collect(),
            );
        }
    }

    fn installed_damage_replacements(
        &self,
        hit: &Value,
        consumed: &BTreeMap<DamageUse, i64>,
    ) -> Result<Vec<DamageReplacement>> {
        Ok(self
            .internal_prevention_references(hit)?
            .into_iter()
            .map(|reference| DamageReplacement {
                reference: reference["reference"].clone(),
                active: true,
                depends_on_amount: false,
                delta: int(&reference["delta"]),
                set: None,
                cap: reference["cap"]
                    .as_array()
                    .map(|_| (int(&reference["cap"][0]), int(&reference["cap"][1]))),
                prevent: reference["prevent"] == true,
                arithmetic: Arithmetic::Checked,
                consumption: reference["remaining"]
                    .as_u64()
                    .filter(|remaining| *remaining > 0)
                    .and_then(|_| reference["reference"]["effect"].as_u64())
                    .map(DamageUse::Installed),
            })
            .filter(|replacement| match replacement.consumption {
                Some(DamageUse::Installed(effect)) => self
                    .internal_prevention_remaining(effect)
                    .is_ok_and(|remaining| {
                        u64::try_from(*consumed.get(&DamageUse::Installed(effect)).unwrap_or(&0))
                            .is_ok_and(|used| remaining > used)
                    }),
                _ => true,
            })
            .collect::<Vec<_>>())
    }

    pub(super) fn damage_replacements(
        &self,
        hit: &Value,
        consumed: &BTreeMap<DamageUse, i64>,
    ) -> Result<Vec<DamageReplacement>> {
        let mut result = self.installed_damage_replacements(hit, consumed)?;
        for source in self.ability_sources("damage replacement")? {
            for ability in self.abilities(&source)? {
                let body = &ability["body"];
                if ability["kind"] != "static"
                    || body["op"] != "replace_damage"
                    || !self.ability_zone(&source, &ability)?
                {
                    continue;
                }
                let frame = self.frame_for(&source)?;
                if let Some(subjects) = body.get("subjects")
                    && !self.matches(string(&hit["target"]), subjects, &frame)?
                {
                    continue;
                }
                if body.get("until").is_some() {
                    return Err(EngineFailure::Unsupported(
                        "timed static damage replacement".into(),
                    ));
                }
                if let Some(replacement) = self.prepare_damage_replacement(
                    body,
                    hit,
                    frame,
                    &self.reference(&source, &ability),
                )? {
                    if replacement.active && body.get("uses").is_some() {
                        return Err(EngineFailure::Unsupported(
                            "limited-use static damage replacement".into(),
                        ));
                    }
                    result.push(replacement);
                }
            }
        }
        for (index, entry) in self.state.continuous.iter().enumerate() {
            if entry["effect"]["op"] != "replace_damage"
                || !self.continuous_applies(entry, string(&hit["target"]))
            {
                continue;
            }
            let mut effect = entry["effect"].clone();
            if let Some(uses) = effect.get_mut("uses") {
                *uses = json!(
                    int(uses)
                        .saturating_sub(*consumed.get(&DamageUse::Legacy(index)).unwrap_or(&0))
                );
            }
            let frame: Frame = serde_json::from_value(entry["context"].clone()).map_err(invalid)?;
            let reference = frame.reference.clone();
            if let Some(mut replacement) =
                self.prepare_damage_replacement(&effect, hit, frame, &reference)?
            {
                replacement.consumption = effect.get("uses").map(|_| DamageUse::Legacy(index));
                result.push(replacement);
            }
        }
        if result.len() > 1 && result.iter().any(|entry| entry.depends_on_amount) {
            return Err(EngineFailure::Unsupported(
                "damage-dependent overlapping replacement order".into(),
            ));
        }
        if result.iter().enumerate().any(|(index, entry)| {
            entry.active
                && result.iter().take(index).any(|prior| {
                    prior.active
                        && prior.reference == entry.reference
                        && (prior.consumption.is_some() || entry.consumption.is_some())
                })
        }) {
            return Err(EngineFailure::Unsupported(
                "limited damage replacements need distinct occurrence references".into(),
            ));
        }
        Ok(result.into_iter().filter(|entry| entry.active).collect())
    }

    pub(super) fn commit_damage_uses(&mut self, consumed: &BTreeMap<DamageUse, i64>) -> Result<()> {
        enum Update {
            Legacy(usize, i64),
            Installed(u64, u64),
        }
        let plans = consumed
            .iter()
            .map(|(usage, count)| {
                Ok(match usage {
                    DamageUse::Legacy(index) => {
                        let entry = self
                            .state
                            .continuous
                            .get(*index)
                            .ok_or_else(|| invalid("missing damage replacement"))?;
                        Update::Legacy(*index, int(&entry["effect"]["uses"]).saturating_sub(*count))
                    }
                    DamageUse::Installed(effect) => {
                        let count = u64::try_from(*count).map_err(invalid)?;
                        let remaining = self
                            .internal_prevention_remaining(*effect)?
                            .checked_sub(count)
                            .ok_or_else(|| invalid("damage prevention budget exhausted"))?;
                        Update::Installed(*effect, remaining)
                    }
                })
            })
            .collect::<Result<Vec<_>>>()?;
        for plan in plans {
            match plan {
                Update::Legacy(index, remaining) => {
                    let entry = self
                        .state
                        .continuous
                        .get_mut(index)
                        .ok_or_else(|| invalid("missing damage replacement"))?;
                    entry["effect"]["uses"] = json!(remaining);
                }
                Update::Installed(effect, remaining) => {
                    self.internal_prevention_set_remaining(effect, remaining)?;
                }
            }
        }
        Ok(())
    }

    fn prepare_damage_replacement(
        &self,
        body: &Value,
        hit: &Value,
        mut frame: Frame,
        reference: &Value,
    ) -> Result<Option<DamageReplacement>> {
        if body.get("uses").is_some_and(|uses| int(uses) <= 0) {
            return Ok(None);
        }
        let kind = string(&body["kind"]);
        if (kind == "effect" && hit["battle"] == true)
            || (kind == "battle"
                && (hit["battle"] != true || string(&hit["target"]).ends_with(".leader")))
            || (kind == "attack" && hit["attack"] != true)
        {
            return Ok(None);
        }
        if let Some(sources) = body.get("sources")
            && !self.matches(string(&hit["source"]), sources, &frame)?
        {
            return Ok(None);
        }
        for key in ["amount", "source", "target"] {
            frame
                .values
                .insert(format!("damage.{key}"), hit[key].clone());
        }
        for key in ["source", "target"] {
            frame
                .bindings
                .insert(format!("damage.{key}"), vec![string(&hit[key]).into()]);
        }
        let active = body
            .get("condition")
            .map_or(Ok(true), |condition| self.truth(condition, &frame))?;
        let set = body
            .get("set")
            .map(|expr| self.number(expr, &frame))
            .transpose()?;
        let depends_on_amount = ["condition", "amount", "set"]
            .iter()
            .any(|key| reads_damage_amount(&body[*key]));
        Ok(Some(DamageReplacement {
            reference: reference.clone(),
            active,
            depends_on_amount,
            delta: self.number(&body["amount"], &frame)?,
            set,
            cap: None,
            prevent: body["prevent"] == true,
            arithmetic: Arithmetic::Saturating,
            consumption: None,
        }))
    }
}

fn reads_damage_amount(node: &Value) -> bool {
    if node["read"] == "damage.amount" {
        return true;
    }
    match node {
        Value::Array(values) => values.iter().any(reads_damage_amount),
        Value::Object(fields) => fields.values().any(reads_damage_amount),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
    }
}

impl DamageReplacement {
    pub(super) fn fold_order(
        mut amount: i64,
        mut ordered: Vec<Self>,
    ) -> Result<(i64, Vec<DamageUse>)> {
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
        } else if let Some(value) = self.set {
            Ok(value)
        } else if let Some((_, value)) = self.cap {
            Ok(value)
        } else if self.arithmetic == Arithmetic::Checked {
            amount
                .checked_add(self.delta)
                .ok_or_else(|| invalid("damage replacement overflow"))
        } else {
            Ok(amount.saturating_add(self.delta))
        }
    }
}
