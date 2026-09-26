#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use alloc::collections::BTreeMap;
use serde_json::{Value, json};

use super::{Frame, Game, Object, other};
use crate::{EngineFailure, Result};

pub(super) struct Placement {
    pub(super) previous: Object,
    pub(super) destination: String,
    pub(super) controller: String,
    pub(super) changed: bool,
    position: Option<u64>,
}

fn width(item: &Value) -> u64 {
    item.get("filler")
        .map_or(1, |count| count.as_u64().unwrap_or_default())
}

fn position_of(items: &[Value], id: &str) -> Option<u64> {
    let mut offset = 0_u64;
    for item in items {
        if item.as_str() == Some(id) {
            return Some(offset);
        }
        offset = offset.saturating_add(width(item));
    }
    None
}

fn insert_at(items: &mut Vec<Value>, id: &str, offset: u64) {
    let mut skipped = 0_u64;
    for index in 0..items.len() {
        let count = width(&items[index]);
        if offset == skipped {
            items.insert(index, json!(id));
            return;
        }
        if offset < skipped.saturating_add(count) {
            let before = offset.saturating_sub(skipped);
            items.splice(
                index..=index,
                [
                    json!({"filler":before}),
                    json!(id),
                    json!({"filler":count.saturating_sub(before)}),
                ],
            );
            return;
        }
        skipped = skipped.saturating_add(count);
    }
    items.push(json!(id));
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Ordered zone placement shares the authoritative object and zone state."
)]
impl Game {
    pub(super) fn conceal_placement_order(&mut self, plans: &[Placement], frame: &Frame) {
        for owner in ["P1", "P2"] {
            let hidden = plans
                .iter()
                .filter(|plan| plan.destination == "deck" && plan.controller == owner)
                .collect::<Vec<_>>();
            if hidden.len() < 2 {
                continue;
            }
            let chooser = frame
                .values
                .get("placement_order_by")
                .and_then(Value::as_str)
                .unwrap_or(owner);
            if let Some(knowledge) = self.state.knowledge.get_mut(other(chooser)) {
                for plan in hidden {
                    knowledge.located.remove(&plan.previous.id);
                }
            }
        }
    }

    pub(super) fn object_position(&self, id: &str) -> Result<Option<u64>> {
        let object = self.object(id)?;
        Ok(position_of(
            &self.zone(&object.controller, &object.zone),
            id,
        ))
    }

    pub(super) fn place_zone_batch(
        &mut self,
        ids: &[String],
        destinations: &BTreeMap<String, String>,
        side: Option<&str>,
        position: Option<&Value>,
        frame: &Frame,
    ) -> Result<Vec<Placement>> {
        let mut plans = Vec::new();
        for id in ids {
            let previous = self.object(id)?.clone();
            let destination = destinations.get(id).cloned().unwrap_or_default();
            if position.is_some() && destination != "deck" {
                return Err(EngineFailure::Unsupported(
                    "ordered placement outside the deck".into(),
                ));
            }
            if previous.zone == destination && position.is_none() {
                continue;
            }
            let controller = if matches!(destination.as_str(), "field" | "ex") {
                side.map_or_else(
                    || previous.controller.clone(),
                    |side| {
                        self.seats(side, frame)
                            .into_iter()
                            .next()
                            .unwrap_or_default()
                    },
                )
            } else {
                previous.owner.clone()
            };
            plans.push(Placement {
                position: self.object_position(id)?,
                previous,
                destination,
                controller,
                changed: true,
            });
        }
        for plan in &plans {
            if let Some(items) = self
                .player_mut(&plan.previous.controller)?
                .zones
                .get_mut(&plan.previous.zone)
            {
                items.retain(|item| item.as_str() != Some(&plan.previous.id));
            }
        }
        let mut offsets = BTreeMap::<(String, String), u64>::new();
        for plan in &plans {
            let items = self
                .player_mut(&plan.controller)?
                .zones
                .entry(plan.destination.clone())
                .or_default();
            if let Some(position) = position.filter(|pos| **pos != "bottom") {
                let added = offsets
                    .entry((plan.controller.clone(), plan.destination.clone()))
                    .or_default();
                let index = position
                    .as_u64()
                    .unwrap_or(1)
                    .saturating_sub(1)
                    .saturating_add(*added);
                insert_at(items, &plan.previous.id, index);
                *added = added.saturating_add(1);
            } else {
                items.push(json!(plan.previous.id));
            }
        }
        for plan in &mut plans {
            plan.changed = plan.previous.zone != plan.destination
                || plan.previous.controller != plan.controller
                || plan.position
                    != position_of(
                        &self.zone(&plan.controller, &plan.destination),
                        &plan.previous.id,
                    );
        }
        Ok(plans)
    }
}
