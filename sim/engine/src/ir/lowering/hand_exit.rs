//! The two mutually exclusive hand actions share one authored ability and source span.
use super::{LoweringError, ast, unsupported};
use crate::ir::events::banish as banish_header;
use crate::ir::events::discard::DiscardFilter;
use crate::ir::{Binding, EventPattern, ScopeId, ZoneKind};

#[derive(Clone, Copy)]
pub(super) enum Branch {
    Discard,
    Banish,
}

#[derive(Clone, Copy)]
pub(super) struct AbilityInput<'entry> {
    pub(super) entry: &'entry ast::Ability,
    pub(super) branch: Option<Branch>,
}

impl<'entry> AbilityInput<'entry> {
    pub(super) const fn ordinary(entry: &'entry ast::Ability) -> Self {
        Self {
            entry,
            branch: None,
        }
    }
}

pub(super) fn is_union(entry: &ast::Ability) -> bool {
    entry
        .auto
        .as_ref()
        .is_some_and(|auto| is_hand_union(&auto.on))
}

pub(super) fn is_hand_union(on: &ast::PhaseSubscription) -> bool {
    matches!(on, ast::PhaseSubscription::AnyOf(events) if events.iter().any(|event| {
        matches!(event, ast::PhaseSubscription::Discarded(_) | ast::PhaseSubscription::Banished(_))
    }))
}

pub(super) fn check(entry: &ast::Ability, path: &str) -> Result<bool, LoweringError> {
    if !is_union(entry) {
        return Ok(false);
    }
    let Some(auto) = &entry.auto else {
        return Ok(false);
    };
    let ast::PhaseSubscription::AnyOf(events) = &auto.on else {
        return Ok(false);
    };
    if events.len() != 2
        || auto.aggregation.is_some()
        || auto.body.per_turn.is_some()
        || super::active_zones(entry, path)? != [ZoneKind::Hand]
    {
        return Err(unsupported(path));
    }
    let discarded = events
        .iter()
        .filter(|on| {
            matches!(on,
                ast::PhaseSubscription::Discarded(event)
                if event.reference.is_some() && event.from.is_some()
                    && !matches!(event.who, Some(ast::EntryPlayer::Opponent))
                    && event.during.is_none() && event.by.is_none() && event.sel.is_none()
            )
        })
        .count();
    let banished = events
        .iter()
        .filter(|on| {
            matches!(on, ast::PhaseSubscription::Banished(ast::BanishedSubscription::Hand(event))
        if matches!(event.reference, ast::This::This))
        })
        .count();
    if discarded != 1 || banished != 1 {
        return Err(unsupported(path));
    }
    Ok(true)
}

pub(super) fn discard(event: u64, scope: ScopeId) -> EventPattern {
    let binding = Binding::new(event, scope);
    EventPattern::Discard {
        binding,
        predicate: DiscardFilter {
            own: Some(true),
            itself: true,
            own_turn: false,
            by: None,
            card: None,
        }
        .predicate(binding),
    }
}

pub(super) fn banish(event: u64, scope: ScopeId) -> EventPattern {
    let binding = Binding::new(event, scope);
    EventPattern::Banish {
        binding,
        predicate: banish_header::predicate(binding),
    }
}
