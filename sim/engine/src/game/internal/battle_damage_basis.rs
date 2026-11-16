use super::{Game, Result, invalid};
use crate::ir::battle_damage_basis;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Combat reads live printed providers from the shared game authority."
)]
impl Game {
    pub(in crate::game) fn internal_combat_damage(&self, source: &str) -> Result<i64> {
        let health = self.health_combat_basis(source)?;
        // Numeric layers already synchronize state before combat; reuse that authoritative value.
        self.object(source)?.state[if health { "hp" } else { "power" }]
            .as_i64()
            .ok_or_else(|| invalid("invalid combat damage characteristic"))
    }

    fn health_combat_basis(&self, source: &str) -> Result<bool> {
        for provider in self.field_ids() {
            let (card, _) = self.internal_effective_print(&provider)?;
            if !self.card_runtime.is_loaded(&card)
                && !self.internal_has_local_definitions(&provider)
            {
                continue;
            }
            for located in self.internal_effective_abilities(&provider)? {
                let Some((objects, condition)) = battle_damage_basis::projection(&located.node)
                else {
                    continue;
                };
                let execution = self.provider_execution(&provider, None)?;
                if execution.boolean(self, condition)?.0
                    && execution
                        .objects(self, objects)?
                        .contains(&execution.reference(self, source)?)
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}
