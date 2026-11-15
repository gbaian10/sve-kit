//! A card's prospective field characteristics supply its intrinsic entry posture.
use super::{
    AbilitySource, Bool, CharacteristicView, Duration, DurationEnd, Effect, EffectDefinitionId,
    EventRewrite, Expr, ObjectOperand, OncePerEvent, PlayerRef, Posture, ProposedEventPattern,
    ReplacementScope, Selector, TimeAnchor,
};

pub(crate) fn replacement(id: EffectDefinitionId, source: AbilitySource) -> Effect {
    Effect::Replacement {
        id,
        source,
        pattern: ProposedEventPattern::EnterField {
            objects: Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
        },
        condition: Expr::Literal(Bool(true)),
        rewrite: EventRewrite::EntryPosture {
            posture: Posture::Acted,
        },
        scope: ReplacementScope::General,
        affected_player: PlayerRef::ProposedMoveController {},
        once_per_event: OncePerEvent::Enforced,
        usage: None,
        prospective_chars: Some(CharacteristicView::ProspectiveEntry),
        duration: Duration {
            end: DurationEnd::NoTemporalExpiry {},
            anchor: TimeAnchor::Installation {},
            usage: None,
            intervals: vec![],
        },
    }
}

pub(crate) fn projection(effect: &Effect) -> Option<Posture> {
    let Effect::Replacement { id, source, .. } = effect else {
        return None;
    };
    (*effect == replacement(*id, source.clone())).then_some(Posture::Acted)
}
