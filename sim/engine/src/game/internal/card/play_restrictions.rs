use super::{Game, PlayerId, Result, player_id};
use crate::ir::ZoneKind;
use crate::ir::play_restrictions::{Restriction, card_restriction};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Printed restrictions share the authoritative card play boundary."
)]
impl Game {
    pub(super) fn internal_card_play_forbidden(
        &self,
        holder: &str,
        actor: PlayerId,
    ) -> Result<bool> {
        let object = self.object(holder)?;
        for located in self.internal_printed_abilities(holder)? {
            let ability = &located.node;
            let in_zone = match object.zone.as_str() {
                "hand" => ability.active_zones.contains(&ZoneKind::Hand),
                "ex" => ability.active_zones.contains(&ZoneKind::Ex),
                _ => false,
            };
            if in_zone
                && match card_restriction(ability) {
                    Some(Restriction::FromEx) => true,
                    Some(Restriction::OwnTurn) => player_id(self.active())? == actor,
                    None => false,
                }
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
