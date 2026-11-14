//! Declaration routing for composable printed static providers.
use super::{Ability, AbilityKind, AbilityOrigin, CharacteristicPatch, ContinuousLayer, Effect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Family {
    Grant,
    Prevention,
    Cost,
    MoveReplacement,
    TriggerMultiplicity,
}

pub(crate) fn family(effect: &Effect) -> Option<Family> {
    match effect {
        Effect::Replacement { .. } if super::move_replacement::projection(effect).is_some() => {
            Some(Family::MoveReplacement)
        }
        Effect::RuleModifier { .. }
            if super::trigger_multiplicity::projection(effect).is_some() =>
        {
            Some(Family::TriggerMultiplicity)
        }
        Effect::Continuous {
            patch: CharacteristicPatch::GrantAbility { .. },
            layer: ContinuousLayer::Ability,
            ..
        } => Some(Family::Grant),
        Effect::Replacement { .. } if super::prevention::projection(effect).is_some() => {
            Some(Family::Prevention)
        }
        Effect::CostModifier { .. } => Some(Family::Cost),
        Effect::Continuous { .. }
        | Effect::Replacement { .. }
        | Effect::Permission { .. }
        | Effect::RuleModifier { .. }
        | Effect::DeckRule { .. } => None,
    }
}

pub(crate) fn contains(ability: &Ability, provider: Family) -> bool {
    ability.kind == AbilityKind::Static
        && matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        && ability
            .declarations
            .iter()
            .any(|effect| family(effect) == Some(provider))
}

pub(crate) fn is_static(ability: &Ability) -> bool {
    ability.kind == AbilityKind::Static
        && matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        && !ability.declarations.is_empty()
        && ability
            .declarations
            .iter()
            .all(|effect| family(effect).is_some())
}
