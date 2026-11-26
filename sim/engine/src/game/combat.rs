use super::Game;
use crate::Result;

impl Game {
    pub(super) fn attack_required(&self) -> Result<bool> {
        if self.internal_attack_required()? {
            return Ok(true);
        }
        self.require_ability_sources("attack requirement")?;
        Ok(false)
    }
}
