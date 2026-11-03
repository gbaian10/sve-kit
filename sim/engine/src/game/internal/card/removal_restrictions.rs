use super::{Ability, Game, Located, Result, unsupported};
use crate::ir::removal_restrictions::{self, Removal, projection};
use crate::ir::{AbilityQualification, ObjectOperand, PlayerRef, ZoneKind};

pub(super) fn check(located: &Located<Ability>) -> Result<()> {
    let ability = &located.node;
    if !removal_restrictions::is_static(ability)
        || ability.qualification != AbilityQualification::Ordinary
        || ability.active_zones != [ZoneKind::Field]
        || ability.source.holder != (ObjectOperand::SourceHolder {})
        || ability.source.controller != (PlayerRef::Controller {})
        || ability.play.is_some()
        || ability.trigger.is_some()
        || ability.body.is_some()
        || !ability.event_captures.is_empty()
    {
        return Err(unsupported("printed holder removal restriction"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Removal permissions use effective printed abilities on the authoritative object."
)]
impl Game {
    pub(in crate::game) fn internal_removal_restricted(
        &self,
        id: &str,
        action: &str,
    ) -> Result<bool> {
        let kind = match action {
            "ability_destroy" => Removal::Destroy,
            "banish" => Removal::Banish,
            _ => return Ok(false),
        };
        if self.object(id)?.zone != "field" {
            return Ok(false);
        }
        let (card, _) = self.internal_effective_print(id)?;
        if !self.card_runtime.is_loaded(&card) {
            return Ok(false);
        }
        Ok(self.internal_printed_abilities(id)?.iter().any(|ability| {
            (removal_restrictions::is_static(&ability.node)
                || removal_restrictions::is_mixed_static(&ability.node))
                && ability
                    .node
                    .declarations
                    .iter()
                    .any(|effect| projection(effect) == Some(kind))
        }))
    }
}
