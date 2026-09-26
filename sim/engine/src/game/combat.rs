#![expect(
    clippy::indexing_slicing,
    reason = "Validated static attack requirements use total JSON indexing."
)]
use super::{Game, int, other};
use crate::Result;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Attack obligations and legality share the authoritative combat state."
)]
impl Game {
    pub(super) fn attack_required(&self) -> Result<bool> {
        let mut targets = self.zone_ids(other(self.active()), "field");
        targets.push(format!("{}.leader", other(self.active())));
        for source in self.ability_sources() {
            let frame = self.frame_for(&source)?;
            for code in self.abilities(&source)? {
                let body = &code["body"];
                if code["kind"] != "static"
                    || body["op"] != "require_attack"
                    || !self.ability_zone(&source, &code)?
                {
                    continue;
                }
                if let Some(condition) = body.get("condition")
                    && !self.truth(condition, &frame)?
                {
                    continue;
                }
                let count = self.number(&body["count"], &frame)?.max(0);
                for id in self.select(&body["subjects"], &frame)? {
                    if self.object(&id)?.controller != self.active()
                        || int(&self.object(&id)?.state["attacks_this_turn"]) >= count
                    {
                        continue;
                    }
                    for target in &targets {
                        if self.can_attack(&id, target)? {
                            return Ok(true);
                        }
                    }
                }
            }
        }
        Ok(false)
    }
}
