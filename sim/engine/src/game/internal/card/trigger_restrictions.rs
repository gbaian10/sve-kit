use super::super::player_id;
use super::{Ability, Game, Located, Result, unsupported};
use crate::ir::{
    AbilityOrigin, AbilityQualification, ObjectOperand, PlayerId, PlayerRef, ZoneKind,
    trigger_restrictions,
};

pub(super) fn check(located: &Located<Ability>) -> Result<()> {
    let ability = &located.node;
    if !trigger_restrictions::is_static(ability)
        || ability.active_zones != [ZoneKind::Field]
        || ability.qualification != AbilityQualification::Ordinary
        || ability.source.holder != (ObjectOperand::SourceHolder {})
        || ability.source.controller != (PlayerRef::Controller {})
        || ability.play.is_some()
        || ability.trigger.is_some()
        || ability.body.is_some()
        || !ability.event_captures.is_empty()
    {
        return Err(unsupported("static trigger permission header"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Trigger permissions inspect the effective field at the occurrence boundary."
)]
impl Game {
    pub(in crate::game::internal) fn internal_trigger_forbidden(
        &self,
        ability: &Ability,
        controller: PlayerId,
    ) -> Result<bool> {
        let AbilityOrigin::Keyword { keyword, .. } = &ability.source.origin else {
            return Ok(false);
        };
        for provider in self.field_ids() {
            let (card, _) = self.internal_effective_print(&provider)?;
            if !self.card_runtime.is_loaded(&card)
                || player_id(&self.object(&provider)?.controller)? == controller
            {
                continue;
            }
            for located in self.internal_printed_abilities(&provider)? {
                if trigger_restrictions::is_static(&located.node)
                    && located
                        .node
                        .declarations
                        .iter()
                        .any(|effect| trigger_restrictions::keyword(effect) == Some(*keyword))
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}
