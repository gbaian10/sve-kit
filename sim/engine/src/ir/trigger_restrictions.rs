//! A field aura suppresses only opponent keyword occurrences, before they become pending.
use super::{
    Ability, AbilityKind, AbilityOrigin, ActionPattern, Bool, Cardinality, Duration, DurationEnd,
    Effect, Expr, KeywordId, Nat, PermissionSubject, PlayerRef, PlayerRelation, Polarity,
    TextQuantifier, TimeAnchor,
};

pub(crate) fn keyword(effect: &Effect) -> Option<KeywordId> {
    let Effect::Permission {
        subject: PermissionSubject::Players { players },
        action: ActionPattern::TriggerAdmission { predicate },
        polarity: Polarity::Forbid,
        condition: Expr::Literal(Bool(true)),
        duration,
        ..
    } = effect
    else {
        return None;
    };
    if players.relation != PlayerRelation::Opponents
        || players.relative_to != (PlayerRef::Controller {})
        || players.cardinality
            != (Cardinality {
                minimum: Nat(0),
                maximum: None,
            })
        || players.text_quantifier != TextQuantifier::Implicit
        || predicate.kind != Some(AbilityKind::Triggered)
        || !predicate.except.is_empty()
        || predicate.holder.is_some()
        || predicate.origin.is_some()
        || *duration
            != (Duration {
                end: DurationEnd::NoTemporalExpiry {},
                anchor: TimeAnchor::Installation {},
                usage: None,
                intervals: vec![],
            })
    {
        return None;
    }
    predicate
        .keyword
        .filter(|keyword| matches!(keyword, KeywordId::Fanfare | KeywordId::OnEvolve))
}

pub(crate) fn is_static(ability: &Ability) -> bool {
    ability.kind == AbilityKind::Static
        && matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        && !ability.declarations.is_empty()
        && ability
            .declarations
            .iter()
            .all(|effect| keyword(effect).is_some())
}
