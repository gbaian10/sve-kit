#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use alloc::collections::BTreeSet;

use serde_json::{Value, json};

use super::{Frame, Game, int, list, other, scalar, string};
use crate::{Result, invalid};

pub(super) fn subsets(items: &[String], min: usize, max: usize) -> Vec<Vec<String>> {
    fn visit(
        items: &[String],
        at: usize,
        min: usize,
        max: usize,
        chosen: &mut Vec<String>,
        out: &mut Vec<Vec<String>>,
    ) {
        if chosen.len() >= min {
            out.push(chosen.clone());
        }
        if chosen.len() >= max {
            return;
        }
        for (i, item) in items.iter().enumerate().skip(at) {
            chosen.push(item.clone());
            visit(items, i.saturating_add(1), min, max, chosen, out);
            chosen.pop();
        }
    }
    let mut out = Vec::new();
    visit(items, 0, min, max, &mut Vec::new(), &mut out);
    out
}

pub(super) fn permutations(items: &[Value]) -> Vec<Vec<Value>> {
    if items.is_empty() {
        return vec![Vec::new()];
    }
    let mut out = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let rest = items
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, v)| v.clone())
            .collect::<Vec<_>>();
        for mut tail in permutations(&rest) {
            tail.insert(0, item.clone());
            out.push(tail);
        }
    }
    out
}

fn distributions(ids: &[String], total: i64) -> Vec<Value> {
    let Some((first, rest)) = ids.split_first() else {
        return if total == 0 {
            vec![json!({})]
        } else {
            Vec::new()
        };
    };
    if rest.is_empty() {
        return if total > 0 {
            vec![json!({first:total})]
        } else {
            Vec::new()
        };
    }
    let mut out = Vec::new();
    for amount in 1..total {
        for mut remaining in distributions(rest, total.saturating_sub(amount)) {
            remaining[first] = json!(amount);
            out.push(remaining);
        }
    }
    out
}

pub(super) fn reference_matches(query: &Value, actual: &Value) -> bool {
    query.as_object().is_some_and(|fields| {
        fields
            .iter()
            .all(|(key, value)| key == "keyword" || actual.get(key) == Some(value))
    })
}
pub(super) fn decision_matches(candidate: &Value, decision: &Value) -> bool {
    candidate.as_object().is_some_and(|map| {
        map.iter()
            .all(|(key, value)| decision.get(key) == Some(value))
    })
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Rule domains share one private state and are split into focused modules."
)]
impl Game {
    /// Complete decision set, before search pruning. Parameterized actions are expanded.
    ///
    /// # Errors
    /// A live card uses semantics the prototype cannot enumerate.
    pub fn legal(&self) -> Result<Vec<Value>> {
        if self.state.game["ended"] == true {
            return Ok(Vec::new());
        }
        if let Some(prompt) = &self.state.prompt {
            return Ok(prompt.choices.clone());
        }
        if let Some(seat) = self.pending_player() {
            let mut out = Vec::new();
            for pending in self.state.pending.iter().filter(|p| p.controller == seat) {
                let base = Self::pending_choice(pending);
                let frame = self.frame_for(&pending.source)?;
                let choices = self.parameterize(base.clone(), &pending.code, &frame)?;
                if choices.is_empty() {
                    out.push(base);
                } else {
                    out.extend(choices);
                }
            }
            return Ok(out);
        }
        let point = self.input_point();
        let seat = string(&point["by"]);
        let quick = point["at"] == "quick";
        if point["at"] == "end" {
            let ids = self
                .zone_ids(seat, "field")
                .into_iter()
                .filter(|id| {
                    self.object(id).is_ok_and(|o| o.state["acted"] != true)
                        && self.keywords(id).is_ok_and(|k| k.contains("guard"))
                })
                .collect::<Vec<_>>();
            return Ok(subsets(&ids, 0, ids.len())
                .into_iter()
                .map(|select| json!({"do":"guard-act","select":select}))
                .collect());
        }
        let mut out = vec![json!({"do":if quick {"pass"} else {"end-phase"}})];
        if !quick {
            let mut targets = vec![format!("{}.leader", other(seat))];
            targets.extend(self.zone_ids(other(seat), "field"));
            for attacker in self.zone_ids(seat, "field") {
                for target in &targets {
                    if self.can_attack(&attacker, target)? {
                        out.push(json!({"do":"attack","attacker":attacker,"target":target}));
                    }
                }
            }
        }
        for id in self
            .zone_ids(seat, "hand")
            .into_iter()
            .chain(self.zone_ids(seat, "ex"))
        {
            if self.play_cost(&id)? > int(&self.player(seat)?.pp["current"]) {
                continue;
            }
            if quick && !self.keywords(&id)?.contains("quick") {
                continue;
            }
            let is_spell = string(&self.face(&id)?["card_type"]).contains("スペル");
            if !is_spell && self.zone_count(seat, "field") >= 5 {
                continue;
            }
            let mut options = vec![json!({"do":"play","card":id})];
            let frame = self.frame_for(&id)?;
            for code in self.abilities(&id)?.iter().filter(|a| a["kind"] == "spell") {
                let mut next = Vec::new();
                for option in options {
                    next.extend(self.parameterize(option, code, &frame)?);
                }
                options = next;
            }
            out.extend(options);
        }
        for id in self.zone_ids(seat, "field") {
            for code in self
                .abilities(&id)?
                .iter()
                .filter(|a| a["kind"] == "activated" && (!quick || a["quick"] == true))
            {
                let reference = self.reference(&id, code);
                let frame = self.frame_for(&id)?;
                out.extend(self.parameterize(
                    json!({"do":"activate","ability":reference}),
                    code,
                    &frame,
                )?);
            }
        }
        Ok(out)
    }

    pub(super) fn can_attack(&self, id: &str, target: &str) -> Result<bool> {
        let object = self.object(id)?;
        let seat = &object.controller;
        if object.zone != "field"
            || object.state["acted"] == true
            || seat != self.active()
            || !string(&self.face(id)?["card_type"]).contains("フォロワー")
        {
            return Ok(false);
        }
        let keys = self.keywords(id)?;
        let leader = target.ends_with(".leader");
        if self.restricted(id, "attack")? || (leader && self.restricted(id, "attack_leader")?) {
            return Ok(false);
        }
        if object.state["entered_this_turn"] == true
            && !keys.contains("storm")
            && (leader || !keys.contains("rush"))
            && object.state["evolved"] != true
        {
            return Ok(false);
        }
        if leader {
            if target != format!("{}.leader", other(seat)) {
                return Ok(false);
            }
        } else {
            let target_object = self.object(target)?;
            if target_object.controller == *seat
                || target_object.zone != "field"
                || !string(&self.face(target)?["card_type"]).contains("フォロワー")
            {
                return Ok(false);
            }
            if target_object.state["acted"] != true && !keys.contains("assail") {
                return Ok(false);
            }
        }
        let guards = self
            .zone_ids(other(seat), "field")
            .into_iter()
            .filter(|guard| {
                self.object(guard).is_ok_and(|o| o.state["acted"] == true)
                    && self
                        .keywords(guard)
                        .is_ok_and(|guard_keys| guard_keys.contains("guard"))
            })
            .collect::<Vec<_>>();
        Ok(guards.is_empty() || guards.iter().any(|guard_id| guard_id == target))
    }

    pub(super) fn play_cost(&self, id: &str) -> Result<i64> {
        let mut cost = scalar(&self.face(id)?["cost"]);
        let mut sources = self.field_ids();
        if !sources.iter().any(|source| source == id) {
            sources.push(id.into());
        }
        for source in sources {
            for code in self.abilities(&source)? {
                let body = &code["body"];
                if body["op"] != "adjust_cost" {
                    continue;
                }
                let frame = self.frame_for(&source)?;
                if !self.matches(id, &body["subjects"], &frame)?
                    || body
                        .get("condition")
                        .is_some_and(|c| !self.truth(c, &frame).unwrap_or(false))
                {
                    continue;
                }
                if let Some(set) = body.get("set") {
                    cost = self.number(set, &frame)?;
                }
                cost = cost.saturating_add(self.number(&body["amount"], &frame)?);
            }
        }
        Ok(cost.max(0))
    }

    pub(super) fn can_pay(&self, costs: &[Value], frame: &Frame) -> Result<bool> {
        let mut pp = 0_i64;
        let mut acted = BTreeSet::new();
        for cost in costs {
            match string(&cost["op"]) {
                "pp" => pp = pp.saturating_add(self.number(&cost["amount"], frame)?.max(0)),
                "act" => {
                    for id in self.select(&cost["subjects"], frame)? {
                        if self.object(&id)?.state["acted"] == true || !acted.insert(id) {
                            return Ok(false);
                        }
                    }
                }
                "move" | "discard" | "banish" => {}
                _ => return Ok(false),
            }
        }
        Ok(int(&self.player(&frame.controller)?.pp["current"]) >= pp)
    }

    pub(super) fn valid_parameters(&self, code: &Value, frame: &Frame) -> Result<bool> {
        for (field, specs) in [
            ("targets", list(&code["targets"])),
            ("costs", list(&code["cost_selections"])),
        ] {
            for spec in specs {
                if !Self::mode_applies(&spec, frame) {
                    continue;
                }
                let candidates = self.select(&spec["select"], frame)?;
                let key = string(&spec["key"]);
                let chosen = list(&frame.decision[field][key]);
                let min = self.number(&spec["min"], frame)?.max(0);
                let max = self.number(&spec["max"], frame)?.max(0);
                let count = i64::try_from(chosen.len()).unwrap_or(i64::MAX);
                if count < min
                    || count > max
                    || (frame.decision[field].get(key).is_none() && max > 0)
                {
                    return Ok(false);
                }
                let mut seen = BTreeSet::new();
                for target in &chosen {
                    if !candidates.iter().any(|c| c == string(target))
                        || !seen.insert(target.to_string())
                    {
                        return Ok(false);
                    }
                }
                if let Some(total) = spec.get("distribute") {
                    if chosen.is_empty() {
                        continue;
                    }
                    let total = self.number(total, frame)?;
                    let distribution = &frame.decision["distribute"][key];
                    if distribution
                        .as_object()
                        .is_none_or(|m| m.len() != chosen.len())
                        || chosen.iter().any(|id| int(&distribution[string(id)]) <= 0)
                        || chosen
                            .iter()
                            .map(|id| int(&distribution[string(id)]))
                            .sum::<i64>()
                            != total
                    {
                        return Ok(false);
                    }
                }
            }
        }
        let body = &code["body"];
        if body["op"] == "choice" {
            let options = list(&frame.decision["options"]);
            let count = i64::try_from(options.len()).unwrap_or(i64::MAX);
            if count < self.number(&body["min"], frame)?
                || count > self.number(&body["max"], frame)?
                || options
                    .iter()
                    .map(Value::to_string)
                    .collect::<BTreeSet<_>>()
                    .len()
                    != options.len()
            {
                return Ok(false);
            }
            if options.iter().any(|option| {
                int(option) < 1
                    || usize::try_from(int(option)).is_ok_and(|n| n > list(&body["modes"]).len())
            }) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn mode_applies(spec: &Value, frame: &Frame) -> bool {
        spec.get("modes").is_none_or(|modes| {
            list(modes)
                .iter()
                .any(|m| list(&frame.decision["options"]).contains(m))
        })
    }

    fn parameterize(&self, base: Value, code: &Value, frame: &Frame) -> Result<Vec<Value>> {
        let mut options = vec![base];
        let body = &code["body"];
        if body["op"] == "choice" {
            let ids = (1..=list(&body["modes"]).len())
                .map(|n| n.to_string())
                .collect::<Vec<_>>();
            let min = usize::try_from(self.number(&body["min"], frame)?.max(0)).map_err(invalid)?;
            let max = usize::try_from(self.number(&body["max"], frame)?.max(0)).map_err(invalid)?;
            let mut expanded = Vec::new();
            for option in options {
                for subset in subsets(&ids, min, max) {
                    let mut option = option.clone();
                    option["options"] = json!(
                        subset
                            .iter()
                            .filter_map(|s| s.parse::<i64>().ok())
                            .collect::<Vec<_>>()
                    );
                    expanded.push(option);
                }
            }
            options = expanded;
        }
        for (field, specs) in [
            ("targets", list(&code["targets"])),
            ("costs", list(&code["cost_selections"])),
        ] {
            for spec in specs {
                let mut expanded = Vec::new();
                for option in options {
                    let mut context = frame.clone();
                    context.decision = option.clone();
                    if !Self::mode_applies(&spec, &context) {
                        expanded.push(option);
                        continue;
                    }
                    let ids = self.select(&spec["select"], &context)?;
                    let min = usize::try_from(self.number(&spec["min"], &context)?.max(0))
                        .map_err(invalid)?;
                    let max = usize::try_from(self.number(&spec["max"], &context)?.max(0))
                        .map_err(invalid)?;
                    for subset in subsets(&ids, min, max) {
                        let mut option = option.clone();
                        option[field][string(&spec["key"])] = json!(subset);
                        if let Some(total) = spec.get("distribute")
                            && !subset.is_empty()
                        {
                            for distribution in
                                distributions(&subset, self.number(total, &context)?)
                            {
                                let mut option = option.clone();
                                option["distribute"][string(&spec["key"])] = distribution;
                                expanded.push(option);
                            }
                            continue;
                        }
                        expanded.push(option);
                    }
                }
                options = expanded;
            }
        }
        options
            .into_iter()
            .filter_map(|option| {
                let mut context = frame.clone();
                context.decision = option.clone();
                match self.can_pay(&list(&code["costs"]), &context) {
                    Ok(true) => Some(Ok(option)),
                    Ok(false) => None,
                    Err(e) => Some(Err(e)),
                }
            })
            .collect()
    }
}
