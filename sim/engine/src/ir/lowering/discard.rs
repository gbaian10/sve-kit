use super::super::{
    DiscardMode, Disjointness, SelectionConstraint, SelectionGroup, SelectionView, ZoneKind,
};
use super::{
    Action, Binding, BodyContext, Bool, Cardinality, Expr, Int, LoweringError, Nat, PlayerRef,
    PlayerRelation, PlayerSet, Selector, TextQuantifier, ast, unsupported,
};
use crate::ir::events::discard::DiscardFilter;
use crate::ir::events::discard::{DiscardCard, DiscardSource};
use crate::ir::{BatchPolicy, ClassId, TraitId, TriggerAggregation};
use alloc::collections::BTreeSet;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Discard shares the body's binding allocator."
)]
impl BodyContext<'_> {
    fn whole_hand(&mut self, player: &PlayerRef, path: &str) -> Result<Selector, LoweringError> {
        Ok(Selector::Zone {
            players: PlayerSet {
                relation: PlayerRelation::SelfPlayer,
                relative_to: player.clone(),
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
                text_quantifier: TextQuantifier::Implicit,
            },
            zone: ZoneKind::Hand,
            view: SelectionView::Current {},
            binding: Binding::new(self.allocator.id(path)?, self.scope),
            predicate: Box::new(Expr::Literal(Bool(true))),
        })
    }

    pub(super) fn discard(
        &mut self,
        value: &ast::Discard,
        path: &str,
    ) -> Result<Action, LoweringError> {
        if value.any.is_some()
            && (value.n.is_some()
                || value.all.is_some()
                || value.random.is_some()
                || matches!(value.who, Some(ast::LeaderSide::Opponent)))
        {
            return Err(unsupported(path));
        }
        if value.reference.is_some()
            && (value.n.is_some()
                || value.all.is_some()
                || value.random.is_some()
                || value.any.is_some())
        {
            return Err(unsupported(path));
        }
        let player = match value.who {
            Some(ast::LeaderSide::Opponent) => super::super::events::timing::opponent(),
            Some(ast::LeaderSide::Controller) | None => PlayerRef::Controller {},
        };
        if let Some(ast::DiscardCount::Number(number)) = &value.n {
            // `all`, `any` and `ref` are separate discard shapes, never a count.
            if value.all.is_some() || value.any.is_some() || value.reference.is_some() {
                return Err(unsupported(path));
            }
            let population = self.whole_hand(&player, path)?;
            let count = self.number(number, path, &mut BTreeSet::new())?;
            let mode = if value.random == Some(true) {
                DiscardMode::Random { population, count }
            } else {
                DiscardMode::ChosenCount { population, count }
            };
            return Ok(Action::Discard { player, mode });
        }
        let mode = match (value.fixed_n(), value.all) {
            (n, None)
                if (n.is_some_and(|n| n > 0) || value.any.is_some())
                    && value.random != Some(true) =>
            {
                let cardinality = Cardinality {
                    minimum: Nat(n.unwrap_or(0)),
                    maximum: n.map(Nat),
                };
                DiscardMode::Chosen {
                    constraint: SelectionConstraint {
                        groups: vec![SelectionGroup {
                            selector: self.whole_hand(&player, path)?,
                            cardinality,
                        }],
                        cardinality,
                        distinct_by: None,
                        disjoint: Disjointness::Disjoint,
                        aggregate_bounds: vec![],
                    },
                }
            }
            (Some(n), None) if n > 0 && value.random == Some(true) => DiscardMode::Random {
                population: self.whole_hand(&player, path)?,
                count: Expr::Literal(Int(i64::try_from(n).map_err(|_overflow| unsupported(path))?)),
            },
            (None, None)
                if value.random.is_none()
                    && let Some(name) = &value.reference =>
            {
                let chosen = Selector::Bound {
                    reference: *self.targets.get(name).ok_or_else(|| unsupported(path))?,
                };
                let hand = self.whole_hand(&player, path)?;
                DiscardMode::All {
                    eligible: Selector::Intersection {
                        sets: vec![hand, chosen],
                    },
                }
            }
            (None, Some(true)) if value.random.is_none() && value.reference.is_none() => {
                DiscardMode::All {
                    eligible: self.whole_hand(&player, path)?,
                }
            }
            _ => return Err(unsupported(path)),
        };
        Ok(Action::Discard { player, mode })
    }
}

pub(super) fn subscription(
    discard: &ast::DiscardSubscription,
    event_id: u64,
    source_id: u64,
    scope: super::ScopeId,
    path: &str,
) -> Result<super::EventPattern, LoweringError> {
    if discard.reference.is_some()
        && (matches!(discard.who, Some(ast::EntryPlayer::Opponent)) || discard.during.is_some())
    {
        return Err(unsupported(path));
    }
    let event = Binding::new(event_id, scope);
    let by = discard
        .by
        .as_ref()
        .map(|by| {
            let ast::SelfPlayer::Controller = by.ability_of.who;
            if discard.reference.is_none()
                || !matches!(discard.who, Some(ast::EntryPlayer::Controller))
                || discard.during.is_some()
                || by.ability_of.r#trait.trim().is_empty()
            {
                return Err(unsupported(path));
            }
            Ok(DiscardSource {
                binding: Binding::new(source_id, scope),
                trait_id: TraitId(by.ability_of.r#trait.clone()),
            })
        })
        .transpose()?;
    let card = discard
        .sel
        .as_ref()
        .map(|sel| discarded_card(sel, discard, path))
        .transpose()?;
    Ok(super::EventPattern::Discard {
        binding: event,
        predicate: DiscardFilter {
            own: discard
                .who
                .as_ref()
                .map(|who| matches!(who, ast::EntryPlayer::Controller)),
            itself: discard.reference.is_some(),
            own_turn: discard.during.is_some(),
            by,
            card,
        }
        .predicate(event),
    })
}

/// Only constant kind and class filters on another discarded card are carried by the closed header.
fn discarded_card(
    sel: &ast::MovementSubject,
    discard: &ast::DiscardSubscription,
    path: &str,
) -> Result<DiscardCard, LoweringError> {
    let ast::MovementSubject {
        r#where: None,
        orig_cost_eq: None,
        orig_cost_le: None,
        orig_cost_ge: None,
        traits_any: None,
        keyword: None,
        class,
        not_class: None,
        kind,
        trait_id: None,
        name: None,
        name_contains: None,
        token: None,
        not_token: None,
        other: None,
    } = sel
    else {
        return Err(unsupported(path));
    };
    if discard.reference.is_some()
        || discard.by.is_some()
        || (kind.is_none() && class.is_none())
        || class.as_ref().is_some_and(|class| class.trim().is_empty())
    {
        return Err(unsupported(path));
    }
    Ok(DiscardCard {
        kind: kind.as_ref().map(super::search::card_kind),
        class: class.clone().map(ClassId),
    })
}

pub(super) fn aggregation(
    auto: Option<&ast::Auto>,
    subject_id: u64,
    scope: super::ScopeId,
    path: &str,
) -> Result<TriggerAggregation, LoweringError> {
    let Some(auto) = auto else {
        return Ok(TriggerAggregation::PerFact {});
    };
    match auto.aggregation.as_ref() {
        None => Ok(TriggerAggregation::PerFact {}),
        Some(ast::AutoAggregation::Batch) if matches!(&auto.on, ast::PhaseSubscription::Discarded(event) if event.reference.is_none() && matches!(event.who, Some(ast::EntryPlayer::Controller)) && event.during.is_some()) => {
            Ok(TriggerAggregation::PerBatch {
                key: BatchPolicy::Instruction {},
            })
        }
        Some(ast::AutoAggregation::Batch) if matches!(&auto.on, ast::PhaseSubscription::Dealt(event) if super::combat::batch_dealt(event, subject_id, scope, path)) => {
            Ok(TriggerAggregation::PerBatch {
                key: BatchPolicy::Instruction {},
            })
        }
        Some(ast::AutoAggregation::Batch) => Err(unsupported(path)),
    }
}
