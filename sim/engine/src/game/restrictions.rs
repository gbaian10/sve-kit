use super::Game;
use crate::Result;

impl Game {
    pub(super) fn restricted(&self, id: &str, action: &str) -> Result<bool> {
        if id
            .strip_suffix(".leader")
            .is_some_and(|seat| self.internal_phase_restricted(seat, action))
        {
            return Ok(true);
        }
        if let Some(seat) = id.strip_suffix(".leader")
            && action == "draw"
            && self.internal_draw_forbidden(seat)?
        {
            return Ok(true);
        }
        if let Some(seat) = id.strip_suffix(".leader")
            && matches!(action, "win" | "lose")
            && self.internal_result_forbidden(seat, action == "win")?
        {
            return Ok(true);
        }
        if (action == "ignore_guard" && self.internal_ignores_ward(id)?)
            || self.internal_removal_restricted(id, action)?
        {
            return Ok(true);
        }
        self.require_ability_sources(&format!("restriction {action}"))?;
        Ok(false)
    }
}
