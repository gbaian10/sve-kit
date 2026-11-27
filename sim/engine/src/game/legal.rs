#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]

use serde_json::{Value, json};

use super::{Game, list, other, string};
use crate::ir::EffectDefinitionId;
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

impl Game {
    pub(super) fn end_choices(&self, seat: &str) -> Result<Vec<Value>> {
        if self.state.flow["stage"] == "discard" {
            let excess = usize::try_from(self.zone_count(seat, "hand").saturating_sub(7).max(0))
                .map_err(invalid)?;
            return Ok(subsets(&self.zone_ids(seat, "hand"), excess, excess)
                .into_iter()
                .map(|select| json!({"do":"end-discard","select":select}))
                .collect());
        }
        let mut ids = Vec::new();
        for id in self.zone_ids(seat, "field") {
            if self.object(&id)?.state["acted"] != true && self.keywords(&id)?.contains("guard") {
                ids.push(id);
            }
        }
        Ok(subsets(&ids, 0, ids.len())
            .into_iter()
            .map(|select| json!({"do":"guard-act","select":select}))
            .collect())
    }

    pub(crate) fn legal_actions(&self) -> Result<Vec<Value>> {
        if !self.card_runtime.is_empty() {
            self.check_internal_resources()?;
        }
        if self.typed.is_some()
            || self.card_runtime.checkpoint.is_some()
            || self.card_runtime.requires_typed_play()
            || self.card_runtime.has_pending()
            || self.state.game["ended"] == true
            || (!self.card_runtime.is_empty() && self.input_point()["at"] == "quick")
        {
            return Ok(Vec::new());
        }
        if self.state.flow["kind"] == "pregame" {
            return self.opening_choices();
        }
        let point = self.input_point();
        let seat = string(&point["by"]);
        match string(&point["at"]) {
            "end" => self.end_choices(seat),
            "quick" => Ok(vec![json!({"do":"pass"})]),
            "main" => {
                let mut actions = Vec::new();
                if !self.attack_required()? {
                    actions.push(json!({"do":"end-phase"}));
                }
                let mut targets = vec![format!("{}.leader", other(seat))];
                targets.extend(self.zone_ids(other(seat), "field"));
                for attacker in self.zone_ids(seat, "field") {
                    for target in &targets {
                        if self.can_attack(&attacker, target)? {
                            actions
                                .push(json!({"do":"attack","attacker":attacker,"target":target}));
                        }
                    }
                }
                Ok(actions)
            }
            _ => Ok(Vec::new()),
        }
    }

    pub(super) fn can_attack(&self, id: &str, target: &str) -> Result<bool> {
        self.can_attack_ignoring(id, target, None)
    }

    pub(super) fn can_attack_ignoring(
        &self,
        id: &str,
        target: &str,
        ignored: Option<(&str, EffectDefinitionId)>,
    ) -> Result<bool> {
        if !self.card_runtime.is_empty() {
            self.require_internal_abilities(id, "attack")?;
        }
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
        if self.internal_attack_restricted(id, target, ignored)?
            || self.restricted(id, "attack")?
            || (leader
                && (self.restricted(id, "attack_leader")?
                    || self.internal_leader_attack_locked(id)?))
        {
            return Ok(false);
        }
        let control_changed = object.state["control_changed_this_turn"] == true;
        let evolution_allows_attack = object.state["evolved"] == true
            && (!control_changed || object.state["evolved_after_control_change"] == true);
        if (object.state["entered_this_turn"] == true || control_changed)
            && !keys.contains("storm")
            && (leader || (!keys.contains("rush") && !evolution_allows_attack))
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
        let mut guards = Vec::new();
        for guard in self.zone_ids(other(seat), "field") {
            if self.object(&guard)?.state["acted"] == true
                && self.object_type(&guard)?.contains("フォロワー")
                && self.keywords(&guard)?.contains("guard")
                && !self.keywords(&guard)?.contains("intimidate")
            {
                guards.push(guard);
            }
        }
        Ok(guards.is_empty() || guards.iter().any(|guard_id| guard_id == target))
    }

    pub(super) fn targetable_by(&self, id: &str, controller: &str) -> Result<bool> {
        if let Some(object) = self.state.objects.get(id)
            && object.controller != controller
            && object.zone == "field"
        {
            return self.keywords(id).map(|keywords| !keywords.contains("aura"));
        }
        Ok(true)
    }
}
