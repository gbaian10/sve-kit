#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use serde_json::{Value, json};

use super::Game;
use crate::Result;

impl Game {
    pub(super) fn change_face_up(&mut self, id: &str, face_up: bool, cause: &Value) -> Result<()> {
        let player_subject = self.player_subject_before(id);
        self.object_mut(id)?.state["face_up"] = json!(face_up);
        let object = self.object(id)?;
        if object.zone == "evolve_deck" {
            let controller = object.controller.clone();
            if face_up {
                for viewer in ["P1", "P2"] {
                    self.learn(viewer, id, true)?;
                }
            } else {
                for (viewer, knowledge) in &mut self.state.knowledge {
                    if *viewer != controller {
                        knowledge.forget_location(id);
                    }
                }
            }
        }
        let group = self.group();
        let player_step = self.emit(
            json!({"kind":"表向き／裏向き","object":id,"face_up":face_up}),
            cause,
            group,
        );
        if !face_up {
            self.player_subject_after(&player_step, player_subject);
        }
        Ok(())
    }

    pub(in crate::game) fn transfer_field_controller(
        &mut self,
        id: &str,
        controller: &str,
        cause: &Value,
        group: u64,
    ) -> Result<()> {
        let player_before = self.player_movement_before(&[id.into()]);
        let previous = self.object(id)?.clone();
        self.player_mut(&previous.controller)?
            .zones
            .entry("field".into())
            .or_default()
            .retain(|value| value != id);
        self.player_mut(controller)?
            .zones
            .entry("field".into())
            .or_default()
            .push(json!(id));
        let object = self.object_mut(id)?;
        object.controller = controller.into();
        // A previous evolution does not grant attack permission after control changes (Q534).
        object.state["control_changed_this_turn"] = json!(true);
        if let Some(state) = object.state.as_object_mut() {
            state.remove("evolved_after_control_change");
        }
        self.emit(json!({"kind":"移動","object":id,"from":format!("{}.field",previous.controller),"to":format!("{controller}.field")}),cause,group);
        self.player_movement_after(player_before);
        Ok(())
    }
}
