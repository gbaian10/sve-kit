use super::{Action, BodyContext, LoweringError, ast, unsupported};
use crate::ir::{Decision, KeywordId};

pub(super) fn cost(
    context: &mut BodyContext<'_>,
    amount: u64,
    path: &str,
) -> Result<(Action, Option<Decision>), LoweringError> {
    if amount == 0 {
        return Err(unsupported(path));
    }
    super::counters::cost(
        context,
        &ast::CounterCost {
            on: ast::CounterCostHolder::Selected(Box::new(amulets())),
            holders: Some(1),
            name: "stack_counter".into(),
            n: amount,
        },
        path,
    )
}

pub(super) const fn amulets() -> ast::PublicQuery {
    ast::PublicQuery {
        who: ast::Side::Controller,
        zone: ast::PublicZone::Field,
        kind: Some(ast::SearchKind::Amulet),
        keyword: Some(ast::FilterKeyword(KeywordId::Stack)),
        other: false,
        racing: None,
        r#trait: None,
        name: None,
        name_contains: None,
        cost_eq: None,
        cost_le: None,
        cost_ge: None,
        orig_cost_eq: None,
        orig_cost_le: None,
        orig_cost_ge: None,
        hp_le: None,
        class: None,
        not_class: None,
        token: None,
        r#where: None,
    }
}
