//! Physical evolution resources carry snapshot information, never prototype programs.
use super::{Game, Result, invalid};
use crate::game::string;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Resource admission and its live role check share one private boundary."
)]
impl Game {
    pub(super) fn admit_evolution_resources(&mut self) -> Result<()> {
        for object in self.state.objects.values() {
            if object.zone != "evolve_deck"
                || self
                    .card_runtime
                    .definitions
                    .current
                    .contains_key(&object.card)
            {
                continue;
            }
            let face = self.catalog.face(&object.card, 0)?;
            if !string(&face["card_type"]).contains("エボルヴ") {
                return Err(invalid("evolution resource must have printed Evolve type"));
            }
            self.card_runtime
                .resources
                .insert(object.id.clone(), object.card.clone());
        }
        Ok(())
    }

    pub(in crate::game) fn internal_evolution_resource(&self, holder: &str) -> Result<bool> {
        let Some(card) = self.card_runtime.resources.get(holder) else {
            return Ok(false);
        };
        let object = self.object(holder)?;
        if &object.card != card || object.state["face"] != 0_u8 {
            return Err(invalid("evolution resource identity or face changed"));
        }
        match object.zone.as_str() {
            "evolve_deck" => Ok(true),
            "evolution" => {
                let bases = self
                    .state
                    .objects
                    .values()
                    .filter(|base| base.state["evolved_with"] == holder)
                    .collect::<Vec<_>>();
                let [base] = bases.as_slice() else {
                    return Err(invalid("evolution resource needs exactly one base"));
                };
                if base.zone != "field"
                    || base.owner != object.owner
                    || string(&self.catalog.face(&base.card, 0)?["name"])
                        != string(&self.catalog.face(card, 0)?["name"])
                {
                    return Err(invalid("invalid evolution resource association"));
                }
                Ok(true)
            }
            _ => Err(invalid("evolution resource left its admitted role")),
        }
    }

    /// A resource that entered play with printed text carries abilities nothing here loads; stop
    /// every later action (like a pending ability) instead of treating the resource as ability-less.
    pub(in crate::game) fn stop_unadmitted_resource(
        &mut self,
        holder: &str,
        face: usize,
    ) -> Result<()> {
        let Some(card) = self.card_runtime.resources.get(holder) else {
            return Ok(());
        };
        // Only a known-empty text and known-empty sections prove the face has no abilities.
        let printed = self.catalog.face(card, face)?;
        if printed["text"].as_str() != Some("")
            || printed["sections"].as_array().is_none_or(|s| !s.is_empty())
        {
            self.card_runtime.unadmitted.insert(holder.into());
        }
        Ok(())
    }
}
