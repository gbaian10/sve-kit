//! Printed movement headers share the immutable event filter plan.
use super::{Binding, EventPattern, LoweringError, ScopeId, ast, unsupported};
use crate::ir::events::mill::MillFilter;
use crate::ir::events::subject::{SubjectFields, SubjectFilter};
use crate::ir::{CardKind, CardName, ClassId, TraitId};

#[expect(
    clippy::too_many_lines,
    reason = "The closed movement event mapping keeps its subject filters together."
)]
pub(super) fn subscription(
    on: &ast::PhaseSubscription,
    event_id: u64,
    scope: ScopeId,
    path: &str,
) -> Result<EventPattern, LoweringError> {
    match on {
        ast::PhaseSubscription::Moved(entry) => moved(entry, event_id, scope, path),
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
                    false,
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
                    true,
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
                    false,
                )?
                .predicate(SubjectFields::Destroy(event)),
            })
        }
        ast::PhaseSubscription::Acted(_)
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
        | ast::PhaseSubscription::Discarded(_)
        | ast::PhaseSubscription::LifeChanged(_) => Err(unsupported(path)),
    }
}

/// 「…が墓場／手札に置かれたとき」 from the field, or from a named player's deck.
fn moved(
    entry: &ast::MoveSubscription,
    event_id: u64,
    scope: ScopeId,
    path: &str,
) -> Result<EventPattern, LoweringError> {
    let (own, zone) = match &entry.from {
        ast::MoveOrigin::Zone(zone) => (None, zone),
        ast::MoveOrigin::Owned { who, zone } => {
            (Some(matches!(who, ast::EntryPlayer::Controller)), zone)
        }
    };
    if *zone == ast::MoveZone::Deck {
        return mill(entry, own, event_id, scope, path);
    }
    let event = Binding::new(event_id, scope);
    Ok(EventPattern::LeaveField {
        binding: event,
        predicate: movement_filter(
            own,
            entry.reference.is_some(),
            entry.sel.as_ref(),
            entry.during.is_some(),
            path,
            false,
        )?
        .predicate(match entry.to {
            ast::DepartureDestination::Cemetery => SubjectFields::Cemetery(event),
            ast::DepartureDestination::Hand => SubjectFields::Hand(event),
        }),
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
        || !matches!(entry.to, ast::DepartureDestination::Cemetery)
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
    allow_original_cost: bool,
) -> Result<SubjectFilter, LoweringError> {
    let original_cost = subject.and_then(|sel| sel.orig_cost_eq);
    if original_cost == Some(0) || (original_cost.is_some() && !allow_original_cost) {
        return Err(unsupported(path));
    }
    Ok(SubjectFilter {
        original_cost: original_cost
            .map(i64::try_from)
            .transpose()
            .map_err(|_overflow| unsupported(path))?,
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
            .map(|kind| match kind {
                ast::SearchKind::Follower => CardKind::Follower,
                ast::SearchKind::Spell => CardKind::Spell,
                ast::SearchKind::Amulet => CardKind::Amulet,
                ast::SearchKind::EvolveFollower => CardKind::EvolveFollower,
            }),
        trait_id: subject
            .and_then(|sel| sel.trait_id.as_ref())
            .map(|id| TraitId(id.clone())),
        name: subject
            .and_then(|sel| sel.name.as_ref())
            .map(|name| CardName(name.clone())),
        class: subject
            .and_then(|sel| sel.class.as_ref())
            .map(|class| ClassId(class.clone())),
        token: subject.and_then(|sel| sel.token.or_else(|| sel.not_token.map(|value| !value))),
        own_turn,
    })
}
