use super::{Game, Result};
use crate::ir::{PlayerRelation, draw_restrictions};

impl Game {
    pub(in crate::game) fn internal_draw_forbidden(&self, seat: &str) -> Result<bool> {
        if self.state.turn["phase"] == "start" {
            return Ok(false);
        }
        for provider in self.field_ids() {
            let (card, _) = self.internal_effective_print(&provider)?;
            if !self.card_runtime.is_loaded(&card) {
                continue;
            }
            let own = self.object(&provider)?.controller == seat;
            for ability in self.internal_effective_abilities(&provider)? {
                if let Some(relation) = draw_restrictions::projection(&ability.node)
                    && own == (*relation == PlayerRelation::SelfPlayer)
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}
