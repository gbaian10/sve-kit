#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use alloc::collections::{BTreeMap, BTreeSet};
use serde_json::{Value, json};

use super::{Frame, Game, Object, list, scalar, string};
use crate::{EngineFailure, Result, invalid};

fn in_print_order(prints: &[String], selected: &[String]) -> Vec<String> {
    let mut counts = BTreeMap::<&str, usize>::new();
    for card in selected {
        let count = counts.entry(card).or_default();
        *count = count.saturating_add(1);
    }
    prints
        .iter()
        .filter(|card| {
            let remaining = counts.entry(card.as_str()).or_default();
            if *remaining == 0 {
                return false;
            }
            *remaining = remaining.saturating_sub(1);
            true
        })
        .cloned()
        .collect()
}

fn token_choices(prints: &[String], count: usize) -> Result<Vec<Value>> {
    let mut sets = BTreeSet::from([Vec::new()]);
    for card in prints {
        let mut additions = Vec::new();
        for selected in &sets {
            if selected.len() < count {
                let mut next = selected.clone();
                next.push(card.clone());
                next.sort();
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
    Ok(sets
        .into_iter()
        .filter(|selected| selected.len() == count)
        .map(|selected| json!({"do":"resolve-choice","select":in_print_order(prints, &selected)}))
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
        if matches!(zone, "field" | "ex") {
            let available = usize::try_from(
                5_i64
                    .saturating_sub(self.zone_count(&frame.controller, zone))
                    .max(0),
            )
            .map_err(invalid)?;
            if prints.len() > available {
                let choices = token_choices(&prints, available)?;
                self.prompt(
                    frame,
                    choices,
                    json!({"resume":"create","node":node,"prints":prints}),
                );
                return Ok(());
            }
        }
        self.create_prints(&prints, node, frame)
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
        let selected = list(&decision["select"])
            .iter()
            .map(|card| string(card).into())
            .collect::<Vec<_>>();
        self.create_prints(&in_print_order(&prints, &selected), &task["node"], frame)
    }

    fn create_prints(&mut self, prints: &[String], node: &Value, frame: &mut Frame) -> Result<()> {
        let created = prints
            .iter()
            .map(|card| self.new_token_object(card, &frame.controller))
            .collect::<Result<Vec<_>>>()?;
        frame.bindings.insert("created-tokens".into(), created);
        Self::prepend(
            frame,
            vec![
                json!({"op":"move","subjects":"created-tokens","to":node["to"],"bind":node["bind"],"capacity_checked":true}),
            ],
        );
        Ok(())
    }
}
