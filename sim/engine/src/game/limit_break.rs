#![expect(
    clippy::indexing_slicing,
    reason = "Attachment state uses total reads and writes to constructed objects."
)]
use super::{Frame, Game, Object};
use crate::Result;
use crate::ir::LimitBreakState;
use serde_json::json;

pub(super) const TREASURE: &str = "トレジャー・トークン";
pub(super) const FINAL_LIMIT_BREAK_POSSIBLE: &str = "final_limit_break_possible";
pub(super) const LIMIT_BREAK_COUNTER: &str = "limit_break";

pub(super) const fn stage(final_break: bool) -> &'static str {
    if final_break {
        "final_limit_broken"
    } else {
        "limit_broken"
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Limit breaks and rule attachments share authoritative movement and numeric layers."
)]
impl Game {
    pub(super) fn treasure_abilities_inactive(&self, id: &str) -> Result<bool> {
        Ok(self.object(id)?.zone != "ex" && self.face(id)?["card_type"] == TREASURE)
    }

    pub(super) fn is_limit_broken(&self, id: &str, final_break: bool) -> Result<bool> {
        Ok(self.object(id)?.zone == "field"
            && self.state.objects.values().any(|material| {
                material.zone == "limit_break"
                    && material.state["limit_break_of"] == id
                    && material.state[stage(final_break)] == true
            }))
    }

    pub(super) fn limit_break_condition(&self, id: &str, state: LimitBreakState) -> Result<bool> {
        match state {
            LimitBreakState::Normal => self.is_limit_broken(id, false),
            LimitBreakState::Final => self.is_limit_broken(id, true),
            LimitBreakState::FinalPossible => {
                Ok(self.keywords(id)?.contains(FINAL_LIMIT_BREAK_POSSIBLE))
            }
        }
    }

    pub(super) fn can_limit_break(&self, holder: &str, final_break: bool) -> Result<bool> {
        Ok(self.object(holder)?.zone == "field"
            && self.object_type(holder)?.contains("フォロワー")
            && !self.is_limit_broken(holder, final_break)?
            && (!final_break || self.keywords(holder)?.contains(FINAL_LIMIT_BREAK_POSSIBLE)))
    }

    pub(super) fn apply_limit_break(
        &mut self,
        holder: &str,
        material: &str,
        final_break: bool,
        timestamp: u64,
        frame: &Frame,
    ) -> Result<bool> {
        if self.object(material)?.zone == "ex" {
            self.move_objects(&[material.into()], "limit_break", None, None, frame)?;
            if self.object(material)?.zone != "limit_break" {
                return Ok(false);
            }
            self.object_mut(material)?.state["limit_break_of"] = json!(holder);
        }
        self.object_mut(material)?.state[stage(final_break)] = json!(true);
        self.add_limit_break_bonus(holder, material, final_break, timestamp)?;
        let controller = &self.object(holder)?.controller;
        self.bump(&format!("{controller}.{LIMIT_BREAK_COUNTER}"), 1);
        let group = self.group();
        let player_step = self.emit(
            json!({"kind":if final_break {"最終上限解放"} else {"上限解放"},"object":holder,"material":material}),
            &frame.cause,
            group,
        );
        self.player_step_actor(&player_step, &frame.controller);
        Ok(true)
    }

    pub(super) fn initialize_limit_breaks(&mut self) -> Result<()> {
        let materials = self
            .state
            .objects
            .values()
            .filter(|material| material.zone == "limit_break")
            .cloned()
            .collect::<Vec<_>>();
        for material in materials {
            let Some(holder) = material.state["limit_break_of"].as_str() else {
                continue;
            };
            if self
                .state
                .objects
                .get(holder)
                .is_some_and(|object| object.zone == "field")
            {
                for final_break in [false, true] {
                    if material.state[stage(final_break)] == true {
                        self.add_limit_break_bonus(holder, &material.id, final_break, 0)?;
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn remove_limit_break_bonus(&mut self, material: &Object) -> Result<()> {
        if let Some(holder) = material.state["limit_break_of"].as_str()
            && self
                .state
                .objects
                .get(holder)
                .is_some_and(|object| object.zone == "field")
        {
            self.remove_limit_break_modifiers(holder, &material.id)?;
        }
        Ok(())
    }
}
