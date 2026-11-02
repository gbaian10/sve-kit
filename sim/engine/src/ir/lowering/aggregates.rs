use super::{Binding, BodyContext, Expr, Int, LoweringError, Selector, ast, unsupported};
use crate::ir::{ExtremumKind, IntOperation, NumericProperty};

const fn property(attribute: ast::AggregateAttribute) -> NumericProperty {
    match attribute {
        ast::AggregateAttribute::Cost => NumericProperty::Cost,
        ast::AggregateAttribute::OrigCost => NumericProperty::OriginalCost,
        ast::AggregateAttribute::Power => NumericProperty::Attack,
        ast::AggregateAttribute::Hp => NumericProperty::Life,
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Public aggregate queries share the body's allocator and source scope."
)]
impl BodyContext<'_> {
    pub(super) fn aggregate(
        &mut self,
        aggregate: &ast::Aggregate,
        path: &str,
    ) -> Result<Expr<Int>, LoweringError> {
        if !matches!(aggregate.function, ast::AggregateFunction::Sum) {
            return Err(unsupported(path));
        }
        let binding = Binding::new(self.allocator.id(path)?, self.card_allocator.scope(path)?);
        let mut objects = super::costs::public_query(&aggregate.of, binding);
        if aggregate.of.other {
            objects = Selector::Difference {
                input: Box::new(objects),
                except: Box::new(super::numeric::holder()),
            };
        }
        Ok(Expr::Compute(Box::new(IntOperation::Sum {
            objects,
            property: property(aggregate.attr),
        })))
    }

    pub(super) fn extremum_query(
        &self,
        query: &ast::ObjectQuery,
        input: Selector,
        path: &str,
    ) -> Result<Selector, LoweringError> {
        let population = ast::PublicQuery {
            who: query.who.clone(),
            zone: ast::PublicZone::Field,
            kind: query.kind.map(ast::FieldKind::search),
            keyword: None,
            r#trait: query.r#trait.clone(),
            class: query.class.clone(),
            not_class: query.not_class.clone(),
            token: query.token.or_else(|| query.not_token.map(|token| !token)),
            name: query.name.clone(),
            name_contains: query.name_contains.clone(),
            other: query.other,
            racing: None,
            orig_cost_eq: None,
            orig_cost_le: query.orig_cost_le,
            orig_cost_ge: None,
            cost_eq: None,
            cost_le: None,
            cost_ge: None,
            hp_le: None,
            r#where: None,
        };
        self.checked_extremum(
            query.alias.as_deref(),
            query.r#where.as_deref(),
            &population,
            input,
            path,
        )
    }

    pub(super) fn target_extremum(
        &self,
        query: &ast::TargetQuery,
        input: Selector,
        path: &str,
    ) -> Result<Selector, LoweringError> {
        if query.alias.is_none() && query.r#where.is_none() {
            return Ok(input);
        }
        if !matches!(
            query.zone,
            Some(ast::TargetZone::Single(ast::TargetSingleZone::Field))
        ) || query.observation.is_some()
            || query.leader.is_some()
            || query.leaders.is_some()
            || query.leader_or
            || query.not_same_name_as_this
            || query.has_evolve.is_some()
            || query.has_state_filter()
            || !query.traits_all.is_empty()
            || !query.names.is_empty()
        {
            return Err(unsupported(path));
        }
        let population = ast::PublicQuery {
            who: query.who.clone().ok_or_else(|| unsupported(path))?,
            zone: ast::PublicZone::Field,
            kind: query.kind.clone(),
            keyword: query.keyword,
            r#trait: query.r#trait.clone(),
            class: query.class.clone(),
            not_class: query.not_class.clone(),
            token: query.token,
            name: query.name.clone(),
            name_contains: query.name_contains.clone(),
            other: query.other,
            racing: None,
            orig_cost_eq: query.orig_cost_eq,
            orig_cost_le: query.orig_cost_le,
            orig_cost_ge: query.orig_cost_ge,
            cost_eq: query.cost_eq,
            cost_le: query.cost_le,
            cost_ge: query.cost_ge,
            hp_le: query.hp_le,
            r#where: None,
        };
        self.checked_extremum(
            query.alias.as_deref(),
            query.r#where.as_deref(),
            &population,
            input,
            path,
        )
    }

    fn checked_extremum(
        &self,
        alias: Option<&str>,
        condition: Option<&ast::QueryExtremumWhere>,
        population: &ast::PublicQuery,
        input: Selector,
        path: &str,
    ) -> Result<Selector, LoweringError> {
        let (alias, condition) = match (alias, condition) {
            (None, None) => return Ok(input),
            (alias, Some(condition)) => (alias.unwrap_or("item"), &condition.cmp),
            (Some(_), None) => return Err(unsupported(path)),
        };
        let (reference, selected_property) = match &condition.left {
            ast::QueryStat::Cost(name) => (name, NumericProperty::Cost),
            ast::QueryStat::OriginalCost(name) => (name, NumericProperty::OriginalCost),
            ast::QueryStat::Attack(name) => (name, NumericProperty::Attack),
            ast::QueryStat::Life(name) => (name, NumericProperty::Life),
        };
        let ast::EqualOnly::Eq = condition.op;
        let aggregate = match &condition.right {
            ast::QueryAggregate::Inline { agg } => agg.as_ref(),
            ast::QueryAggregate::Bound { bound } => {
                let mut name = bound.as_str();
                let mut visiting = super::BTreeSet::new();
                loop {
                    if !visiting.insert(name) {
                        return Err(unsupported(path));
                    }
                    match self.definitions.get(name) {
                        Some(ast::Number::Aggregate { agg }) => break agg,
                        Some(ast::Number::Alias(next)) => name = next,
                        _ => return Err(unsupported(path)),
                    }
                }
            }
        };
        if reference != alias || selected_property != property(aggregate.attr) {
            return Err(unsupported(path));
        }
        // A minimum from another population is a comparison, not a set extremum.
        if *population != aggregate.of {
            return Err(unsupported(path));
        }
        let kind = match aggregate.function {
            ast::AggregateFunction::Min => ExtremumKind::Minimum,
            ast::AggregateFunction::Max => ExtremumKind::Maximum,
            ast::AggregateFunction::Sum => return Err(unsupported(path)),
        };
        Ok(Selector::Extremum {
            input: Box::new(input),
            property: selected_property,
            kind,
        })
    }
}
