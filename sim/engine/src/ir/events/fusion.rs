//! A material trait is frozen once per Fusion, independent of the material count.
use super::played::PlayedFilter;
use super::subject::SubjectFilter;
use crate::ir::{
    Ability, AbilityOrigin, Binding, Bool, EventPattern, EventSnapshotPolicy, Expr, FusedEvent,
    Phase, PlayerFrozen, PlayerRef, Predicate, TraitId, TraitsFrozen, TriggerCondition, ZoneKind,
};
use core::slice::from_ref;

pub(crate) fn predicate(event: Binding<FusedEvent>, material: &TraitId) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::And {
        predicates: vec![
            Expr::Compute(Box::new(Predicate::PlayerEquals {
                left: Expr::Frozen(Box::new(PlayerFrozen::FusedController { event })),
                right: Expr::Literal(PlayerRef::Controller {}),
            })),
            Expr::Compute(Box::new(Predicate::HasTrait {
                traits: Expr::Frozen(Box::new(TraitsFrozen::FusedMaterialTraits { event })),
                trait_id: material.clone(),
            })),
        ],
    }))
}

fn material(pattern: &EventPattern) -> Option<TraitId> {
    let EventPattern::Fused {
        binding,
        predicate: expression,
    } = pattern
    else {
        return None;
    };
    let Expr::Compute(and) = expression else {
        return None;
    };
    let Predicate::And { predicates } = and.as_ref() else {
        return None;
    };
    let Expr::Compute(has) = predicates.get(1)? else {
        return None;
    };
    let Predicate::HasTrait { trait_id, .. } = has.as_ref() else {
        return None;
    };
    (!trait_id.0.is_empty() && *expression == predicate(*binding, trait_id))
        .then_some(trait_id.clone())
}

pub(crate) fn combined(pattern: &EventPattern) -> bool {
    let EventPattern::AnyOf { patterns } = pattern else {
        return false;
    };
    if !matches!(patterns.len(), 2 | 3) {
        return false;
    }
    let Some(trait_id) = patterns.iter().find_map(material) else {
        return false;
    };
    let mut fused = false;
    let mut played = false;
    let mut phase = false;
    for branch in patterns {
        if let EventPattern::Fused { .. } = branch {
            if fused || material(branch).as_ref() != Some(&trait_id) {
                return false;
            }
            fused = true;
        } else if let EventPattern::Play { binding, predicate } = branch {
            if played {
                return false;
            }
            let Some(plan) = PlayedFilter::parse(*binding, predicate) else {
                return false;
            };
            let expected = PlayedFilter {
                subject: SubjectFilter {
                    own: Some(true),
                    trait_id: Some(trait_id.clone()),
                    ..SubjectFilter::default()
                },
                card: plan.card,
                original: None,
                traits_any: vec![],
                alternate: None,
            };
            if *predicate != expected.predicate(*binding) {
                return false;
            }
            played = true;
        } else if let EventPattern::Phase { binding, predicate } = branch {
            if phase
                || *predicate
                    != super::timing::phase_predicate(
                        *binding,
                        Phase::Main,
                        Some(PlayerRef::Controller {}),
                    )
            {
                return false;
            }
            phase = true;
        } else {
            return false;
        }
    }
    fused && played
}

pub(crate) const fn branch_view(pattern: &EventPattern) -> Option<EventSnapshotPolicy> {
    if matches!(pattern, EventPattern::Fused { .. }) {
        Some(EventSnapshotPolicy::BeforeExit)
    } else if matches!(pattern, EventPattern::Play { .. }) {
        Some(EventSnapshotPolicy::AtReveal)
    } else if matches!(pattern, EventPattern::Phase { .. }) {
        Some(EventSnapshotPolicy::AfterEntry)
    } else {
        None
    }
}

pub(crate) fn patterns(pattern: &EventPattern) -> &[EventPattern] {
    if let EventPattern::AnyOf { patterns } = pattern {
        if combined(pattern) {
            return patterns;
        }
        return &[];
    }
    from_ref(pattern)
}

pub(crate) fn filter(ability: &Ability) -> Option<TraitId> {
    if !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        || !matches!(
            ability.active_zones.as_slice(),
            [ZoneKind::Field | ZoneKind::Ex]
        )
    {
        return None;
    }
    let trigger = ability.trigger.as_ref()?;
    if trigger.active_zones != ability.active_zones {
        return None;
    }
    let TriggerCondition::Event { pattern } = &trigger.on else {
        return None;
    };
    patterns(pattern).iter().find_map(material)
}

pub(crate) fn has_phase(ability: &Ability) -> bool {
    ability.trigger.as_ref().is_some_and(|trigger| {
        let TriggerCondition::Event { pattern } = &trigger.on else {
            return false;
        };
        combined(pattern)
            && patterns(pattern)
                .iter()
                .any(|branch| matches!(branch, EventPattern::Phase { .. }))
    })
}
