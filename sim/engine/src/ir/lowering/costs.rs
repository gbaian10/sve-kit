use super::{
    Allocator, Binding, Bool, Cardinality, Expr, Int, Located, LoweringError, Nat, PlayerRef,
    PlayerRelation, PlayerSet, Predicate, ScopeId, SelectionView, Selector, TextQuantifier,
    ZoneKind, ast, decode, source_span, unsupported,
};
use crate::ir::{
    BoolRead, Comparison, CostChange, IntRead, ObjectRef, PlayCostModifier, PlayerResource,
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
            let (amount, condition) = if let Some(amount) = value.as_i64() {
                (amount, None)
            } else {
                let change: ast::ConditionalChange = decode(value.clone(), &path)?;
                let ((false, Some(amount), None) | (true, None, Some(amount))) =
                    (set, change.delta, change.set)
                else {
                    return Err(unsupported(&path));
                };
                (amount, Some(condition(&change.r#if, allocator, &path)?))
            };
            if (set && amount < 0) || (!set && amount > 0) {
                return Err(unsupported(&path));
            }
            let mut source = source.clone();
            source.author_path = path;
            let amount = Expr::Literal(Int(amount));
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
                kind: kind.as_ref(),
                ..Default::default()
            },
            binding,
        )),
    };
    Ok(super::numeric::aggregate_count(objects, None))
}

fn condition(
    value: &ast::CostCondition,
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
        ast::CostCondition::All(items) | ast::CostCondition::Any(items) => {
            let predicates = items
                .iter()
                .map(|item| condition(item, allocator, path))
                .collect::<Result<Vec<_>, _>>()?;
            if matches!(value, ast::CostCondition::All(_)) {
                Predicate::And { predicates }
            } else {
                Predicate::Or { predicates }
            }
        }
        ast::CostCondition::Not(value) => Predicate::Not {
            predicate: condition(value, allocator, path)?,
        },
        ast::CostCondition::Exists(value) => Predicate::Exists {
            objects: query(value, allocator, path)?,
        },
        ast::CostCondition::Cmp(value) => Predicate::CompareInt {
            left: match &value.left {
                ast::PublicNumber::Literal(amount) => Expr::Literal(Int(*amount)),
                ast::PublicNumber::Count { count } => {
                    super::numeric::aggregate_count(query(count, allocator, path)?, None)
                }
                ast::PublicNumber::DistinctNames { distinct_names } => {
                    super::numeric::distinct_count(
                        distinct_names,
                        super::super::ObjectProperty::Names,
                        Binding::new(allocator.id(path)?, ScopeId(0)),
                        path,
                    )?
                }
                ast::PublicNumber::DistinctOriginalCosts {
                    distinct_orig_costs,
                } => super::numeric::distinct_count(
                    distinct_orig_costs,
                    super::super::ObjectProperty::Cost,
                    Binding::new(allocator.id(path)?, ScopeId(0)),
                    path,
                )?,
            },
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

pub(super) fn query(
    value: &ast::PublicQuery,
    allocator: &mut Allocator,
    path: &str,
) -> Result<Selector, LoweringError> {
    if value.other || value.class.is_some() || value.not_class.is_some() || value.token.is_some() {
        return Err(unsupported(path));
    }
    Ok(public_query(
        value,
        Binding::new(allocator.id(path)?, ScopeId(0)),
    ))
}

pub(super) fn public_query(value: &ast::PublicQuery, binding: Binding<ObjectRef>) -> Selector {
    Selector::Zone {
        players: PlayerSet {
            relation: match value.who {
                ast::Side::Controller => PlayerRelation::SelfPlayer,
                ast::Side::Opponent => PlayerRelation::Opponents,
                ast::Side::All => PlayerRelation::All,
            },
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(0),
                maximum: None,
            },
            text_quantifier: TextQuantifier::Implicit,
        },
        zone: match value.zone {
            ast::PublicZone::Field => ZoneKind::Field,
            ast::PublicZone::Ex => ZoneKind::Ex,
            ast::PublicZone::Cemetery => ZoneKind::Cemetery,
        },
        view: SelectionView::Current {},
        binding,
        predicate: Box::new(super::selection::field_filter(
            &super::selection::ObjectFilter::from(value),
            binding,
        )),
    }
}
