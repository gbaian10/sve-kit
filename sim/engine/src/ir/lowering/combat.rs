//! Event headers lower through shared subject filters and existing characteristic reads.
use super::{Binding, EventPattern, LoweringError, ScopeId, ast, movement, unsupported};
use crate::ir::events::attack::AttackFilter;
use crate::ir::events::damage::{DamageFilter, DamageKind, RecipientFilter};
use crate::ir::events::life::LifeFilter;
use crate::ir::{Int, KeywordId};

fn keyword(name: &str, path: &str) -> Result<KeywordId, LoweringError> {
    super::keywords::keyword_id(name).ok_or_else(|| unsupported(path))
}
fn qualifier(
    condition: &ast::AttackCondition,
    alias: &str,
    plan: &mut AttackFilter,
    path: &str,
) -> Result<(), LoweringError> {
    match condition {
        ast::AttackCondition::Cmp(comparison)
            if comparison.left.power_of == alias
                && matches!(comparison.op, ast::Comparison::Ge)
                && plan.power_at_least.is_none() =>
        {
            plan.power_at_least = Some(Int(
                i64::try_from(comparison.right).map_err(|_error| unsupported(path))?
            ));
        }
        ast::AttackCondition::Exists(query)
            if query.r#in == alias && plan.keywords_any.is_empty() =>
        {
            plan.keywords_any.push(keyword(&query.keyword, path)?);
        }
        ast::AttackCondition::Any(conditions)
            if !conditions.is_empty() && plan.keywords_any.is_empty() =>
        {
            for alternative in conditions {
                let ast::AttackCondition::Exists(query) = alternative else {
                    return Err(unsupported(path));
                };
                if query.r#in != alias {
                    return Err(unsupported(path));
                }
                plan.keywords_any.push(keyword(&query.keyword, path)?);
            }
        }
        ast::AttackCondition::All(conditions) if !conditions.is_empty() => {
            for conjunct in conditions {
                qualifier(conjunct, alias, plan, path)?;
            }
        }
        ast::AttackCondition::Cmp(_)
        | ast::AttackCondition::Any(_)
        | ast::AttackCondition::All(_)
        | ast::AttackCondition::Exists(_) => return Err(unsupported(path)),
    }
    Ok(())
}

pub(super) fn attack(
    entry: &ast::AttackSubscription,
    event_id: u64,
    scope: ScopeId,
    path: &str,
) -> Result<EventPattern, LoweringError> {
    if entry.reference.is_some() && entry.sel.is_some() {
        return Err(unsupported(path));
    }
    let subject_filter = entry.sel.as_ref().map(|subject| ast::MovementSubject {
        class: subject.class.clone(),
        kind: subject.kind.clone(),
        trait_id: subject.trait_id.clone(),
        name: subject.name.clone(),
        token: subject.token,
        not_token: subject.not_token,
        other: subject.other,
    });
    let mut plan = AttackFilter {
        subject: movement::movement_filter(
            entry
                .who
                .as_ref()
                .map(|who| matches!(who, ast::EntryPlayer::Controller)),
            entry.reference.is_some(),
            subject_filter.as_ref(),
            false,
        ),
        own_turn: entry
            .during
            .as_ref()
            .map(|during| matches!(during.turn, ast::EntryPlayer::Controller)),
        ..AttackFilter::default()
    };
    if let Some(subject) = &entry.sel {
        if let Some(name) = &subject.keyword {
            plan.keywords_any.push(keyword(name, path)?);
        }
        match (&subject.alias, &subject.r#where) {
            (None, None) => {}
            (Some(alias), Some(condition)) if !alias.is_empty() => {
                qualifier(condition, alias, &mut plan, path)?;
            }
            _ => return Err(unsupported(path)),
        }
    }
    let event = Binding::new(event_id, scope);
    Ok(EventPattern::Attack {
        binding: event,
        predicate: plan.predicate(event),
    })
}

const fn recipient(value: &ast::DamageRecipientSubscription) -> RecipientFilter {
    match value {
        ast::DamageRecipientSubscription::Source(_) => RecipientFilter::SourceHolder,
        ast::DamageRecipientSubscription::Leader { leader } => {
            RecipientFilter::Leader(Some(matches!(leader, ast::EntryPlayer::Controller)))
        }
    }
}
const fn kind(value: &ast::AbilityDamage) -> DamageKind {
    match value {
        ast::AbilityDamage::Ability => DamageKind::Ability,
        ast::AbilityDamage::Combat => DamageKind::Combat,
        ast::AbilityDamage::Attack => DamageKind::Attack,
    }
}
pub(super) fn damaged(
    entry: &ast::DamageSubscription,
    event_id: u64,
    scope: ScopeId,
) -> EventPattern {
    let event = Binding::new(event_id, scope);
    EventPattern::Damage {
        binding: event,
        predicate: DamageFilter {
            dealt: None,
            recipient: entry.recipient.as_ref().map(recipient),
            kind: entry.kind.as_ref().map(kind),
            own_turn: entry
                .during
                .as_ref()
                .map(|during| matches!(during.turn, ast::EntryPlayer::Controller)),
        }
        .predicate(event),
    }
}
pub(super) fn dealt(
    entry: &ast::DealtSubscription,
    event_id: u64,
    scope: ScopeId,
    path: &str,
) -> Result<EventPattern, LoweringError> {
    if entry.reference.is_some() && entry.sel.is_some() {
        return Err(unsupported(path));
    }
    let event = Binding::new(event_id, scope);
    Ok(EventPattern::Damage {
        binding: event,
        predicate: DamageFilter {
            dealt: Some(movement::movement_filter(
                entry
                    .who
                    .as_ref()
                    .map(|who| matches!(who, ast::EntryPlayer::Controller)),
                entry.reference.is_some(),
                entry.sel.as_ref(),
                false,
            )),
            recipient: entry.recipient.as_ref().map(recipient),
            kind: entry.kind.as_ref().map(kind),
            own_turn: entry
                .during
                .as_ref()
                .map(|during| matches!(during.turn, ast::EntryPlayer::Controller)),
        }
        .predicate(event),
    })
}
pub(super) fn life(entry: &ast::LifeSubscription, event_id: u64, scope: ScopeId) -> EventPattern {
    let event = Binding::new(event_id, scope);
    EventPattern::LifeChange {
        binding: event,
        predicate: LifeFilter {
            own: entry
                .who
                .as_ref()
                .map(|who| matches!(who, ast::EntryPlayer::Controller)),
            up: entry
                .direction
                .as_ref()
                .map(|direction| matches!(direction, ast::LifeDirection::Up)),
            own_turn: entry
                .during
                .as_ref()
                .map(|during| matches!(during.turn, ast::EntryPlayer::Controller)),
        }
        .predicate(event),
    }
}
