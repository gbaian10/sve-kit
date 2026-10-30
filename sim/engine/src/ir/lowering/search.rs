use super::super::{
    CardKind, CardName, CharacteristicView, Comparison, CostChange, DecisionId, Disjointness,
    Effect, FollowPermission, InstallationSubjects, InstallationTime, KindRead, MoveDestination,
    MoveSpec, MovementReceipt, NamesRead, NumberRead, ObjectProperty, ObjectRef, OwnerZone,
    ReceiptReference, SearchContinuation, SearchDestination, SearchFound, SearchPartition,
    SelectionConstraint, SelectionGroup, TraitId, TraitsRead, ZoneRef,
};
use super::{
    Action, Binding, BodyContext, Bool, Cardinality, Expr, Int, IntOperation, LoweringError, Nat,
    ObjectOperand, ObjectView, PlayerRef, PlayerRelation, PlayerSet, Predicate, ReceiptId,
    SelectionView, Selector, TextQuantifier, ZoneKind, ast, numeric, unsupported,
};

fn quantity(n: Option<u64>, upto: Option<u64>, path: &str) -> Result<Cardinality, LoweringError> {
    match (n, upto) {
        (Some(count), None) => Ok(Cardinality {
            minimum: Nat(count),
            maximum: Some(Nat(count)),
        }),
        (None, Some(count)) => Ok(Cardinality {
            minimum: Nat(0),
            maximum: Some(Nat(count)),
        }),
        _ => Err(unsupported(path)),
    }
}

fn view(binding: Binding<ObjectRef>) -> ObjectView {
    ObjectView::Current {
        object: Box::new(ObjectOperand::Bound { binding }),
    }
}

fn filter(
    selector: &ast::SearchSelector,
    candidate: Binding<ObjectRef>,
    path: &str,
) -> Result<Expr<Bool>, LoweringError> {
    match selector {
        ast::SearchSelector::Query(query) => query_filter(query, candidate, path),
        ast::SearchSelector::Union { any_of } if !any_of.is_empty() => {
            let predicates = any_of
                .iter()
                .map(|query| query_filter(query, candidate, path))
                .collect::<Result<Vec<_>, _>>()?;
            if predicates
                .iter()
                .any(|predicate| matches!(predicate, Expr::Literal(_)))
            {
                return Err(unsupported(path));
            }
            Ok(Expr::Compute(Box::new(Predicate::Or { predicates })))
        }
        ast::SearchSelector::Union { .. } => Err(unsupported(path)),
    }
}

fn query_filter(
    query: &ast::SearchQuery,
    candidate: Binding<ObjectRef>,
    path: &str,
) -> Result<Expr<Bool>, LoweringError> {
    let ast::SelfPlayer::Controller = query.who;
    let ast::DeckZone::Deck = query.zone;
    let mut filter = super::selection::ObjectFilter::from(query);
    if let Some(keyword) = &query.keyword {
        filter.keyword =
            Some(super::keywords::keyword_id(keyword).ok_or_else(|| unsupported(path))?);
    }
    filter.power_at_most = match (&query.alias, &query.r#where) {
        (None, None) => None,
        (Some(alias), Some(condition))
            if !alias.is_empty()
                && condition.cmp.left.power_of == *alias
                && matches!(condition.cmp.op, ast::Comparison::Le) =>
        {
            Some(condition.cmp.right)
        }
        _ => return Err(unsupported(path)),
    };
    let predicate = super::selection::field_filter(&filter, candidate);
    if !query.not_same_name_as_this {
        return Ok(predicate);
    }
    let same_name = Expr::Compute(Box::new(Predicate::NamesIntersect {
        left: Expr::Live(Box::new(NamesRead::Names {
            object: view(candidate),
        })),
        right: Expr::Live(Box::new(NamesRead::Names {
            object: ObjectView::SourceInformation {},
        })),
    }));
    Ok(Expr::Compute(Box::new(Predicate::And {
        predicates: vec![
            predicate,
            Expr::Compute(Box::new(Predicate::Not {
                predicate: same_name,
            })),
        ],
    })))
}

pub(super) fn object_filter(
    kind: Option<&ast::SearchKind>,
    trait_id: Option<&str>,
    name: Option<&str>,
    candidate: Binding<ObjectRef>,
) -> Expr<Bool> {
    let mut predicates = Vec::new();
    if let Some(kind) = kind {
        predicates.push(Expr::Compute(Box::new(Predicate::KindEquals {
            left: Expr::Live(Box::new(KindRead::Kind {
                object: view(candidate),
                view: CharacteristicView::Effective,
            })),
            right: Expr::Literal(match kind {
                ast::SearchKind::Follower => CardKind::Follower,
                ast::SearchKind::Spell => CardKind::Spell,
                ast::SearchKind::Amulet => CardKind::Amulet,
            }),
        })));
    }
    if let Some(trait_id) = trait_id {
        predicates.push(Expr::Compute(Box::new(Predicate::HasTrait {
            traits: Expr::Live(Box::new(TraitsRead::Traits {
                object: view(candidate),
                view: CharacteristicView::Effective,
            })),
            trait_id: TraitId(trait_id.to_owned()),
        })));
    }
    if let Some(name) = name {
        predicates.push(Expr::Compute(Box::new(Predicate::NamesIntersect {
            left: Expr::Live(Box::new(NamesRead::Names {
                object: view(candidate),
            })),
            right: Expr::Literal(vec![CardName(name.to_owned())]),
        })));
    }
    if predicates.is_empty() {
        Expr::Literal(Bool(true))
    } else {
        Expr::Compute(Box::new(Predicate::And { predicates }))
    }
}

const fn destination(zone: &ast::SearchZone) -> MoveDestination {
    match zone {
        ast::SearchZone::Hand => MoveDestination::Owners {
            zone: OwnerZone::Hand,
        },
        ast::SearchZone::Cemetery => MoveDestination::Owners {
            zone: OwnerZone::Cemetery,
        },
        ast::SearchZone::Ex | ast::SearchZone::Field => MoveDestination::Shared {
            zone: ZoneRef {
                player: PlayerRef::Controller {},
                kind: if matches!(zone, ast::SearchZone::Ex) {
                    ZoneKind::Ex
                } else {
                    ZoneKind::Field
                },
            },
        },
    }
}

const fn movement(zone: &ast::SearchZone) -> MoveSpec {
    MoveSpec {
        to: destination(zone),
        initial_posture: None,
        entry_effects: vec![],
        position: None,
        order: None,
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Search owns its private selectors and movement continuations."
)]
impl BodyContext<'_> {
    pub(super) fn search_result(
        &mut self,
        step: &ast::Step,
        receipt: ReceiptId,
        path: &str,
    ) -> Result<(), LoweringError> {
        if let ast::Step::Search(search) = step
            && let Some(name) = &search.result
            && matches!(
                search.to,
                ast::SearchDestination::Single(ast::SearchZone::Ex)
            )
        {
            self.ex_outputs.insert(
                name.clone(),
                Selector::MoveAfter {
                    receipt: MovementReceipt::Search {
                        receipt: ReceiptReference::new(receipt),
                    },
                    permission: FollowPermission::SameEffectExplicitReference {},
                },
            );
        }
        if let ast::Step::Search(search) = step
            && let Some(name) = &search.result
            && !self.searches.insert(name.clone())
        {
            return Err(unsupported(path));
        }
        Ok(())
    }
    fn distinct_search_group(
        &mut self,
        cardinality: Cardinality,
        path: &str,
    ) -> Result<SelectionConstraint, LoweringError> {
        let binding = Binding::new(1, self.card_allocator.scope(path)?);
        Ok(SelectionConstraint {
            groups: vec![SelectionGroup {
                selector: Selector::Zone {
                    players: PlayerSet {
                        relation: PlayerRelation::SelfPlayer,
                        relative_to: PlayerRef::Controller {},
                        cardinality: Cardinality {
                            minimum: Nat(1),
                            maximum: Some(Nat(1)),
                        },
                        text_quantifier: TextQuantifier::Implicit,
                    },
                    zone: ZoneKind::Deck,
                    view: SelectionView::Current {},
                    binding,
                    predicate: Box::new(Expr::Literal(Bool(true))),
                },
                cardinality,
            }],
            cardinality,
            distinct_by: Some(ObjectProperty::Names),
            disjoint: Disjointness::Disjoint,
            aggregate_bounds: vec![],
        })
    }

    pub(super) fn search(
        &mut self,
        search: &ast::Search,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let candidate = Binding::new(1, self.card_allocator.scope(path)?);
        let (filter, cardinality, groups) = if let Some(query) = &search.sel {
            let predicate = filter(query, candidate, path)?;
            let cardinality = quantity(search.n, search.upto, path)?;
            if search.distinct_by.is_some() {
                (
                    predicate,
                    cardinality,
                    Some(self.distinct_search_group(cardinality, path)?),
                )
            } else {
                (predicate, cardinality, None)
            }
        } else if let Some(groups) = &search.groups {
            let mut lowered = Vec::new();
            let mut minimum = 0_u64;
            let mut maximum = 0_u64;
            for group in groups {
                let cardinality = quantity(group.n, group.upto, path)?;
                minimum = minimum
                    .checked_add(cardinality.minimum.0)
                    .ok_or_else(|| unsupported(path))?;
                maximum = maximum
                    .checked_add(cardinality.maximum.ok_or_else(|| unsupported(path))?.0)
                    .ok_or_else(|| unsupported(path))?;
                let binding = Binding::new(1, self.card_allocator.scope(path)?);
                let predicate = filter(&group.sel, binding, path)?;
                if matches!(predicate, Expr::Literal(_)) {
                    return Err(unsupported(path));
                }
                lowered.push(SelectionGroup {
                    selector: Selector::Zone {
                        players: PlayerSet {
                            relation: PlayerRelation::SelfPlayer,
                            relative_to: PlayerRef::Controller {},
                            cardinality: Cardinality {
                                minimum: Nat(1),
                                maximum: Some(Nat(1)),
                            },
                            text_quantifier: TextQuantifier::Implicit,
                        },
                        zone: ZoneKind::Deck,
                        view: SelectionView::Current {},
                        binding,
                        predicate: Box::new(predicate),
                    },
                    cardinality,
                });
            }
            let cardinality = Cardinality {
                minimum: Nat(minimum),
                maximum: Some(Nat(maximum)),
            };
            (
                Expr::Literal(Bool(true)),
                cardinality,
                Some(SelectionConstraint {
                    groups: lowered,
                    cardinality,
                    distinct_by: search.distinct_by.as_ref().map(|_| ObjectProperty::Names),
                    disjoint: Disjointness::Disjoint,
                    aggregate_bounds: vec![],
                }),
            )
        } else {
            return Err(unsupported(path));
        };
        if matches!(filter, Expr::Literal(_))
            && groups.is_none()
            && matches!(
                search.to,
                ast::SearchDestination::Single(ast::SearchZone::Field)
                    | ast::SearchDestination::Replacement { .. }
            )
        {
            return Err(unsupported(path));
        }
        let scope = self.card_allocator.scope(path)?;
        let continuation = SearchContinuation {
            found: Binding::new(1, scope),
            revealed: Binding::new(2, scope),
        };
        let destination =
            self.search_destination(&search.to, cardinality, continuation.found, path)?;
        Ok(Action::Search {
            decision: DecisionId(self.card_allocator.id(path)?),
            player: PlayerRef::Controller {},
            deck: PlayerRef::Controller {},
            candidate,
            filter,
            cardinality,
            groups,
            destination: Some(destination),
            continuation,
        })
    }

    fn search_destination(
        &mut self,
        value: &ast::SearchDestination,
        cardinality: Cardinality,
        found: Binding<SearchFound>,
        path: &str,
    ) -> Result<SearchDestination, LoweringError> {
        match value {
            ast::SearchDestination::Single(zone) => Ok(SearchDestination::Single {
                movement: movement(zone),
            }),
            ast::SearchDestination::Partition {
                order: ast::Printed::Printed,
                partition,
            } => {
                if !matches!(partition.as_slice(), [hand, cemetery]
                    if matches!(hand.zone, ast::SearchZone::Hand) && matches!(cemetery.zone, ast::SearchZone::Cemetery)
                    && cardinality.maximum.is_some_and(|max| max.0.saturating_sub(hand.n) <= cemetery.n))
                {
                    return Err(unsupported(path));
                }
                let mut destinations = Vec::new();
                for part in partition {
                    destinations.push(SearchPartition {
                        decision: DecisionId(self.card_allocator.id(path)?),
                        zone: destination(&part.zone),
                        cardinality: quantity(Some(part.n), None, path)?,
                    });
                }
                Ok(SearchDestination::Partition { destinations })
            }
            ast::SearchDestination::Replacement { zone, instead } => {
                if !matches!(zone, ast::SearchZone::Hand)
                    || !matches!(instead.zone, ast::SearchZone::Field)
                    || instead.may != Some(true)
                    || cardinality.maximum.is_none_or(|max| max.0 > 1)
                {
                    return Err(unsupported(path));
                }
                let ast::LessEqual::LessEqual = instead.r#if.cmp.op;
                let ast::Found::Found = instead.r#if.cmp.left.cost_of;
                let object = Binding::new(1, self.card_allocator.scope(path)?);
                Ok(SearchDestination::OptionalSelfReplacement {
                    decision: DecisionId(self.card_allocator.id(path)?),
                    predicate: Expr::Compute(Box::new(Predicate::SearchFoundAny {
                        found,
                        object,
                        predicate: Expr::Compute(Box::new(Predicate::CompareInt {
                            left: Expr::Compute(Box::new(IntOperation::InformationOrZero {
                                information: NumberRead::Cost {
                                    object: view(object),
                                    view: CharacteristicView::Effective,
                                },
                            })),
                            comparison: Comparison::LessEqual,
                            right: Expr::Literal(Int(instead.r#if.cmp.right)),
                        })),
                    })),
                    movement: movement(&instead.zone),
                })
            }
        }
    }

    pub(super) fn discount(
        &mut self,
        installation: &ast::Apply,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let ast::InstalledEffect::CostMod(modifier) = &installation.effect else {
            return Err(unsupported(path));
        };
        let ast::Objects::Follow { follow } = &installation.on else {
            return Err(unsupported(path));
        };
        let objects = self
            .ex_outputs
            .get(follow)
            .cloned()
            .ok_or_else(|| unsupported(path))?;
        let ast::Affected::Affected = modifier.when_playing.subjects;
        if modifier.delta > 0 || !matches!(installation.until, Some(ast::Until::Turn)) {
            return Err(unsupported(path));
        }
        let affected = Binding::new(1, self.card_allocator.scope(path)?);
        let duration = numeric::duration(installation.until.as_ref(), path)?;
        Ok(Action::InstallEffect {
            subjects: InstallationSubjects::Objects { objects, affected },
            capture: InstallationTime::InstructionStart,
            effect: Box::new(Effect::CostModifier {
                subjects: Selector::Bound {
                    reference: affected,
                },
                change: CostChange::Delta {
                    amount: Expr::Literal(Int(modifier.delta)),
                },
                duration: duration.clone(),
                usage: None,
            }),
            duration,
        })
    }
}
