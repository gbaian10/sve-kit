use super::{Game, Result, invalid, seat_name};
use crate::ir::{Nat, PlayerId, Selector, spell_chain};
use alloc::collections::BTreeSet;

pub(super) fn check_population(population: &Selector) -> Result<()> {
    if let Selector::Union { sets } = population {
        for set in sets {
            check_population(set)?;
        }
        Ok(())
    } else {
        super::card::selection::public_zone_selector(population)
    }
}

impl Game {
    pub(in crate::game::internal) fn internal_spell_chain(&self, player: PlayerId) -> Result<Nat> {
        let mut population = BTreeSet::new();
        let mut replaced = false;
        for provider in self.field_ids() {
            let (card, _) = self.internal_effective_print(&provider)?;
            if !self.card_runtime.is_loaded(&card)
                && !self.internal_has_local_definitions(&provider)
            {
                continue;
            }
            for located in self.internal_effective_abilities(&provider)? {
                let Some((who, objects, condition)) = spell_chain::projection(&located.node) else {
                    continue;
                };
                let execution = self.provider_execution(&provider, None)?;
                if execution.player(self, who)? == player && execution.boolean(self, condition)?.0 {
                    replaced = true;
                    population.extend(
                        execution
                            .objects(self, objects)?
                            .into_iter()
                            .map(|object| (object.id.0, object.generation.0)),
                    );
                }
            }
        }
        if replaced {
            return Ok(Nat(u64::try_from(population.len()).map_err(invalid)?));
        }
        let mut count = 0_u64;
        for id in self.zone_ids(seat_name(player)?, "cemetery") {
            if self.object_type(&id)? == "スペル" {
                count = count
                    .checked_add(1)
                    .ok_or_else(|| invalid("SpellChain count overflow"))?;
            }
        }
        Ok(Nat(count))
    }
}
