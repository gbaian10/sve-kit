//! The card-play dependency links a quote to a real root spell parameter, never any matching name.
use super::{Allocator, BodyContext, Expr, LoweringError, ast, unsupported};
use crate::ir::{
    Binding, Bool, BoolRead, CardName, ChoiceDomain, Decision, ExecutionRef, PlayTargetMatch,
    Selector, SourceSpan,
};

#[derive(Clone)]
pub(super) struct ResolvedTarget {
    source: SourceSpan,
    name: String,
    decision: Decision,
    selection: Binding<Selector>,
}

pub(super) fn record(context: &mut BodyContext<'_>, body: &ast::Body, source: &SourceSpan) {
    for target in &body.targets {
        if let Some(selection) = context.targets.get(&target.bind)
            && let Some(decision) = context.parameters.iter().find(|parameter| {
                matches!(&parameter.domain, ChoiceDomain::Objects { binding, .. } if binding == selection)
            })
        {
            context.card_allocator.formal_spell_targets.push(ResolvedTarget {
                source: source.clone(), name: target.bind.clone(), decision: decision.clone(), selection: *selection,
            });
        }
    }
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Other cost conditions have no cross-head dependency."
)]
pub(super) fn uses_chosen(condition: &ast::CostCondition) -> bool {
    match condition {
        ast::CostCondition::Chosen(_) => true,
        ast::CostCondition::All(parts) | ast::CostCondition::Any(parts) => {
            parts.iter().any(uses_chosen)
        }
        ast::CostCondition::Not(part) => uses_chosen(part),
        _ => false,
    }
}

pub(super) fn condition(
    chosen: &ast::CostChosen,
    source: &SourceSpan,
    allocator: &Allocator,
    path: &str,
) -> Result<Expr<Bool>, LoweringError> {
    let mut targets = allocator.formal_spell_targets.iter().filter(|target| {
        target.source.card_id == source.card_id
            && target.source.card_no == source.card_no
            && target.source.face == source.face
            && target.name == chosen.reference
    });
    let target = targets.next().ok_or_else(|| unsupported(path))?;
    if targets.next().is_some() {
        return Err(unsupported(path));
    }
    let predicate = match chosen.r#where.as_ref() {
        None => PlayTargetMatch::Present {},
        Some(filter) => match (&filter.name, &filter.name_contains) {
            (Some(name), None) if !name.is_empty() => PlayTargetMatch::NameEquals {
                name: CardName(name.clone()),
            },
            (None, Some(fragment)) if !fragment.is_empty() => PlayTargetMatch::NameContains {
                fragment: CardName(fragment.clone()),
            },
            _ => return Err(unsupported(path)),
        },
    };
    Ok(Expr::Live(Box::new(BoolRead::CardPlayTarget {
        execution: ExecutionRef::Current {},
        decision: target.decision.id,
        selection: target.selection,
        predicate,
    })))
}
