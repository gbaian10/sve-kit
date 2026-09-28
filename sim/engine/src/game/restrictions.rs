#![expect(
    clippy::indexing_slicing,
    reason = "Validated restriction programs and saved contexts use total JSON indexing."
)]
use serde_json::Value;

use super::{Frame, Game, int, string};
use crate::{EngineFailure, Result, invalid};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "All action prohibitions share the authoritative state and duration checks."
)]
impl Game {
    pub(super) fn card_play_prohibited(&self, id: &str) -> Result<bool> {
        let leader = format!("{}.leader", self.object(id)?.controller);
        if self.restricted(id, "play")? || self.restricted(&leader, "play")? {
            return Ok(true);
        }
        if string(&self.face(id)?["card_type"]).contains("フォロワー")
            && self.restricted(&leader, "play_follower")?
        {
            return Ok(true);
        }
        let frame = self.frame_for(id)?;
        for code in self.abilities(id)? {
            // An intrinsic play prohibition is checked in the card's play procedure (10.3.4).
            if code["kind"] == "static"
                && code["body"]["subjects"] == "self"
                && (code.get("active_zones").is_none() || self.ability_zone(id, &code)?)
                && self.restriction_condition(&code["body"], "play", &frame)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn restricted(&self, id: &str, action: &str) -> Result<bool> {
        for entry in &self.state.continuous {
            if entry["effect"]["op"] != "restrict"
                || entry["effect"]["action"] != action
                || !self.continuous_applies(entry, id)
                || !self.continuous_window_active(entry)?
            {
                continue;
            }
            let frame: Frame = serde_json::from_value(entry["context"].clone()).map_err(invalid)?;
            if self.restriction_condition(&entry["effect"], action, &frame)? {
                return Ok(true);
            }
        }
        for source in self.ability_sources() {
            for code in self.abilities(&source)? {
                if code["kind"] != "static" || !self.ability_zone(&source, &code)? {
                    continue;
                }
                let frame = self.frame_for(&source)?;
                if self.restriction_condition(&code["body"], action, &frame)?
                    && self.matches(id, &code["body"]["subjects"], &frame)?
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn restriction_condition(&self, body: &Value, action: &str, frame: &Frame) -> Result<bool> {
        if body["op"] != "restrict" || body["action"] != action {
            return Ok(false);
        }
        body.get("condition")
            .map_or(Ok(true), |condition| self.truth(condition, frame))
    }

    pub(super) fn continuous_window_active(&self, entry: &Value) -> Result<bool> {
        if entry["during"].is_null() {
            return Ok(true);
        }
        let phase = string(&entry["window_phase"]);
        if !matches!(phase, "start" | "main" | "turn") {
            return Err(EngineFailure::Unsupported(
                "restriction period needs a saved player, turn and phase".into(),
            ));
        }
        Ok(entry["window_player"] == self.active()
            && int(&entry["window_turn"]) == int(&self.state.turn["elapsed_turns"][self.active()])
            && (phase == "turn" || self.state.turn["phase"] == phase))
    }
}
