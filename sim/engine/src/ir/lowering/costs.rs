use super::{
    Allocator, Binding, Bool, Cardinality, Expr, Int, Located, LoweringError, Nat, PlayerRef,
    PlayerRelation, PlayerSet, Predicate, ScopeId, SelectionView, Selector, TextQuantifier,
    ZoneKind, ast, decode, source_span, unsupported,
};
use crate::ir::{
    BoolRead, Comparison, CostChange, HistoryCount, IntOperation, IntRead, NatRead, ObjectOperand,
    ObjectRef, PlayCostModifier, PlayerResource,
};

pub(super) fn lower(
    key: &super::CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    play: &ast::PlayCosts,
    allocator: &mut Allocator,
    modifiers: &mut Vec<Located<PlayCostModifier>>,
) -> Result<(), LoweringError> {
    if entry.imports.is_some() {
        return Err(unsupported(path));
    }
    let source = source_span(key, path, entry)?;
    for (field, input, set) in [
        ("cost_delta", &play.cost_delta, false),
        ("cost_set", &play.cost_set, true),
    ] {
        let Some(input) = input else {
            continue;
        };
        let values = input
            .as_array()
            .map_or_else(|| vec![input], |items| items.iter().collect());
        for (index, value) in values.into_iter().enumerate() {
            let path = if input.is_array() {
                format!("{path}/play/{field}/{index}")
            } else {
                format!("{path}/play/{field}")
            };
            let (amount, condition) = if value.get("if").is_some() {
                let change: ast::ConditionalChange = decode(value.clone(), &path)?;
                let ((false, Some(amount), None) | (true, None, Some(amount))) =
                    (set, change.delta, change.set)
                else {
                    return Err(unsupported(&path));
                };
                (
                    amount,
                    Some(condition(&change.r#if, entry, allocator, &path)?),
                )
            } else {
                (decode::<ast::Number>(value.clone(), &path)?, None)
            };
            if set && !matches!(amount, ast::Number::Literal(amount) if amount >= 0) {
                return Err(unsupported(&path));
            }
            let body = ast::Body::default();
            let mut context = super::BodyContext::new(entry, ScopeId(0), &body, None, allocator);
            context.definitions = &play.define;
            context.allocator.next = context.card_allocator.next;
            let amount = context.number(&amount, &path, &mut super::BTreeSet::new())?;
            context.card_allocator.next = context.card_allocator.next.max(context.allocator.next);
            if !context.resolution_captures.is_empty() {
                return Err(unsupported(&path));
            }
            let mut source = source.clone();
            source.author_path = path;
            modifiers.push(Located {
                source,
                node: PlayCostModifier {
                    condition,
                    change: if set {
                        CostChange::Set { amount }
                    } else {
                        CostChange::Delta { amount }
                    },
                },
            });
        }
    }
    Ok(())
}

fn population(
    spells: bool,
    allocator: &mut Allocator,
    path: &str,
) -> Result<Expr<Int>, LoweringError> {
    // Discounts read the payment state; resolution captures would freeze the wrong moment.
    let binding = Binding::new(allocator.id(path)?, ScopeId(0));
    let kind = spells.then_some(ast::SearchKind::Spell);
    let objects = Selector::Zone {
        players: PlayerSet {
            relation: PlayerRelation::SelfPlayer,
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
            text_quantifier: TextQuantifier::Implicit,
        },
        zone: ZoneKind::Cemetery,
        view: SelectionView::Current {},
        binding,
        predicate: Box::new(super::selection::field_filter(
            &super::selection::ObjectFilter {
                kind: kind.as_ref().map(super::search::card_kind),
                ..Default::default()
            },
            binding,
        )),
    };
    Ok(super::numeric::aggregate_count(objects, None))
}

fn condition(
    value: &ast::CostCondition,
    entry: &ast::Ability,
    allocator: &mut Allocator,
    path: &str,
) -> Result<Expr<Bool>, LoweringError> {
    Ok(Expr::Compute(Box::new(match value {
        ast::CostCondition::Spellchain(threshold) | ast::CostCondition::Necro(threshold) => {
            Predicate::CompareInt {
                left: population(
                    matches!(value, ast::CostCondition::Spellchain(_)),
                    allocator,
                    path,
                )?,
                comparison: Comparison::GreaterEqual,
                right: Expr::Literal(Int(
                    i64::try_from(*threshold).map_err(|_private| unsupported(path))?
                )),
            }
        }
        ast::CostCondition::Awakened(true) => Predicate::CompareInt {
            left: Expr::Live(Box::new(IntRead::PlayerResource {
                player: PlayerRef::Controller {},
                property: PlayerResource::PpMaximum,
            })),
            comparison: Comparison::GreaterEqual,
            right: Expr::Literal(Int(7)),
        },
        ast::CostCondition::Crimson(true) => {
            return Ok(Expr::Live(Box::new(BoolRead::Crimson {
                player: PlayerRef::Controller {},
            })));
        }
        ast::CostCondition::LeaderLife(threshold) => {
            super::conditions::leader_life_condition(threshold, path)?
        }
        ast::CostCondition::Awakened(false) | ast::CostCondition::Crimson(false) => {
            return Err(unsupported(path));
        }
        ast::CostCondition::InZone(location) => {
            if location.r#ref != "this" {
                return Err(unsupported(path));
            }
            return Ok(Expr::Live(Box::new(BoolRead::CardPlayOrigin {
                execution: super::super::ExecutionRef::Current {},
                zone: match location.zone.as_str() {
                    "hand" => ZoneKind::Hand,
                    "ex" => ZoneKind::Ex,
                    _ => return Err(unsupported(path)),
                },
            })));
        }
        ast::CostCondition::All(items) | ast::CostCondition::Any(items) => {
            let predicates = items
                .iter()
                .map(|item| condition(item, entry, allocator, path))
                .collect::<Result<Vec<_>, _>>()?;
            if matches!(value, ast::CostCondition::All(_)) {
                Predicate::And { predicates }
            } else {
                Predicate::Or { predicates }
            }
        }
        ast::CostCondition::Not(value) => Predicate::Not {
            predicate: condition(value, entry, allocator, path)?,
        },
        ast::CostCondition::Exists(value) => Predicate::Exists {
            objects: query(value, allocator, path)?,
        },
        ast::CostCondition::History(history) => history_condition(history, entry, allocator, path)?,
        ast::CostCondition::Cmp(value) => Predicate::CompareInt {
            left: condition_number(&value.left, entry, allocator, path)?,
            comparison: match value.op {
                ast::Comparison::Eq => Comparison::Equal,
                ast::Comparison::Ne => Comparison::NotEqual,
                ast::Comparison::Lt => Comparison::Less,
                ast::Comparison::Le => Comparison::LessEqual,
                ast::Comparison::Gt => Comparison::Greater,
                ast::Comparison::Ge => Comparison::GreaterEqual,
            },
            right: Expr::Literal(Int(value.right)),
        },
    })))
}

fn condition_number(
    value: &ast::PublicNumber,
    entry: &ast::Ability,
    allocator: &mut Allocator,
    path: &str,
) -> Result<Expr<Int>, LoweringError> {
    Ok(match value {
        ast::PublicNumber::Literal(amount) => Expr::Literal(Int(*amount)),
        ast::PublicNumber::Count { count } => cost_count(count, entry, allocator, path)?,
        ast::PublicNumber::DistinctNames { distinct_names } => super::numeric::distinct_count(
            distinct_names,
            super::super::ObjectProperty::Names,
            Binding::new(allocator.id(path)?, ScopeId(0)),
            path,
        )?,
        ast::PublicNumber::DistinctOriginalCosts {
            distinct_orig_costs,
        } => super::numeric::distinct_count(
            distinct_orig_costs,
            super::super::ObjectProperty::Cost,
            Binding::new(allocator.id(path)?, ScopeId(0)),
            path,
        )?,
    })
}

fn history_condition(
    history: &ast::HistoryQuery,
    entry: &ast::Ability,
    allocator: &mut Allocator,
    path: &str,
) -> Result<Predicate, LoweringError> {
    let body = ast::Body::default();
    let mut context = super::BodyContext::new(entry, ScopeId(0), &body, None, allocator);
    context.allocator.next = context.card_allocator.next;
    let query = context.history_query(history, path)?;
    context.card_allocator.next = context.card_allocator.next.max(context.allocator.next);
    if matches!(history.distinct, Some(ast::HistoryDistinct::Subjects))
        || !crate::ir::history_turn::HistoryPlan::compile(&query)
            .is_some_and(|plan| plan.cost_facts())
    {
        return Err(unsupported(path));
    }
    Ok(Predicate::CompareNat {
        left: Expr::Live(Box::new(NatRead::HistoryCount {
            query,
            count: HistoryCount::Facts,
        })),
        comparison: Comparison::GreaterEqual,
        right: Expr::Literal(Nat(history.at_least.unwrap_or(1))),
    })
}

fn condition_count_query(
    value: &ast::PublicQuery,
    allocator: &mut Allocator,
    path: &str,
) -> Result<Selector, LoweringError> {
    if value.class.is_some()
        && matches!(value.zone, ast::PublicZone::Cemetery)
        && matches!(value.who, ast::Side::Controller)
        && !value.other
        && value.not_class.is_none()
        && value.token.is_none()
    {
        return public_query(value, Binding::new(allocator.id(path)?, ScopeId(0)), path);
    }
    query(value, allocator, path)
}

pub(super) fn query(
    value: &ast::PublicQuery,
    allocator: &mut Allocator,
    path: &str,
) -> Result<Selector, LoweringError> {
    if value.other
        || value.class.is_some()
        || value.not_class.is_some()
        || value.token.is_some()
        || matches!(value.zone, ast::PublicZone::FieldEx)
    {
        return Err(unsupported(path));
    }
    public_query(value, Binding::new(allocator.id(path)?, ScopeId(0)), path)
}

pub(super) fn public_query(
    value: &ast::PublicQuery,
    binding: Binding<ObjectRef>,
    path: &str,
) -> Result<Selector, LoweringError> {
    // Counts and numeric reads have no card that needs the holder-name exclusion yet.
    if value.not_same_name_as_this {
        return Err(unsupported(path));
    }
    named_public_query(value, binding, path)
}

/// A public query that may exclude the ability holder's names (exists conditions).
pub(super) fn named_public_query(
    value: &ast::PublicQuery,
    binding: Binding<ObjectRef>,
    path: &str,
) -> Result<Selector, LoweringError> {
    // A trait union is admitted only for cost materials, which build their own selector.
    if !value.names.is_empty()
        || value.acted.is_some()
        || value.traits_any.is_some()
        || (value.evolved.is_some() && !matches!(value.zone, ast::PublicZone::Field))
    {
        return Err(unsupported(path));
    }
    let objects = match value.zone {
        ast::PublicZone::Field => public_zone_query(value, binding, ZoneKind::Field, path)?,
        ast::PublicZone::Ex => public_zone_query(value, binding, ZoneKind::Ex, path)?,
        ast::PublicZone::Cemetery => public_zone_query(value, binding, ZoneKind::Cemetery, path)?,
        ast::PublicZone::FieldEx => return Err(unsupported(path)),
    };
    Ok(if value.other {
        Selector::Difference {
            input: Box::new(objects),
            except: Box::new(Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            }),
        }
    } else {
        objects
    })
}

fn public_zone_query(
    value: &ast::PublicQuery,
    binding: Binding<ObjectRef>,
    zone: ZoneKind,
    path: &str,
) -> Result<Selector, LoweringError> {
    Ok(Selector::Zone {
        players: PlayerSet {
            relation: value.who.relation().ok_or_else(|| unsupported(path))?,
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(0),
                maximum: None,
            },
            text_quantifier: TextQuantifier::Implicit,
        },
        zone,
        view: SelectionView::Current {},
        binding,
        predicate: Box::new(super::selection::field_filter(
            &super::selection::ObjectFilter::from(value),
            binding,
        )),
    })
}

fn cost_count(
    count: &ast::PublicCountQuery,
    entry: &ast::Ability,
    allocator: &mut Allocator,
    path: &str,
) -> Result<Expr<Int>, LoweringError> {
    match count {
        ast::PublicCountQuery::Public(query) => Ok(super::numeric::aggregate_count(
            condition_count_query(query, allocator, path)?,
            None,
        )),
        ast::PublicCountQuery::Numeric(query) => {
            let body = ast::Body::default();
            let mut context = super::BodyContext::new(entry, ScopeId(0), &body, None, allocator);
            if matches!(query.zone, ast::CountZone::Hand)
                && query.other
                && matches!(query.who, ast::Side::Controller)
            {
                let mut unfiltered = query.clone();
                unfiltered.other = false;
                context.zone_count(&unfiltered, path)?;
                return Ok(Expr::Compute(Box::new(IntOperation::FromNat {
                    value: Expr::Live(Box::new(NatRead::OtherHandCount {
                        player: PlayerRef::Controller {},
                        excluding: ObjectOperand::SourceHolder {},
                    })),
                })));
            }
            // Private zones stay closed to cost conditions apart from the hand count above.
            if matches!(query.zone, ast::CountZone::Hand | ast::CountZone::Deck) {
                return Err(unsupported(path));
            }
            context.zone_count(query, path)
        }
    }
}
