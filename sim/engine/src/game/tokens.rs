use alloc::collections::BTreeSet;
use serde_json::json;

use super::{Game, Object, scalar, string};
use crate::{EngineFailure, Result, invalid};

/// Contract 35: a token not yet created is chosen by its rules name (9.1.2.3), so
/// choices list names in card-text order and identical names collapse.
pub(super) fn token_combinations(names: &[String], count: usize) -> Result<Vec<Vec<String>>> {
    let mut sets = BTreeSet::from([Vec::<usize>::new()]);
    for index in 0..names.len() {
        let mut additions = Vec::new();
        for selected in &sets {
            if selected.len() < count {
                let mut next = selected.clone();
                next.push(index);
                additions.push(next);
            }
        }
        sets.extend(additions);
        if sets.len() > 20_000 {
            return Err(EngineFailure::Unsupported(
                "token capacity choices exceed prototype limit".into(),
            ));
        }
    }
    let mut seen = BTreeSet::new();
    let mut choices = Vec::new();
    for selected in sets.into_iter().filter(|selected| selected.len() == count) {
        let chosen = selected
            .iter()
            .filter_map(|index| names.get(*index).cloned())
            .collect::<Vec<_>>();
        let mut multiset = chosen.clone();
        multiset.sort();
        if seen.insert(multiset) {
            choices.push(chosen);
        }
    }
    Ok(choices)
}

/// Maps chosen names back to the earliest unused print of each name, in text order.
pub(super) fn prints_for_names(
    prints: &[String],
    names: &[String],
    chosen: &[String],
) -> Result<Vec<String>> {
    let mut used = BTreeSet::new();
    for name in chosen {
        let index = names
            .iter()
            .enumerate()
            .position(|(index, candidate)| candidate == name && !used.contains(&index))
            .ok_or_else(|| invalid(format!("token not offered for creation: {name}")))?;
        used.insert(index);
    }
    Ok(used
        .into_iter()
        .filter_map(|index| prints.get(index).cloned())
        .collect())
}

impl Game {
    pub(super) fn new_resource_object(&mut self, role: &str, controller: &str) -> Result<String> {
        let template = self.catalog.rule_bindings.template(role)?;
        self.new_token_object(&template.card_no, template.face_ordinal, controller)
    }

    pub(super) fn new_token_object(
        &mut self,
        card: &str,
        face: usize,
        controller: &str,
    ) -> Result<String> {
        let printed = self.catalog.face(card, face)?;
        let state = json!({"power":scalar(&printed["power"]),"hp":scalar(&printed["hp"]),"max_hp":scalar(&printed["hp"]),"acted":false,"evolved":false,"entered_this_turn":false,"face":face,"damage":0_i64,"counters":{},"keywords":[],"silenced":false,"stats_increased_this_turn":false});
        let id = loop {
            let next = format!("new-{}", self.state.next_object);
            self.state.next_object = self.state.next_object.saturating_add(1);
            if !self.state.objects.contains_key(&next) {
                break next;
            }
        };
        self.state.objects.insert(
            id.clone(),
            Object {
                id: id.clone(),
                card: card.into(),
                owner: controller.into(),
                controller: controller.into(),
                zone: "void".into(),
                generation: 0,
                state,
            },
        );
        Ok(id)
    }

    pub(super) fn token_name(&self, card: &str) -> Result<String> {
        self.catalog.rules_label(card, 0)
    }

    pub(super) fn unique_crest_creations(
        &self,
        prints: &[String],
        zone: &str,
        controller: &str,
    ) -> Result<Vec<String>> {
        if zone != "ex" {
            return Ok(prints.to_vec());
        }
        let mut names = BTreeSet::new();
        for id in self.zone_ids(controller, zone) {
            if self.object_type(&id)?.contains("クレスト") {
                names.insert(self.card_name(&id)?);
            }
        }
        let mut allowed = Vec::new();
        for print in prints {
            let face = self.catalog.face(print, 0)?;
            let name = self.catalog.rules_label(print, 0)?;
            if !string(&face["card_type"]).contains("クレスト") || names.insert(name) {
                allowed.push(print.clone());
            }
        }
        Ok(allowed)
    }
}
