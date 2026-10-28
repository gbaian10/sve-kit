use super::{
    Allocator, Binding, Bool, Cardinality, Expr, Int, IntOperation, Located, LoweringError, Nat,
    NatRead, PlayerRef, PlayerRelation, PlayerSet, Predicate, ScopeId, SelectionView, Selector,
    TextQuantifier, ZoneKind, ast, decode, source_span, unsupported,
};
use crate::ir::{Comparison, CostChange, ObjectRef, PlayCostModifier};

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

fn condition(
    value: &ast::CostCondition,
    allocator: &mut Allocator,
    path: &str,
) -> Result<Expr<Bool>, LoweringError> {
    Ok(Expr::Compute(Box::new(match value {
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
                    Expr::Compute(Box::new(IntOperation::FromNat {
                        value: Expr::Live(Box::new(NatRead::ZoneCount {
                            selector: query(count, allocator, path)?,
                        })),
                    }))
                }
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
    if value.other
        || value.class.is_some()
        || value.token.is_some()
        || matches!(value.zone, ast::PublicZone::Ex)
    {
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
        predicate: Box::new(super::search::object_filter(
            value.kind.as_ref(),
            value.r#trait.as_deref(),
            value.name.as_deref(),
            binding,
        )),
    }
}
