#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use alloc::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::{AbilityUse, Frame, Game, int, list, other, string};
use crate::{EngineFailure, Result, invalid};

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
        fields.iter().all(|(key, value)| {
            (key == "keyword" && actual.get(key).is_none()) || actual.get(key) == Some(value)
        })
    })
}
pub(super) fn decision_matches(candidate: &Value, decision: &Value) -> bool {
    if candidate["choice"] == "decline"
        && (decision.get("select").is_some() || decision.get("order").is_some())
    {
        return false;
    }
    if candidate["do"] == "resolve-choice"
        && candidate["select"].as_array().is_some_and(Vec::is_empty)
        && decision["choice"] == "decline"
        && decision.get("select").is_none()
        && decision.get("order").is_none()
    {
        let mut normalized = decision.clone();
        normalized["select"] = json!([]);
        return decision_matches(candidate, &normalized);
    }
    candidate.as_object().is_some_and(|map| {
        map.iter().all(|(key, value)| {
            if key == "select" && candidate["do"] == "resolve-choice" {
                unordered_selection_matches(candidate, decision)
            } else {
                decision.get(key) == Some(value)
            }
        })
    })
}

pub(super) fn unordered_selection_matches(candidate: &Value, decision: &Value) -> bool {
    let Some(selected) = decision["select"].as_array() else {
        return false;
    };
    let mut actual = selected.iter().map(Value::to_string).collect::<Vec<_>>();
    let mut expected = list(&candidate["select"])
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>();
    actual.sort_unstable();
    expected.sort_unstable();
    candidate["do"] == decision["do"] && actual == expected
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
    #[expect(
        clippy::too_many_lines,
        reason = "The legal decision union enumerates each distinct input kind once."
    )]
    pub fn legal(&self) -> Result<Vec<Value>> {
        if self.state.game["ended"] == true {
            return Ok(Vec::new());
        }
        if self.state.flow["kind"] == "pregame" {
            return self.opening_choices();
        }
        if let Some(prompt) = &self.state.prompt {
            return Ok(prompt.choices.clone());
        }
        if let Some(seat) = self.pending_player() {
            return self.pending_actions(seat);
        }
        let point = self.input_point();
        let seat = string(&point["by"]);
        let quick = point["at"] == "quick";
        if point["at"] == "end" {
            if self.state.flow["stage"] == "discard" {
                let excess =
                    usize::try_from(self.zone_count(seat, "hand").saturating_sub(7).max(0))
                        .map_err(invalid)?;
                return Ok(subsets(&self.zone_ids(seat, "hand"), excess, excess)
                    .into_iter()
                    .map(|select| json!({"do":"end-discard","select":select}))
                    .collect());
            }
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
        let mut out = if quick {
            vec![json!({"do":"pass"})]
        } else if self.attack_required()? {
            Vec::new()
        } else {
            vec![json!({"do":"end-phase"})]
        };
        if !quick {
            out.extend(self.evolution_actions(seat)?);
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
            .chain(self.zone_ids(seat, "cemetery"))
        {
            if !self.playable_zone(&id)? || self.card_play_prohibited(&id)? {
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
            for option in options {
                let mut payment = frame.clone();
                payment.decision = option.clone();
                for code in self
                    .abilities(&id)?
                    .iter()
                    .filter(|code| code["kind"] == "spell")
                {
                    self.prepare_additional(code, &mut payment)?;
                }
                let mut costs =
                    vec![json!({"op":"pp","amount":self.play_cost_context(&id,&payment)?})];
                costs.extend(
                    self.abilities(&id)?
                        .iter()
                        .filter(|code| code["kind"] == "spell")
                        .flat_map(|code| Self::payment_nodes(code, &payment)),
                );
                if self.can_pay(&costs, &payment)? {
                    out.push(option);
                }
            }
        }
        for id in ["field", "hand", "ex", "cemetery"]
            .into_iter()
            .flat_map(|zone| self.zone_ids(seat, zone))
        {
            for code in self.abilities(&id)?.iter().filter(|a| {
                matches!(string(&a["kind"]), "activated" | "meal" | "ride")
                    && (!quick || a["quick"] == true)
            }) {
                if !self.ability_zone(&id, code)? || !self.can_use(&id, code)? {
                    continue;
                }
                if (matches!(string(&code["kind"]), "meal" | "ride") || code["advance"] == true) {
                    out.extend(self.resource_options(&id, code)?);
                    continue;
                }
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

    fn pending_actions(&self, seat: &str) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        for pending in self.state.pending.iter().filter(|p| p.controller == seat) {
            let base = Self::pending_choice(pending);
            let frame = self.pending_frame(pending, &base)?;
            let choices = self.parameterize(base.clone(), &pending.code, &frame)?;
            if choices.is_empty() {
                let mut cancellation = base;
                if !list(&pending.code["costs"]).is_empty() {
                    cancellation["costs"] = json!("decline");
                }
                out.push(cancellation);
            } else {
                out.extend(choices);
                if !list(&pending.code["costs"]).is_empty() {
                    let mut decline = base;
                    decline["costs"] = json!("decline");
                    out.push(decline);
                }
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
            || !self.object_type(id)?.contains("フォロワー")
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
                || !self.object_type(target)?.contains("フォロワー")
                || self.keywords(target)?.contains("intimidate")
            {
                return Ok(false);
            }
            if target_object.state["acted"] != true && !keys.contains("assail") {
                return Ok(false);
            }
        }
        if self.restricted(id, "ignore_guard")? {
            return Ok(true);
        }
        let guards = self
            .zone_ids(other(seat), "field")
            .into_iter()
            .filter(|guard| {
                self.object(guard).is_ok_and(|o| o.state["acted"] == true)
                    && self
                        .object_type(guard)
                        .is_ok_and(|kind| kind.contains("フォロワー"))
                    && self
                        .keywords(guard)
                        .is_ok_and(|guard_keys| guard_keys.contains("guard"))
            })
            .collect::<Vec<_>>();
        Ok(guards.is_empty() || guards.iter().any(|guard_id| guard_id == target))
    }

    pub(super) fn can_pay(&self, costs: &[Value], frame: &Frame) -> Result<bool> {
        let mut pp = 0_i64;
        let mut acted = BTreeSet::new();
        let mut flipped = BTreeMap::new();
        let mut counters = BTreeMap::new();
        let mut atoms = Vec::new();
        self.cost_atoms(costs, frame, &mut atoms)?;
        let has_movement = atoms
            .iter()
            .any(|(cost, _)| matches!(string(&cost["op"]), "move" | "discard" | "banish"));
        for (cost, context) in atoms {
            match string(&cost["op"]) {
                "pp" => pp = pp.saturating_add(self.number(&cost["amount"], &context)?.max(0)),
                "earth_rite" => {
                    let count = self.number(&cost["count"], &context)?;
                    if !self
                        .select(
                            &json!({"side":"self","zone":"field","keyword":"stack"}),
                            &context,
                        )?
                        .iter()
                        .any(|id| {
                            self.object(id).is_ok_and(|object| {
                                int(&object.state["counters"]["stack_counter"]) >= count
                            })
                        })
                    {
                        return Ok(false);
                    }
                }
                "act" => {
                    for id in self.select(&cost["subjects"], &context)? {
                        if self.object(&id)?.state["acted"] == true || !acted.insert(id) {
                            return Ok(false);
                        }
                    }
                }
                "flip" => {
                    for id in self.select(&cost["subjects"], &context)? {
                        let face_up = flipped
                            .entry(id.clone())
                            .or_insert(self.object(&id)?.state["face_up"] == true);
                        if *face_up == (cost["face"] == "up") {
                            return Ok(false);
                        }
                        *face_up = cost["face"] == "up";
                    }
                }
                "skip_turn" => {
                    if self.number(&cost["count"], &context)? < 0 {
                        return Ok(false);
                    }
                }
                "counter" => {
                    let amount = self.number(&cost["amount"], &context)?;
                    let name = string(&cost["name"]);
                    for id in self.select(&cost["subjects"], &context)? {
                        let count = counters
                            .entry((id.clone(), name.to_owned()))
                            .or_insert(int(&self.object(&id)?.state["counters"][name]));
                        let after = count.checked_add(amount).ok_or_else(|| {
                            EngineFailure::Unsupported("counter cost exceeds numeric range".into())
                        })?;
                        if after < 0 {
                            return Ok(false);
                        }
                        *count = after;
                    }
                }
                "move" | "discard" | "banish" | "lesson" | "eat" | "reveal" | "_drive_point"
                | "_earth_payment" => {}
                unknown => {
                    return Err(EngineFailure::Unsupported(format!(
                        "cost opcode: {unknown}"
                    )));
                }
            }
        }
        if int(&self.player(&frame.controller)?.pp["current"]) < pp {
            return Ok(false);
        }
        if has_movement {
            return self.movement_payment_possible(costs, frame);
        }
        Ok(true)
    }

    pub(super) fn valid_parameters(&self, code: &Value, frame: &Frame) -> Result<bool> {
        if !self.valid_play_conditions(code, frame)? {
            return Ok(false);
        }
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
                    if field == "targets" && !self.targetable(string(target), frame)? {
                        return Ok(false);
                    }
                }
                let selected = chosen
                    .iter()
                    .map(|id| string(id).to_owned())
                    .collect::<Vec<_>>();
                if !self.selection_constraints(&spec, &selected, &frame.decision[field])? {
                    return Ok(false);
                }
                if let Some(constraint) = spec.get("constraint")
                    && !self.truth(constraint, frame)?
                {
                    return Ok(false);
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
        if body["op"] == "choice" && body["timing"] != "resolve" && body.get("by").is_none() {
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

    fn valid_play_conditions(&self, code: &Value, frame: &Frame) -> Result<bool> {
        if !self.prepare_additional(code, &mut frame.clone())? {
            return Ok(false);
        }
        if let Some(condition) = code.get("play_if")
            && !self.truth(condition, frame)?
        {
            return Ok(false);
        }
        if let Some(domain) = code["variables"].get("x") {
            let Some(x) = frame.decision["x"].as_i64() else {
                return Ok(false);
            };
            if x < self.number(&domain["min"], frame)?.max(0)
                || x > self.number(&domain["max"], frame)?
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn mode_applies(spec: &Value, frame: &Frame) -> bool {
        spec.get("modes").is_none_or(|modes| {
            list(modes)
                .iter()
                .any(|m| list(&frame.decision["options"]).contains(m))
        })
    }

    pub(super) fn parameterize(
        &self,
        base: Value,
        code: &Value,
        initial: &Frame,
    ) -> Result<Vec<Value>> {
        if let Some(domain) = code["variables"].get("x") {
            let min = self.number(&domain["min"], initial)?.max(0);
            let max = self.number(&domain["max"], initial)?;
            if max.saturating_sub(min) > 10_000 {
                return Err(EngineFailure::Unsupported(
                    "X domain exceeds exhaustive prototype limit".into(),
                ));
            }
            let mut result = Vec::new();
            for x in min..=max {
                let mut choice = base.clone();
                choice["x"] = json!(x);
                result.extend(self.parameterize_fixed(choice, code, initial)?);
            }
            return Ok(result);
        }
        self.parameterize_fixed(base, code, initial)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Parameter expansion preserves the ordered Cartesian product of every independent choice."
    )]
    fn parameterize_fixed(&self, base: Value, code: &Value, initial: &Frame) -> Result<Vec<Value>> {
        let mut frame = initial.clone();
        frame.decision = base.clone();
        self.freeze(code, "play-start", &mut frame)?;
        let mut options = vec![base];
        let body = &code["body"];
        if body["op"] == "choice" && body["timing"] != "resolve" && body.get("by").is_none() {
            let ids = (1..=list(&body["modes"]).len())
                .map(|n| n.to_string())
                .collect::<Vec<_>>();
            let min =
                usize::try_from(self.number(&body["min"], &frame)?.max(0)).map_err(invalid)?;
            let max =
                usize::try_from(self.number(&body["max"], &frame)?.max(0)).map_err(invalid)?;
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
                    let mut ids = Vec::new();
                    for id in self.select(&spec["select"], &context)? {
                        if field != "targets" || self.targetable(&id, &context)? {
                            ids.push(id);
                        }
                    }
                    let min = usize::try_from(self.number(&spec["min"], &context)?.max(0))
                        .map_err(invalid)?;
                    let max = usize::try_from(self.number(&spec["max"], &context)?.max(0))
                        .map_err(invalid)?;
                    for subset in self.selection_subsets(&ids, min, max, &spec, &option[field])? {
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
        let mut with_additional = Vec::new();
        for option in options {
            with_additional.extend(self.additional_choices(option, code, &frame)?);
        }
        with_additional
            .into_iter()
            .filter_map(|option| {
                let mut context = frame.clone();
                context.decision = option.clone();
                match self
                    .prepare_additional(code, &mut context)
                    .and_then(|ready| {
                        if ready {
                            self.valid_parameters(code, &context)
                        } else {
                            Ok(false)
                        }
                    })
                    .and_then(|valid| {
                        if valid {
                            self.can_pay(&Self::payment_nodes(code, &context), &context)
                        } else {
                            Ok(false)
                        }
                    }) {
                    Ok(true) => Some(Ok(option)),
                    Ok(false) => None,
                    Err(e) => Some(Err(e)),
                }
            })
            .collect()
    }

    fn targetable(&self, id: &str, frame: &Frame) -> Result<bool> {
        if let Some(object) = self.state.objects.get(id)
            && object.controller != frame.controller
            && object.zone == "field"
        {
            return self.keywords(id).map(|keywords| !keywords.contains("aura"));
        }
        Ok(true)
    }

    pub(super) fn ability_zone(&self, id: &str, code: &Value) -> Result<bool> {
        let object = self.object(id)?;
        let player = self.player(&object.controller)?;
        if code["ub"] == true
            && (player.construction != "title" || player.title != "プリンセスコネクト！Re:Dive")
        {
            return Ok(false);
        }
        Ok(code.get("active_zones").map_or_else(
            || object.zone == "field",
            |zones| list(zones).contains(&json!(object.zone)),
        ))
    }

    pub(super) fn usage_key(id: &str, generation: u64, code: &Value) -> String {
        json!({"source":id,"generation":generation,"line":code["line"],"section":code["section"],"face":code["face"],"keyword":code["keyword"]}).to_string()
    }

    pub(super) fn can_use(&self, id: &str, code: &Value) -> Result<bool> {
        let Some(limit) = code["limit"].as_u64() else {
            return Ok(true);
        };
        Ok(self
            .state
            .used
            .get(&Self::usage_key(id, self.object(id)?.generation, code))
            .map_or(0, |entry| entry.count)
            < limit)
    }

    pub(super) fn mark_use(&mut self, id: &str, code: &Value) -> Result<()> {
        if code.get("limit").is_some() {
            let generation = self.object(id)?.generation;
            let key = Self::usage_key(id, generation, code);
            let ability = self.reference(id, code);
            let entry = self.state.used.entry(key).or_insert(AbilityUse {
                ability,
                generation,
                count: 0,
            });
            entry.count = entry.count.saturating_add(1);
        }
        Ok(())
    }
}
