//! A live printed provider supplies the population counted by `SpellChain`.
use super::{
    Ability, AbilityId, AbilityKind, AbilityOrigin, AbilityQualification, AbilitySource, Bool,
    Duration, DurationEnd, Effect, Expr, ObjectOperand, PlayerRef, RuleModifier, ScopeId, Selector,
    TimeAnchor, ZoneKind,
};

pub(crate) fn ability(
    source: AbilitySource,
    scope: ScopeId,
    player: PlayerRef,
    population: Selector,
    condition: Expr<Bool>,
) -> Ability {
    Ability {
        id: AbilityId(scope.0),
        scope,
        source: source.clone(),
        kind: AbilityKind::Static,
        qualification: AbilityQualification::Ordinary,
        active_zones: vec![ZoneKind::Field],
        event_captures: vec![],
        play: None,
        trigger: None,
        body: None,
        declarations: vec![Effect::RuleModifier {
            modifier: RuleModifier::SpellChainPopulation { player, population },
            condition,
            duration: Duration {
                end: DurationEnd::NoTemporalExpiry {},
                anchor: TimeAnchor::Installation {},
                usage: None,
                intervals: vec![],
            },
            source,
        }],
    }
}

pub(crate) fn projection(candidate: &Ability) -> Option<(&PlayerRef, &Selector, &Expr<Bool>)> {
    let [
        Effect::RuleModifier {
            modifier: RuleModifier::SpellChainPopulation { player, population },
            condition,
            ..
        },
    ] = candidate.declarations.as_slice()
    else {
        return None;
    };
    if !matches!(candidate.source.origin, AbilityOrigin::Printed { .. })
        || candidate.source.holder != (ObjectOperand::SourceHolder {})
        || candidate.source.controller != (PlayerRef::Controller {})
    {
        return None;
    }
    let mut expected = ability(
        candidate.source.clone(),
        candidate.scope,
        player.clone(),
        population.clone(),
        condition.clone(),
    );
    expected.id = candidate.id;
    (*candidate == expected).then_some((player, population, condition))
}
