use super::{
    Ability, AbilityKind, AbilityOrigin, ActionPattern, Bool, Cardinality, Duration, DurationEnd,
    Effect, Expr, Nat, PermissionSubject, PlayerRef, PlayerRelation, Polarity, TextQuantifier,
    TimeAnchor,
};

pub(crate) fn permission(effect: &Effect) -> Option<(&PlayerRelation, bool)> {
    let Effect::Permission {
        subject: PermissionSubject::Players { players },
        action,
        polarity: Polarity::Forbid,
        condition: Expr::Literal(Bool(true)),
        duration,
        ..
    } = effect
    else {
        return None;
    };
    let (affected, win) = match action {
        ActionPattern::Win { players: targets } => (targets, true),
        ActionPattern::Lose { players: targets } => (targets, false),
        ActionPattern::PlayCard { .. }
        | ActionPattern::PlayAbility { .. }
        | ActionPattern::Attack { .. }
        | ActionPattern::ChooseTarget { .. }
        | ActionPattern::Draw { .. }
        | ActionPattern::Destroy { .. }
        | ActionPattern::Banish { .. }
        | ActionPattern::Stand { .. }
        | ActionPattern::IncreasePpMaximum { .. }
        | ActionPattern::IgnoreWard { .. }
        | ActionPattern::Reroll { .. }
        | ActionPattern::ChooseMode { .. }
        | ActionPattern::TriggerAdmission { .. } => return None,
    };
    (players == affected
        && matches!(
            players.relation,
            PlayerRelation::SelfPlayer | PlayerRelation::Opponents
        )
        && players.relative_to == (PlayerRef::Controller {})
        && players.cardinality
            == (Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            })
        && players.text_quantifier == TextQuantifier::Implicit
        && *duration
            == (Duration {
                end: DurationEnd::NoTemporalExpiry {},
                anchor: TimeAnchor::Installation {},
                usage: None,
                intervals: vec![],
            }))
    .then_some((&players.relation, win))
}

pub(crate) fn is_static(ability: &Ability) -> bool {
    ability.kind == AbilityKind::Static
        && matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        && !ability.declarations.is_empty()
        && ability
            .declarations
            .iter()
            .all(|effect| permission(effect).is_some())
}
