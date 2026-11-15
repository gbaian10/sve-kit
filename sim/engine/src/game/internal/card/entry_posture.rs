use super::{Ability, Game, Located, Result, printed_origin, unsupported};
use crate::ir::{
    AbilityKind, AbilityOrigin, AbilityQualification, Effect, ObjectOperand, PlayerRef, Posture,
    ZoneKind, entry_posture,
};

pub(super) fn check(located: &Located<Ability>, declarations: &[Effect]) -> Result<()> {
    let ability = &located.node;
    if ability.kind != AbilityKind::Static
        || !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        || ability.qualification != AbilityQualification::Ordinary
        || ability.active_zones != [ZoneKind::Field]
        || ability.source.holder != (ObjectOperand::SourceHolder {})
        || ability.source.controller != (PlayerRef::Controller {})
        || ability.play.is_some()
        || ability.trigger.is_some()
        || ability.body.is_some()
        || !ability.event_captures.is_empty()
    {
        return Err(unsupported("prospective entry posture header"));
    }
    for effect in declarations {
        let Effect::Replacement { source, .. } = effect else {
            return Err(unsupported("entry posture replacement"));
        };
        if source != &ability.source || entry_posture::projection(effect) != Some(Posture::Acted) {
            return Err(unsupported("entry posture replacement source"));
        }
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Every movement route shares the prospective printed entry rule."
)]
impl Game {
    pub(in crate::game) fn internal_entry_acted(&self, card: &str, face: usize) -> Result<bool> {
        let Some(program) = self.card_runtime.definitions.cards.get(card) else {
            return Ok(false);
        };
        for ability in &program.card().abilities {
            let (printed, _) = printed_origin(&ability.node.source.origin)?;
            if self.printed_face_ordinal(card, printed)? == face
                && ability.node.active_zones == [ZoneKind::Field]
                && ability
                    .node
                    .declarations
                    .iter()
                    .any(|effect| entry_posture::projection(effect) == Some(Posture::Acted))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
