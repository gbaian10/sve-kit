//! Printed movement headers share the immutable event filter plan.
use super::{Binding, EventPattern, LoweringError, ScopeId, ast, unsupported};
use crate::ir::events::ex_departure::ExDepartureFilter;
use crate::ir::events::mill::MillFilter;
use crate::ir::events::played::PlayedFilter;
use crate::ir::events::subject::{SubjectFields, SubjectFilter};
use crate::ir::history_turn::source_link::SourceLink;
use crate::ir::{
    CardKind, CardName, ClassId, Comparison, KeywordId, ObjectRef, PlayEvent, TraitId, ZoneKind,
};

#[derive(Clone, Copy)]
pub(super) enum FilterSupport {
    Basic,
    OriginalCost,
    EntryCost,
    CemeteryWard,
}

pub(super) fn subscription(
    on: &ast::PhaseSubscription,
    event_id: u64,
    subject_id: u64,
    scope: ScopeId,
    path: &str,
) -> Result<EventPattern, LoweringError> {
    match on {
        ast::PhaseSubscription::Moved(entry) => moved(entry, event_id, subject_id, scope, path),
        ast::PhaseSubscription::PlacedEx(entry) => {
            if entry
                .sel
                .as_ref()
                .is_some_and(|sel| sel.token.is_some() || sel.not_token.is_some())
            {
                return Err(unsupported(path));
            }
            let event = Binding::new(event_id, scope);
            Ok(EventPattern::EnterEx {
                binding: event,
                predicate: movement_filter(
                    entry
                        .who
                        .as_ref()
                        .map(|who| matches!(who, ast::EntryPlayer::Controller)),
                    entry.reference.is_some(),
                    entry.sel.as_ref(),
                    entry.during.is_some(),
                    path,
                    FilterSupport::Basic,
                )?
                .predicate(SubjectFields::Ex(event)),
            })
        }
        ast::PhaseSubscription::LeaveField(entry) => {
            let event = Binding::new(event_id, scope);
            Ok(EventPattern::LeaveField {
                binding: event,
                predicate: movement_filter(
                    entry
                        .who
                        .as_ref()
                        .map(|who| matches!(who, ast::EntryPlayer::Controller)),
                    entry.reference.is_some(),
                    entry.sel.as_ref(),
                    entry.during.is_some(),
                    path,
                    FilterSupport::OriginalCost,
                )?
                .predicate(SubjectFields::Leave(event)),
            })
        }
        ast::PhaseSubscription::Destroyed(entry) => {
            if entry
                .sel
                .as_ref()
                .is_some_and(|sel| sel.token.is_some() || sel.not_token.is_some())
            {
                return Err(unsupported(path));
            }
            let event = Binding::new(event_id, scope);
            Ok(EventPattern::Destroy {
                binding: event,
                predicate: movement_filter(
                    entry
                        .who
                        .as_ref()
                        .map(|who| matches!(who, ast::EntryPlayer::Controller)),
                    entry.reference.is_some(),
                    entry.sel.as_ref(),
                    entry.during.is_some(),
                    path,
                    FilterSupport::Basic,
                )?
                .predicate(SubjectFields::Destroy(event)),
            })
        }
        ast::PhaseSubscription::Fused(_)
        | ast::PhaseSubscription::FusedAsMaterial(_)
        | ast::PhaseSubscription::AnyOf(_)
        | ast::PhaseSubscription::Acted(_)
        | ast::PhaseSubscription::Stood(_)
        | ast::PhaseSubscription::StatChanged(_)
        | ast::PhaseSubscription::Evolved(_)
        | ast::PhaseSubscription::UbActivated(_)
        | ast::PhaseSubscription::Played(_)
        | ast::PhaseSubscription::Phase(_)
        | ast::PhaseSubscription::EnterField(_)
        | ast::PhaseSubscription::Damage(_)
        | ast::PhaseSubscription::Attacked(_)
        | ast::PhaseSubscription::Dealt(_)
        | ast::PhaseSubscription::DriveChecked(_)
        | ast::PhaseSubscription::DriveTrigger(_)
        | ast::PhaseSubscription::Drew(_)
        | ast::PhaseSubscription::Discarded(_)
        | ast::PhaseSubscription::DiceRolled(_)
        | ast::PhaseSubscription::Banished(_)
        | ast::PhaseSubscription::LifeChanged(_)
        | ast::PhaseSubscription::Equipped(_) => Err(unsupported(path)),
    }
}

/// 「…が墓場／手札に置かれたとき」 from the field, or from a named player's deck.
fn moved(
    entry: &ast::MoveSubscription,
    event_id: u64,
    damage_id: u64,
    scope: ScopeId,
    path: &str,
) -> Result<EventPattern, LoweringError> {
    let (own, zone) = match &entry.from {
        ast::MoveOrigin::Zone(zone) => (None, zone),
        ast::MoveOrigin::Owned { who, zone } => {
            (Some(matches!(who, ast::EntryPlayer::Controller)), zone)
        }
    };
    if *zone == ast::MoveZone::Ex {
        let own = own.ok_or_else(|| unsupported(path))?;
        if entry.reference.is_some() || entry.sel.is_some() {
            return Err(unsupported(path));
        }
        let event = Binding::new(event_id, scope);
        return Ok(EventPattern::Move {
            binding: event,
            predicate: ExDepartureFilter {
                own,
                own_turn: entry.during.is_some(),
                to: entry.to.as_ref().map(|to| match to {
                    ast::DepartureDestination::Cemetery => ZoneKind::Cemetery,
                    ast::DepartureDestination::Hand => ZoneKind::Hand,
                }),
            }
            .predicate(event),
        });
    }
    if *zone == ast::MoveZone::Deck {
        return mill(entry, own, event_id, scope, path);
    }
    let event = Binding::new(event_id, scope);
    let mut subject = entry.sel.clone();
    let link = subject.as_mut().and_then(|subject| subject.r#where.take());
    let filter = movement_filter(
        own,
        entry.reference.is_some(),
        subject.as_ref(),
        entry.during.is_some(),
        path,
        if own == Some(true) && matches!(entry.to, Some(ast::DepartureDestination::Cemetery)) {
            FilterSupport::CemeteryWard
        } else {
            FilterSupport::Basic
        },
    )?;
    let predicate = if let Some(link) = link {
        let history = &link.history;
        let ast::DealtDamageHistory::DealtDamage = history.event;
        let ast::ThisTurn::ThisTurn = history.window;
        let ast::Item::Item = history.r#where.recipient;
        if own != Some(false)
            || filter.kind != Some(CardKind::Follower)
            || !matches!(entry.to, Some(ast::DepartureDestination::Cemetery))
            || entry.during.is_none()
        {
            return Err(unsupported(path));
        }
        let source = match (&history.who, &history.sel, &history.r#where.reference) {
            (None, None, Some(ast::This::This)) => SubjectFilter {
                itself: Some(true),
                ..SubjectFilter::default()
            },
            (Some(ast::LeaderSide::Controller), Some(source_subject), None) => SubjectFilter {
                own: Some(true),
                trait_id: Some(TraitId(source_subject.r#trait.clone())),
                ..SubjectFilter::default()
            },
            _ => return Err(unsupported(path)),
        };
        let predicate = SourceLink {
            subject: filter,
            source,
            damage: Binding::new(damage_id, scope),
        }
        .predicate(event);
        if SourceLink::parse(event, &predicate).is_none() {
            return Err(unsupported(path));
        }
        predicate
    } else {
        filter.predicate(match entry.to {
            Some(ast::DepartureDestination::Cemetery) => SubjectFields::Cemetery(event),
            Some(ast::DepartureDestination::Hand) => SubjectFields::Hand(event),
            None => SubjectFields::Leave(event),
        })
    };
    Ok(EventPattern::LeaveField {
        binding: event,
        predicate,
    })
}

/// A milled card has no field information, so only its owner and the turn can filter it.
fn mill(
    entry: &ast::MoveSubscription,
    own: Option<bool>,
    event_id: u64,
    scope: ScopeId,
    path: &str,
) -> Result<EventPattern, LoweringError> {
    let Some(own) = own else {
        return Err(unsupported(path));
    };
    if entry.reference.is_some()
        || entry.sel.is_some()
        || !matches!(entry.to, Some(ast::DepartureDestination::Cemetery))
    {
        return Err(unsupported(path));
    }
    let event = Binding::new(event_id, scope);
    Ok(EventPattern::Move {
        binding: event,
        predicate: MillFilter {
            own,
            own_turn: entry.during.is_some(),
        }
        .predicate(event),
    })
}

pub(super) fn movement_filter(
    own: Option<bool>,
    itself: bool,
    subject: Option<&ast::MovementSubject>,
    own_turn: bool,
    path: &str,
    support: FilterSupport,
) -> Result<SubjectFilter, LoweringError> {
    if subject.is_some_and(|sel| sel.r#where.is_some()) {
        return Err(unsupported(path));
    }
    if subject.is_some_and(|sel| {
        sel.not_class.as_ref().is_some_and(|class| {
            class.is_empty() || sel.class.is_some() || !matches!(support, FilterSupport::EntryCost)
        })
    }) {
        return Err(unsupported(path));
    }
    // Only an entering subject reads a trait union (BP21-008), never beside a single trait.
    let traits_any = match subject.and_then(|sel| sel.traits_any.as_ref()) {
        None => Vec::new(),
        Some(traits)
            if matches!(support, FilterSupport::EntryCost)
                && subject.is_some_and(|sel| sel.trait_id.is_none())
                && traits.len() >= 2
                && traits.iter().enumerate().all(|(index, id)| {
                    !id.is_empty() && !traits.iter().take(index).any(|previous| previous == id)
                }) =>
        {
            traits.iter().cloned().map(TraitId).collect()
        }
        Some(_) => return Err(unsupported(path)),
    };
    // Only an entering subject reads a name fragment (BP13-027).
    if subject.is_some_and(|sel| {
        sel.name_contains.as_ref().is_some_and(|fragment| {
            fragment.is_empty() || !matches!(support, FilterSupport::EntryCost)
        })
    }) {
        return Err(unsupported(path));
    }
    let bounds = subject
        .map_or([None, None, None], |sel| {
            [
                sel.orig_cost_eq.map(|n| (Comparison::Equal, n)),
                sel.orig_cost_le.map(|n| (Comparison::LessEqual, n)),
                sel.orig_cost_ge.map(|n| (Comparison::GreaterEqual, n)),
            ]
        })
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let original_cost = match bounds.as_slice() {
        [] => None,
        [(comparison, cost)] => Some((
            *comparison,
            i64::try_from(*cost).map_err(|_overflow| unsupported(path))?,
        )),
        _ => return Err(unsupported(path)),
    };
    let keyword = subject.and_then(|sel| sel.keyword).map(|value| value.0);
    let allow_original_cost = matches!(
        support,
        FilterSupport::OriginalCost | FilterSupport::EntryCost
    );
    if (original_cost.is_some() && !allow_original_cost)
        || keyword.is_some_and(|keyword| {
            keyword != KeywordId::Ward || !matches!(support, FilterSupport::CemeteryWard)
        })
    {
        return Err(unsupported(path));
    }
    Ok(SubjectFilter {
        keyword,
        original_cost,
        own,
        itself: if itself {
            Some(true)
        } else {
            subject
                .and_then(|sel| sel.other)
                .filter(|other| *other)
                .map(|_| false)
        },
        kind: subject
            .and_then(|sel| sel.kind.as_ref())
            .map(super::search::card_kind),
        trait_id: subject
            .and_then(|sel| sel.trait_id.as_ref())
            .map(|id| TraitId(id.clone())),
        traits_any,
        name: subject
            .and_then(|sel| sel.name.as_ref())
            .map(|name| CardName(name.clone())),
        name_contains: subject
            .and_then(|sel| sel.name_contains.as_ref())
            .map(|fragment| CardName(fragment.clone())),
        name_contains_any: Vec::new(),
        not_class: subject
            .and_then(|sel| sel.not_class.as_ref())
            .map(|class| ClassId(class.clone())),
        class: subject
            .and_then(|sel| sel.class.as_ref())
            .map(|class| ClassId(class.clone())),
        token: subject.and_then(|sel| sel.token.or_else(|| sel.not_token.map(|value| !value))),
        own_turn,
    })
}

pub(super) fn played(
    entry: &ast::PlayedSubscription,
    event: Binding<PlayEvent>,
    card: Binding<ObjectRef>,
    path: &str,
) -> Result<EventPattern, LoweringError> {
    let (mut subject, alternate) = match &entry.sel {
        None => (None, None),
        Some(ast::PlayedSubject::Single(subject)) => (Some(subject.clone()), None),
        Some(ast::PlayedSubject::Union { any_of }) => {
            let [first, second] = any_of.as_ref();
            (Some(first.clone()), Some(second))
        }
    };
    let mut original = None;
    let mut traits_any = Vec::new();
    if let Some(q) = subject.as_mut() {
        let bounds = [
            q.orig_cost_eq.take().map(|n| (Comparison::Equal, n)),
            q.orig_cost_le.take().map(|n| (Comparison::LessEqual, n)),
            q.orig_cost_ge.take().map(|n| (Comparison::GreaterEqual, n)),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        original = match bounds.as_slice() {
            [] => None,
            [(op, n)] => Some((
                *op,
                i64::try_from(*n).map_err(|_overflow| unsupported(path))?,
            )),
            _ => return Err(unsupported(path)),
        };
        if let Some(traits) = q.traits_any.take() {
            if traits.len() < 2 || q.other.is_some() {
                return Err(unsupported(path));
            }
            traits_any = traits.into_iter().map(TraitId).collect();
        }
    }
    let common = movement_filter(
        entry
            .who
            .as_ref()
            .map(|who| matches!(who, ast::EntryPlayer::Controller)),
        false,
        subject.as_ref(),
        entry
            .during
            .as_ref()
            .is_some_and(|during| matches!(during.turn, ast::SelfPlayer::Controller)),
        path,
        FilterSupport::Basic,
    )?;
    let plan = PlayedFilter {
        subject: common.clone(),
        card,
        original,
        traits_any,
        alternate: alternate
            .map(|branch| {
                movement_filter(
                    common.own,
                    false,
                    Some(branch),
                    common.own_turn,
                    path,
                    FilterSupport::Basic,
                )
            })
            .transpose()?,
    };
    if !plan.valid() {
        return Err(unsupported(path));
    }
    Ok(EventPattern::Play {
        binding: event,
        predicate: plan.predicate(event),
    })
}

pub(super) fn entry_union(
    entries: &[ast::PhaseSubscription],
    event_id: u64,
    scope: ScopeId,
    path: &str,
) -> Result<EventPattern, LoweringError> {
    if entries.len() < 2 {
        return Err(unsupported(path));
    }
    let mut common = None;
    let mut fragments = Vec::new();
    for entry in entries {
        let ast::PhaseSubscription::EnterField(entry) = entry else {
            return Err(unsupported(path));
        };
        let mut filter = movement_filter(
            entry
                .who
                .as_ref()
                .map(|side| matches!(side, ast::EntryPlayer::Controller)),
            entry.reference.is_some(),
            entry.sel.as_ref(),
            entry.during.is_some(),
            path,
            FilterSupport::EntryCost,
        )?;
        let fragment = filter
            .name_contains
            .take()
            .ok_or_else(|| unsupported(path))?;
        if fragments.contains(&fragment) || common.as_ref().is_some_and(|common| *common != filter)
        {
            return Err(unsupported(path));
        }
        common = Some(filter);
        fragments.push(fragment);
    }
    let mut filter = common.ok_or_else(|| unsupported(path))?;
    filter.name_contains_any = fragments;
    let event = Binding::new(event_id, scope);
    let predicate = filter.predicate(SubjectFields::Entry(event));
    if SubjectFilter::parse(SubjectFields::Entry(event), &predicate).is_none() {
        return Err(unsupported(path));
    }
    Ok(EventPattern::EnterField {
        binding: event,
        predicate,
    })
}
