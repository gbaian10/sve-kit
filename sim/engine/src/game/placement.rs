#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use alloc::collections::BTreeMap;
use serde_json::{Value, json};

use super::legal::subsets;
use super::{Frame, Game, Object, other, string};
use crate::{EngineFailure, Result, invalid};

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
    pub(super) fn placement_controller(
        &self,
        object: &Object,
        destination: &str,
        side: Option<&str>,
        frame: &Frame,
    ) -> Result<String> {
        if !matches!(destination, "field" | "ex") {
            return Ok(object.owner.clone());
        }
        let controller = if let Some(side) = side {
            let seats = self.seats(side, frame);
            if seats.len() != 1 {
                return Err(EngineFailure::Unsupported(
                    "one movement needs one destination per object".into(),
                ));
            }
            seats.into_iter().next().unwrap_or_default()
        } else {
            object.controller.clone()
        };
        if object.zone == destination && controller != object.controller {
            return Err(EngineFailure::Unsupported(
                "cross-player movement within one zone needs dedicated identity rules".into(),
            ));
        }
        Ok(controller)
    }

    pub(super) fn movement_capacity_choices(
        &self,
        node: &Value,
        ids: &[String],
        frame: &Frame,
    ) -> Result<Option<Vec<Value>>> {
        let zone = string(&node["to"]);
        if !matches!(zone, "field" | "ex") || node["capacity_checked"] == true {
            return Ok(None);
        }
        let mut groups = BTreeMap::<String, Vec<String>>::new();
        for id in ids {
            let object = self.object(id)?;
            let controller =
                self.placement_controller(object, zone, node["side"].as_str(), frame)?;
            if object.zone != zone {
                groups.entry(controller).or_default().push(id.clone());
            }
        }
        let mut overflow = false;
        let mut combined = vec![Vec::new()];
        for (controller, arrivals) in groups {
            let available = usize::try_from(
                5_i64
                    .saturating_sub(self.zone_count(&controller, zone))
                    .max(0),
            )
            .map_err(invalid)?;
            overflow |= arrivals.len() > available;
            let count = available.min(arrivals.len());
            let subsets = subsets(&arrivals, count, count);
            if combined.len().saturating_mul(subsets.len()) > 10_000 {
                return Err(EngineFailure::Unsupported(
                    "movement capacity choices exceed prototype limit".into(),
                ));
            }
            combined = combined
                .into_iter()
                .flat_map(|prefix| {
                    subsets.iter().map(move |selected| {
                        let mut joined = prefix.clone();
                        joined.extend(selected.clone());
                        joined
                    })
                })
                .collect();
        }
        Ok(overflow.then(|| {
            combined
                .into_iter()
                .map(|selected| {
                    let ordered = ids
                        .iter()
                        .filter(|id| selected.contains(id))
                        .collect::<Vec<_>>();
                    json!({"do":"resolve-choice","select":ordered})
                })
                .collect()
        }))
    }

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
            let controller = self.placement_controller(&previous, &destination, side, frame)?;
            if previous.zone == destination && position.is_none() {
                continue;
            }
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
