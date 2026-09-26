#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing and constructed write maps."
)]
use super::legal::permutations;
use super::{BTreeMap, Frame, Game, int, list, string};
use crate::{EngineFailure, Result, invalid};
use serde_json::{Value, json};

#[derive(Clone)]
struct DamageReplacement {
    reference: Value,
    active: bool,
    depends_on_amount: bool,
    delta: i64,
    set: Option<i64>,
    prevent: bool,
    consumption: Option<usize>,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Damage replacement and simultaneous application share authoritative state."
)]
impl Game {
    pub(super) fn damage_batch(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        let hits = list(&node["hits"]);
        let mut actual = Vec::new();
        let mut consumed = BTreeMap::<usize, i64>::new();
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
            let mut amount = int(&hit["amount"]);
            for replacement in ordered {
                if amount <= 0 {
                    break;
                }
                if let Some(index) = replacement.consumption {
                    let count = consumed.entry(index).or_default();
                    *count = count.saturating_add(1);
                }
                if replacement.prevent {
                    amount = 0;
                } else if let Some(value) = replacement.set {
                    amount = value;
                } else {
                    amount = amount.saturating_add(replacement.delta);
                }
            }
            if amount > 0 {
                actual.push(
                    json!({"source":source,"target":id,"amount":amount,"battle":hit["battle"],"attack":hit["attack"]}),
                );
            }
        }
        // A later hit may still need input; commit charges only after the whole batch is planned.
        for (index, count) in consumed {
            let uses = &mut self.state.continuous[index]["effect"]["uses"];
            *uses = json!(int(uses).saturating_sub(count));
        }
        Self::damage_receipt(&actual, node["bind"].as_str(), frame);
        self.apply_damage(&actual, frame)
    }

    fn apply_damage(&mut self, actual: &[Value], frame: &Frame) -> Result<()> {
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
                let object = self.object_mut(id)?;
                object.state["hp"] = json!(int(&object.state["hp"]).saturating_sub(amount));
                object.state["damage"] = json!(int(&object.state["damage"]).saturating_add(amount));
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
        self.enqueue(pending);
        self.life_change_triggers(&life_changes, &frame.cause)?;
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

    fn damage_replacements(
        &self,
        hit: &Value,
        consumed: &BTreeMap<usize, i64>,
    ) -> Result<Vec<DamageReplacement>> {
        let mut result = Vec::new();
        for source in self.ability_sources() {
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
                *uses = json!(int(uses).saturating_sub(*consumed.get(&index).unwrap_or(&0)));
            }
            let frame: Frame = serde_json::from_value(entry["context"].clone()).map_err(invalid)?;
            let reference = frame.reference.clone();
            if let Some(mut replacement) =
                self.prepare_damage_replacement(&effect, hit, frame, &reference)?
            {
                replacement.consumption = effect.get("uses").map(|_| index);
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
            prevent: body["prevent"] == true,
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
