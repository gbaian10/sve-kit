#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use serde_json::json;

use super::movement::Attachment;
use super::{Frame, Game, int};
use crate::{EngineFailure, Result};

impl Game {
    pub(super) fn clean_attachment_rules(&mut self) -> Result<bool> {
        let equipment_changed = self.clean_attachments(Attachment::Equipment)?;
        let limit_break_changed = self.clean_attachments(Attachment::LimitBreak)?;
        Ok(equipment_changed || limit_break_changed)
    }

    fn clean_attachments(&mut self, kind: Attachment) -> Result<bool> {
        let zone = kind.zone();
        let attachments = self
            .state
            .objects
            .values()
            .filter(|object| object.zone == zone)
            .cloned()
            .collect::<Vec<_>>();
        let mut changed = false;
        for object in attachments {
            let holder = object.state[kind.link()]
                .as_str()
                .and_then(|id| self.state.objects.get(id))
                .filter(|holder| holder.zone == "field");
            let Some(controller) = holder.map(|holder| holder.controller.clone()) else {
                self.erase_attachment(&object.id, kind)?;
                changed = true;
                continue;
            };
            if controller != object.controller {
                self.player_mut(&object.controller)?
                    .zones
                    .entry(zone.into())
                    .or_default()
                    .retain(|id| id != &object.id);
                self.player_mut(&controller)?
                    .zones
                    .entry(zone.into())
                    .or_default()
                    .push(json!(&object.id));
                self.object_mut(&object.id)?
                    .controller
                    .clone_from(&controller);
                let group = self.group();
                self.emit(json!({"kind":"支配変更","object":object.id,"from":object.controller,"to":controller}), &json!({"rule":kind.controller_rule()}), group);
                changed = true;
            }
        }
        Ok(changed)
    }

    pub(super) fn clean_field_rules(&mut self) -> Result<bool> {
        let mut previous = Vec::new();
        let mut rules = json!({});
        let mut destroyed = Vec::new();
        for id in self.field_ids() {
            let object = self.object(&id)?.clone();
            let empty_stack = self.keywords(&id)?.contains("stack")
                && int(&object.state["counters"]["stack_counter"]) <= 0;
            let depleted = int(&object.state["hp"]) <= 0;
            let bane = object.state["bane_damaged"] == true;
            let destroy = self.object_type(&id)?.contains("フォロワー")
                && (depleted || (bane && !self.restricted(&id, "ability_destroy")?));
            if bane {
                self.object_mut(&id)?.state["bane_damaged"] = json!(false);
            }
            if destroy {
                destroyed.push((id.clone(), if depleted { "11.3.1" } else { "11.3.2" }));
            }
            if empty_stack || destroy {
                rules[&id] = json!(if empty_stack {
                    "11.7.1"
                } else if depleted {
                    "11.3.1"
                } else {
                    "11.3.2"
                });
                previous.push(object);
            }
        }
        if previous.is_empty() {
            return Ok(false);
        }
        let player_batch = self.player_event_mark();
        let group = self.group();
        let destroyed_subjects = destroyed
            .iter()
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for (id, rule) in destroyed {
            self.emit(
                json!({"kind":"破壊","object":id}),
                &json!({"rule":rule}),
                group,
            );
        }
        self.seal_player_event_batch(player_batch);
        let mut frame = Frame {
            cause: json!({"rule":"11"}),
            ..Frame::default()
        };
        frame.values.insert("movement_rules".into(), rules);
        frame
            .values
            .insert("destroyed_subjects".into(), json!(destroyed_subjects));
        let ids = previous
            .iter()
            .map(|object| object.id.clone())
            .collect::<Vec<_>>();
        let player_movement = self.player_event_mark();
        self.move_objects(&ids, "cemetery", None, None, &frame)?;
        self.player_movement_method(
            player_movement,
            &destroyed_subjects,
            super::player::events::MovementMethod::RuleDestroy,
        );
        if previous.iter().all(|before| {
            self.state.objects.get(&before.id).is_some_and(|after| {
                after.zone == before.zone
                    && after.generation == before.generation
                    && after.state["counters"] == before.state["counters"]
            })
        }) {
            return Err(EngineFailure::Unsupported(
                "state-based field cleanup made no progress".into(),
            ));
        }
        Ok(true)
    }
}
