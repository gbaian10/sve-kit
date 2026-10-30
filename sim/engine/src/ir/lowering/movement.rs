//! Printed movement headers share the immutable event filter plan.
use super::{Binding, EventPattern, LoweringError, ScopeId, ast, unsupported};
use crate::ir::events::subject::{SubjectFields, SubjectFilter};
use crate::ir::{CardKind, CardName, TraitId};

pub(super) fn subscription(
    on: &ast::PhaseSubscription,
    event_id: u64,
    scope: ScopeId,
    path: &str,
) -> Result<EventPattern, LoweringError> {
    match on {
        ast::PhaseSubscription::Moved(entry) => {
            let own = match &entry.from {
                ast::MoveOrigin::Zone(_) => None,
                ast::MoveOrigin::Owned { who, .. } => {
                    Some(matches!(who, ast::EntryPlayer::Controller))
                }
            };
            let event = Binding::new(event_id, scope);
            Ok(EventPattern::LeaveField {
                binding: event,
                predicate: movement_filter(
                    own,
                    entry.reference.is_some(),
                    entry.sel.as_ref(),
                    entry.during.is_some(),
                )
                .predicate(SubjectFields::Leave(event)),
            })
        }
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
                )
                .predicate(SubjectFields::Ex(event)),
            })
        }
        ast::PhaseSubscription::Evolved(_)
        | ast::PhaseSubscription::Played(_)
        | ast::PhaseSubscription::Phase(_)
        | ast::PhaseSubscription::EnterField(_)
        | ast::PhaseSubscription::LeaveField(_)
        | ast::PhaseSubscription::Damage(_) => Err(unsupported(path)),
    }
}

fn movement_filter(
    own: Option<bool>,
    itself: bool,
    subject: Option<&ast::MovementSubject>,
    own_turn: bool,
) -> SubjectFilter {
    SubjectFilter {
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
            }),
        trait_id: subject
            .and_then(|sel| sel.trait_id.as_ref())
            .map(|id| TraitId(id.clone())),
        name: subject
            .and_then(|sel| sel.name.as_ref())
            .map(|name| CardName(name.clone())),
        token: subject.and_then(|sel| sel.token.or_else(|| sel.not_token.map(|value| !value))),
        own_turn,
    }
}
