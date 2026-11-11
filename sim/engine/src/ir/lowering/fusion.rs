//! Fusion keeps its mixed private/public payment inside one native cost instruction.
use super::{AbilitySource, Allocator, BodyContext, EventIds, LoweringError, ast, unsupported};
use crate::ir::events::fusion;
use crate::ir::{
    Action, Binding, Cardinality, CostSpec, CostStep, Disjointness, EventPattern, Instruction,
    InstructionContext, InstructionId, Nat, ObjectOperand, PlaySpec, PlayerRef, PlayerRelation,
    PlayerSet, ReceiptId, SelectionConstraint, SelectionGroup, SelectionView, Selector,
    TextQuantifier, TraitId, ZoneKind,
};

pub(super) fn lower(
    context: &mut BodyContext<'_>,
    activation: &ast::Activate,
    source: &AbilitySource,
    path: &str,
) -> Result<PlaySpec, LoweringError> {
    let body = &activation.body;
    if activation.cost.is_some()
        || activation.usable_if.is_some()
        || activation.quick.is_some()
        || activation.advance.is_some()
        || body.optional_cost.is_some()
        || body.ub.is_some()
        || body.per_turn.is_some()
        || !body.targets.is_empty()
        || super::play::choice_body(body, path)?
    {
        return Err(unsupported(path));
    }
    let fusion = activation
        .fusion
        .as_ref()
        .ok_or_else(|| unsupported(path))?;
    let count = cardinality(fusion, path)?;
    if fusion.sel.zone.len() != 2
        || !fusion
            .sel
            .zone
            .iter()
            .any(|zone| matches!(zone, ast::ActivationZone::Hand))
        || !fusion
            .sel
            .zone
            .iter()
            .any(|zone| matches!(zone, ast::ActivationZone::Ex))
        || context.activation_zone != Some(ZoneKind::Hand)
    {
        return Err(unsupported(path));
    }
    let token = token_filter(&fusion.sel, path)?;
    let mut sets = Vec::new();
    for zone in [ZoneKind::Hand, ZoneKind::Ex] {
        let binding = Binding::new(context.card_allocator.id(path)?, context.scope);
        let filter = super::selection::ObjectFilter {
            kind: fusion.sel.kind.as_ref().map(super::search::card_kind),
            trait_id: fusion.sel.r#trait.as_deref(),
            token,
            original_cost: [None, None, fusion.sel.orig_cost_ge],
            ..super::selection::ObjectFilter::default()
        };
        sets.push(Selector::Zone {
            players: PlayerSet {
                relative_to: PlayerRef::Controller {},
                relation: PlayerRelation::SelfPlayer,
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
                text_quantifier: TextQuantifier::Implicit,
            },
            zone,
            view: SelectionView::Current {},
            binding,
            predicate: Box::new(super::selection::field_filter(&filter, binding)),
        });
    }
    let materials = SelectionConstraint {
        groups: vec![SelectionGroup {
            selector: Selector::Difference {
                input: Box::new(Selector::Union { sets }),
                except: Box::new(super::numeric::holder()),
            },
            cardinality: count,
        }],
        cardinality: count,
        disjoint: Disjointness::Disjoint,
        distinct_by: None,
        aggregate_bounds: vec![],
    };
    let receipt = ReceiptId(context.card_allocator.id(path)?);
    if let Some(name) = fusion.result.as_deref() {
        record_receipt(context, name, receipt, path)?;
    }
    Ok(PlaySpec {
        limit: None,
        captures: vec![],
        parameters: vec![],
        costs: CostSpec::Ordered {
            steps: vec![CostStep {
                keyword: None,
                materials: None,
                instruction: Box::new(Instruction {
                    id: InstructionId(receipt.0),
                    receipt,
                    source: source.clone(),
                    actor: PlayerRef::Controller {},
                    context: InstructionContext::Cost,
                    precondition: None,
                    action: Action::Fusion {
                        subject: ObjectOperand::SourceHolder {},
                        materials,
                    },
                }),
            }],
        },
    })
}

fn token_filter(query: &ast::FusionQuery, path: &str) -> Result<Option<bool>, LoweringError> {
    match (query.token, query.not_token) {
        (token @ (None | Some(true)), None) => Ok(token),
        (None, Some(true)) => Ok(Some(false)),
        _ => Err(unsupported(path)),
    }
}

fn record_receipt(
    context: &mut BodyContext<'_>,
    name: &str,
    receipt: ReceiptId,
    path: &str,
) -> Result<(), LoweringError> {
    if name.is_empty()
        || context.observations.contains_key(name)
        || context.definitions.contains_key(name)
    {
        return Err(unsupported(path));
    }
    context.observations.insert(
        name.to_owned(),
        super::super::ObjectReceipt::Fusion {
            receipt: super::super::ReceiptReference::new(receipt),
        },
    );
    Ok(())
}

fn cardinality(fusion: &ast::Fusion, path: &str) -> Result<Cardinality, LoweringError> {
    match (fusion.n, fusion.upto) {
        (Some(1), None) => Ok(Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        }),
        (None, Some(2)) => Ok(Cardinality {
            minimum: Nat(0),
            maximum: Some(Nat(2)),
        }),
        _ => Err(unsupported(path)),
    }
}

pub(super) fn subscription(
    on: &ast::PhaseSubscription,
    ids: EventIds,
    path: &str,
    allocator: &mut Allocator,
) -> Result<EventPattern, LoweringError> {
    let EventIds {
        event: event_id,
        scope,
        ..
    } = ids;
    if let ast::PhaseSubscription::Fused(subscription) = on {
        let binding = Binding::new(event_id, scope);
        if subscription.material.r#trait.is_empty() {
            return Err(unsupported(path));
        }
        Ok(EventPattern::Fused {
            binding,
            predicate: fusion::predicate(binding, &TraitId(subscription.material.r#trait.clone())),
        })
    } else if let ast::PhaseSubscription::AnyOf(subscriptions) = on {
        let patterns = subscriptions
            .iter()
            .map(|subscription| {
                let branch = EventIds {
                    event: allocator.id(path)?,
                    subject: allocator.id(path)?,
                    facet: allocator.id(path)?,
                    scope,
                };
                super::lowered_subscription(subscription, branch, path, allocator)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let pattern = EventPattern::AnyOf { patterns };
        if !fusion::combined(&pattern) {
            return Err(unsupported(path));
        }
        Ok(pattern)
    } else {
        Err(unsupported(path))
    }
}
