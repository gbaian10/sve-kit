#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use alloc::collections::BTreeSet;
use serde_json::{Value, json};

use super::{Frame, Game, Object, list, scalar, string};
use crate::{EngineFailure, Result, invalid};

/// Contract 35: a token not yet created is chosen by its rules name (9.1.2.3), so
/// choices list names in card-text order and identical names collapse.
fn token_choices(names: &[String], count: usize) -> Result<Vec<Value>> {
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
            choices.push(json!({"do":"resolve-choice","select":chosen}));
        }
    }
    Ok(choices)
}

/// Maps chosen names back to the earliest unused print of each name, in text order.
fn prints_for_names(prints: &[String], names: &[String], chosen: &[String]) -> Result<Vec<String>> {
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

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Rule domains share one private state and are split into focused modules."
)]
impl Game {
    pub(super) fn new_named_object(&mut self, name: &str, controller: &str) -> Result<String> {
        let card = self.catalog.token_print(name)?;
        self.new_token_object(&card, controller)
    }

    fn new_token_object(&mut self, card: &str, controller: &str) -> Result<String> {
        let printed = self.catalog.face(card, 0)?;
        let state = json!({"power":scalar(&printed["power"]),"hp":scalar(&printed["hp"]),"max_hp":scalar(&printed["hp"]),"acted":false,"evolved":false,"entered_this_turn":false,"face":0_i64,"damage":0_i64,"counters":{},"keywords":[],"silenced":false,"stats_increased_this_turn":false});
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

    pub(super) fn create_tokens(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        let specifications = node.get("tokens").map_or_else(|| vec![node.clone()], list);
        let mut prints = Vec::new();
        for specification in specifications {
            let count = usize::try_from(self.number(&specification["count"], frame)?.max(0))
                .map_err(invalid)?;
            if count == 0 {
                continue;
            }
            if prints.len().saturating_add(count) > 1000 {
                return Err(EngineFailure::Unsupported(
                    "token creation exceeds prototype limit".into(),
                ));
            }
            let name = self.eval(&specification["name"], frame)?;
            let card = self.catalog.token_print(string(&name))?;
            prints.extend(vec![card; count]);
        }
        let zone = string(&node["to"]);
        let prints = self.unique_crest_creations(&prints, zone, &frame.controller)?;
        if matches!(zone, "field" | "ex") {
            let available = usize::try_from(
                5_i64
                    .saturating_sub(self.zone_count(&frame.controller, zone))
                    .max(0),
            )
            .map_err(invalid)?;
            if prints.len() > available {
                let names = prints
                    .iter()
                    .map(|card| self.token_name(card))
                    .collect::<Result<Vec<_>>>()?;
                let choices = token_choices(&names, available)?;
                self.prompt(
                    frame,
                    choices,
                    json!({"resume":"create","node":node,"prints":prints,"names":names}),
                );
                return Ok(());
            }
        }
        self.create_prints(&prints, node, &frame.controller.clone(), frame)
    }

    pub(super) fn resume_creation(
        &mut self,
        task: &Value,
        decision: &Value,
        frame: &mut Frame,
    ) -> Result<()> {
        let prints = list(&task["prints"])
            .iter()
            .map(|card| string(card).into())
            .collect::<Vec<_>>();
        let names = list(&task["names"])
            .iter()
            .map(|name| string(name).into())
            .collect::<Vec<_>>();
        let selected = list(&decision["select"])
            .iter()
            .map(|name| string(name).into())
            .collect::<Vec<_>>();
        let mut node = task["node"].clone();
        node["print_selected"] = json!(true);
        self.create_prints(
            &prints_for_names(&prints, &names, &selected)?,
            &node,
            &frame.controller.clone(),
            frame,
        )
    }

    fn create_prints(
        &mut self,
        prints: &[String],
        node: &Value,
        controller: &str,
        frame: &mut Frame,
    ) -> Result<()> {
        let prints = self.unique_crest_creations(prints, string(&node["to"]), controller)?;
        let created = prints
            .iter()
            .map(|card| self.new_token_object(card, controller))
            .collect::<Result<Vec<_>>>()?;
        if node["print_selected"] == true {
            for id in &created {
                self.object_mut(id)?.state["creation_print_selected"] = json!(true);
            }
        }
        frame.bindings.insert("created-tokens".into(), created);
        Self::prepend(
            frame,
            vec![
                json!({"op":"move","subjects":"created-tokens","to":node["to"],"side":controller,"bind":node["bind"],"capacity_checked":true}),
            ],
        );
        Ok(())
    }

    fn token_name(&self, card: &str) -> Result<String> {
        let face = self.catalog.face(card, 0)?;
        Ok(self.catalog.program(card)?["rules_name"]
            .as_str()
            .unwrap_or_else(|| string(&face["name"]))
            .into())
    }

    fn unique_crest_creations(
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
            let name = self.catalog.program(print)?["rules_name"]
                .as_str()
                .unwrap_or_else(|| string(&face["name"]));
            if !string(&face["card_type"]).contains("クレスト") || names.insert(name.into()) {
                allowed.push(print.clone());
            }
        }
        Ok(allowed)
    }

    pub(super) fn transform(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        let selected = self.select(&node["subjects"], frame)?;
        let original = node["subjects"].as_str().map_or_else(
            || Ok(selected.clone()),
            |reference| self.chosen_references(reference, frame),
        )?;
        let names = node
            .get("names")
            .map_or_else(|| vec![node["name"].clone(); original.len()], list);
        if names.len() != original.len() {
            return Err(invalid(
                "transform names must correspond to the original selected subjects",
            ));
        }
        let mut plans = Vec::new();
        for (id, name) in original.iter().zip(&names) {
            if selected.contains(id) && !self.restricted(id, "banish")? {
                let object = self.object(id)?;
                if !matches!(object.zone.as_str(), "void" | "banish") {
                    plans.push((object.clone(), self.catalog.token_print(string(name))?));
                }
            }
        }
        let Some((first, _)) = plans.first() else {
            frame.performed = 0;
            if let Some(bind) = node["bind"].as_str() {
                frame.bindings.insert(bind.into(), Vec::new());
            }
            return Ok(());
        };
        let controller = first.controller.clone();
        let zone = first.zone.clone();
        if plans
            .iter()
            .any(|(object, _)| object.controller != controller || object.zone != zone)
        {
            return Err(EngineFailure::Unsupported(
                "simultaneous transformation across distinct origin zones".into(),
            ));
        }
        let ids = plans
            .iter()
            .map(|(object, _)| object.id.clone())
            .collect::<Vec<_>>();
        self.banish_objects(&ids, frame)?;
        let prints = plans
            .iter()
            .filter(|(before, _)| {
                self.state.objects.get(&before.id).is_some_and(|after| {
                    before.generation != after.generation
                        && matches!(after.zone.as_str(), "banish" | "void")
                })
            })
            .map(|(_, card)| card.clone())
            .collect::<Vec<_>>();
        self.create_prints(
            &prints,
            &json!({"to":zone,"bind":node["bind"]}),
            &controller,
            frame,
        )
    }
}
