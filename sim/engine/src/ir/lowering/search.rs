use super::super::{
    AggregateBound, CardKind, CardName, CharacteristicView, Comparison, CostChange, CostKind,
    DecisionId, Disjointness, Effect, FollowPermission, InstallationSubjects, InstallationTime,
    KindRead, MemberProjection, MoveDestination, MoveSpec, MovementReceipt, NamesRead, NumberRead,
    NumericProperty, ObjectProperty, ObjectRef, OwnerZone, Posture, ReceiptReference,
    SearchContinuation, SearchDestination, SearchFound, SearchPartition, SelectionConstraint,
    SelectionGroup, Stat, TraitId, TraitsRead, ZoneRef,
};
use super::groups::{Groups, TotalBounds};
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

fn entry_posture(search: &ast::Search, path: &str) -> Result<Option<Posture>, LoweringError> {
    if search.acted.is_none() {
        return Ok(None);
    }
    if search.n != Some(1)
        || search.upto.is_some()
        || search.groups.is_some()
        || search.distinct_by.is_some()
        || search.cost_sum_le.is_some()
        || !matches!(
            search.to,
            ast::SearchDestination::Single(ast::SearchZone::Field)
        )
    {
        return Err(unsupported(path));
    }
    Ok(Some(Posture::Acted))
}

fn view(binding: Binding<ObjectRef>) -> ObjectView {
    ObjectView::Current {
        object: Box::new(ObjectOperand::Bound { binding }),
    }
}

fn and_filter(predicate: Expr<Bool>, clause: Expr<Bool>) -> Expr<Bool> {
    if predicate == Expr::Literal(Bool(true)) {
        clause
    } else {
        Expr::Compute(Box::new(Predicate::And {
            predicates: vec![predicate, clause],
        }))
    }
}

fn filter(
    context: &mut BodyContext<'_>,
    selector: &ast::SearchSelector,
    candidate: Binding<ObjectRef>,
    path: &str,
) -> Result<Expr<Bool>, LoweringError> {
    match selector {
        ast::SearchSelector::Query(query) => query_filter(context, query, candidate, path),
        ast::SearchSelector::Union { any_of } if !any_of.is_empty() => {
            let predicates = any_of
                .iter()
                .map(|query| query_filter(context, query, candidate, path))
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
    context: &mut BodyContext<'_>,
    query: &ast::SearchQuery,
    candidate: Binding<ObjectRef>,
    path: &str,
) -> Result<Expr<Bool>, LoweringError> {
    let ast::SelfPlayer::Controller = query.who;
    let ast::DeckZone::Deck = query.zone;
    let mut filter = super::selection::ObjectFilter::from(query);
    if let Some(keyword) = &query.keyword {
        filter.keyword = Some(
            super::keywords::ability_filter_id(keyword)
                .or_else(|| super::keywords::basic_keyword_id(keyword))
                .ok_or_else(|| unsupported(path))?,
        );
    }
    let original_power = match (&query.alias, &query.r#where) {
        (None, None) => None,
        (Some(alias), Some(condition))
            if !alias.is_empty() && condition.cmp.left.power_of == *alias =>
        {
            match condition.cmp.op {
                ast::Comparison::Le => {
                    filter.power_at_most = Some(condition.cmp.right);
                    None
                }
                ast::Comparison::Eq => Some(condition.cmp.right),
                ast::Comparison::Ne
                | ast::Comparison::Lt
                | ast::Comparison::Ge
                | ast::Comparison::Gt => {
                    return Err(unsupported(path));
                }
            }
        }
        _ => return Err(unsupported(path)),
    };
    let mut predicate = super::selection::field_filter(&filter, candidate);
    if let Some(limit) = original_power {
        let attack = super::selection::number_limit(
            NumberRead::Stats {
                object: view(candidate),
                view: CharacteristicView::Original,
                property: Stat::Attack,
            },
            Comparison::Equal,
            limit,
        );
        predicate = and_filter(predicate, attack);
    }
    if let Some(ast::SearchName::SameAs { same_as }) = &query.name {
        let names = context.paid_name_filter(same_as, candidate, path)?;
        predicate = and_filter(predicate, names);
    }
    if let Some(ast::SearchKindFilter::SameAs { same_as }) = &query.kind {
        let receipt = context.banish_receipt(&same_as.objects_of, path)?.clone();
        let member = Binding::new(1, context.card_allocator.scope(path)?);
        let kind = Expr::Compute(Box::new(Predicate::ReceiptAny {
            receipt,
            projection: MemberProjection::Before,
            member,
            predicate: Expr::Compute(Box::new(Predicate::KindEquals {
                left: Expr::Live(Box::new(KindRead::Kind {
                    object: view(candidate),
                    view: CharacteristicView::Original,
                })),
                right: Expr::Live(Box::new(KindRead::Kind {
                    object: ObjectView::Snapshot {
                        value: Box::new(Expr::Bound(member)),
                    },
                    view: CharacteristicView::Original,
                })),
            })),
        }));
        predicate = and_filter(predicate, kind);
    }
    let predicate = context.dynamic_limits(
        predicate,
        candidate,
        &[
            (
                query.cost_le.as_ref(),
                CharacteristicView::Effective,
                Comparison::LessEqual,
            ),
            (
                query.orig_cost_le.as_ref(),
                CharacteristicView::Original,
                Comparison::LessEqual,
            ),
            (
                query.orig_cost_eq.as_ref(),
                CharacteristicView::Original,
                Comparison::Equal,
            ),
        ],
        path,
    )?;
    if !query.not_same_name_as_this {
        return Ok(predicate);
    }
    Ok(exclude_source_name(predicate, candidate))
}

pub(super) fn exclude_source_name(
    predicate: Expr<Bool>,
    candidate: Binding<ObjectRef>,
) -> Expr<Bool> {
    let same_name = Expr::Compute(Box::new(Predicate::NamesIntersect {
        left: Expr::Live(Box::new(NamesRead::Names {
            object: view(candidate),
        })),
        right: Expr::Live(Box::new(NamesRead::Names {
            object: ObjectView::SourceInformation {},
        })),
    }));
    Expr::Compute(Box::new(Predicate::And {
        predicates: vec![
            predicate,
            Expr::Compute(Box::new(Predicate::Not {
                predicate: same_name,
            })),
        ],
    }))
}

/// Formal targets precede the body's source snapshot, so "これと同名" reads the holder's card.
pub(super) fn exclude_holder_name(
    predicate: Expr<Bool>,
    candidate: Binding<ObjectRef>,
) -> Expr<Bool> {
    let same_name = Expr::Compute(Box::new(Predicate::NamesIntersect {
        left: Expr::Live(Box::new(NamesRead::Names {
            object: view(candidate),
        })),
        right: Expr::Live(Box::new(NamesRead::Names {
            object: ObjectView::Current {
                object: Box::new(ObjectOperand::SourceHolder {}),
            },
        })),
    }));
    Expr::Compute(Box::new(Predicate::And {
        predicates: vec![
            predicate,
            Expr::Compute(Box::new(Predicate::Not {
                predicate: same_name,
            })),
        ],
    }))
}

pub(super) fn object_filter(
    kind: Option<&ast::SearchKind>,
    trait_id: Option<&str>,
    name: Option<&str>,
    candidate: Binding<ObjectRef>,
) -> Expr<Bool> {
    view_filter(kind.map(card_kind), trait_id, name, view(candidate))
}

pub(super) const fn card_kind(kind: &ast::SearchKind) -> CardKind {
    match kind {
        ast::SearchKind::Follower => CardKind::Follower,
        ast::SearchKind::Spell => CardKind::Spell,
        ast::SearchKind::Amulet => CardKind::Amulet,
        ast::SearchKind::EvolveFollower => CardKind::EvolveFollower,
    }
}

pub(super) fn view_filter(
    kind: Option<CardKind>,
    trait_id: Option<&str>,
    name: Option<&str>,
    object: ObjectView,
) -> Expr<Bool> {
    let mut predicates = Vec::new();
    if let Some(kind) = kind {
        predicates.push(Expr::Compute(Box::new(Predicate::KindEquals {
            left: Expr::Live(Box::new(KindRead::Kind {
                object: object.clone(),
                view: CharacteristicView::Effective,
            })),
            right: Expr::Literal(kind),
        })));
    }
    if let Some(trait_id) = trait_id {
        predicates.push(Expr::Compute(Box::new(Predicate::HasTrait {
            traits: Expr::Live(Box::new(TraitsRead::Traits {
                object: object.clone(),
                view: CharacteristicView::Effective,
            })),
            trait_id: TraitId(trait_id.to_owned()),
        })));
    }
    if let Some(name) = name {
        predicates.push(Expr::Compute(Box::new(Predicate::NamesIntersect {
            left: Expr::Live(Box::new(NamesRead::Names { object })),
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
        ast::SearchZone::Banish => MoveDestination::Owners {
            zone: OwnerZone::Banish,
        },
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

const fn movement(zone: &ast::SearchZone, initial_posture: Option<Posture>) -> MoveSpec {
    MoveSpec {
        to: destination(zone),
        initial_posture,
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
                ast::SearchDestination::Single(ast::SearchZone::Field)
            )
        {
            let followers = matches!(&search.sel, Some(ast::SearchSelector::Query(query)) if matches!(query.kind, Some(ast::SearchKindFilter::Literal(ast::SearchKind::Follower))));
            let upto = search.upto.as_ref().and_then(ast::NumericArgument::literal);
            let single = followers
                && matches!((search.n, upto), (Some(1), None) | (None, Some(1)))
                && !search
                    .upto
                    .as_ref()
                    .is_some_and(ast::NumericArgument::is_dynamic);
            let grouped = search.distinct_by.is_none()
                && search.groups.as_ref().is_some_and(|groups| !groups.is_empty() && groups.iter().all(|group| {
                    matches!(&group.sel, ast::SearchSelector::Query(query) if matches!(query.kind, Some(ast::SearchKindFilter::Literal(ast::SearchKind::Follower))))
                        && matches!((group.n, group.upto), (Some(1), None) | (None, Some(1)))
                }));
            if followers || grouped {
                self.field_outputs.insert(
                    name.clone(),
                    super::FieldOutput {
                        receipt: MovementReceipt::Search {
                            receipt: ReceiptReference::new(receipt),
                        },
                        // Grouped numeric follow must not widen the singleton evolution permission.
                        kind: if single {
                            super::FieldOutputKind::SingleFollower
                        } else {
                            super::FieldOutputKind::Other
                        },
                    },
                );
            }
        }
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
    /// One group over the whole filtered deck, for constraints across every found card.
    fn deck_search_group(
        &mut self,
        cardinality: Cardinality,
        distinct: bool,
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
            distinct_by: distinct.then_some(ObjectProperty::Names),
            disjoint: Disjointness::Disjoint,
            aggregate_bounds: vec![],
        })
    }

    /// 元のコストの合計がN以下 bounds every found card, grouped or not.
    fn capped_search_groups(
        &mut self,
        search: &ast::Search,
        cardinality: Cardinality,
        groups: Option<SelectionConstraint>,
        path: &str,
    ) -> Result<Option<SelectionConstraint>, LoweringError> {
        let Some(limit) = search.cost_sum_le else {
            return Ok(groups);
        };
        if !matches!(search.to, ast::SearchDestination::Single(_)) {
            return Err(unsupported(path));
        }
        let mut constraint = match groups {
            Some(constraint) => constraint,
            None => self.deck_search_group(cardinality, false, path)?,
        };
        constraint
            .aggregate_bounds
            .push(super::play::original_cost_bound(limit, path)?);
        Ok(Some(constraint))
    }

    fn search_count_bound(
        &mut self,
        search: &ast::Search,
        path: &str,
    ) -> Result<Option<AggregateBound>, LoweringError> {
        let Some(ast::NumericArgument::Expression(expression)) = &search.upto else {
            return Ok(None);
        };
        if search.n.is_some() || search.sel.is_none() {
            return Err(unsupported(path));
        }
        let number = super::decode(expression.as_ref().clone(), path)?;
        let value = self.number(&number, path, &mut super::BTreeSet::new())?;
        if !super::super::public_numbers::check(&value) {
            return Err(unsupported(path));
        }
        Ok(Some(AggregateBound {
            property: NumericProperty::Count,
            comparison: Comparison::LessEqual,
            value,
        }))
    }

    pub(super) fn search(
        &mut self,
        search: &ast::Search,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let candidate = Binding::new(1, self.card_allocator.scope(path)?);
        let upper_bound = self.search_count_bound(search, path)?;
        let (filter, cardinality, groups) = if let Some(query) = &search.sel {
            let predicate = filter(self, query, candidate, path)?;
            let cardinality = if upper_bound.is_some() {
                Cardinality {
                    minimum: Nat(0),
                    maximum: None,
                }
            } else {
                quantity(
                    search.n,
                    search.upto.as_ref().and_then(ast::NumericArgument::literal),
                    path,
                )?
            };
            if search.distinct_by.is_some() {
                (
                    predicate,
                    cardinality,
                    Some(self.deck_search_group(cardinality, true, path)?),
                )
            } else {
                (predicate, cardinality, None)
            }
        } else if let Some(groups) = &search.groups {
            let mut lowered = Groups::new(TotalBounds::Sum);
            for group in groups {
                let cardinality = quantity(group.n, group.upto, path)?;
                lowered.push(
                    cardinality,
                    || {
                        let binding = Binding::new(1, self.card_allocator.scope(path)?);
                        let predicate = filter(self, &group.sel, binding, path)?;
                        if matches!(predicate, Expr::Literal(_)) {
                            return Err(unsupported(path));
                        }
                        Ok(Selector::Zone {
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
                        })
                    },
                    path,
                )?;
            }
            let constraint =
                lowered.finish(search.distinct_by.as_ref().map(|_| ObjectProperty::Names));
            let cardinality = constraint.cardinality;
            (Expr::Literal(Bool(true)), cardinality, Some(constraint))
        } else {
            return Err(unsupported(path));
        };
        let mut groups = self.capped_search_groups(search, cardinality, groups, path)?;
        if let Some(bound) = upper_bound {
            let mut constraint = match groups {
                Some(constraint) => constraint,
                None => self.deck_search_group(cardinality, false, path)?,
            };
            constraint.aggregate_bounds.push(bound);
            groups = Some(constraint);
        }
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
        let destination = self.search_destination(search, cardinality, continuation.found, path)?;
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
        value: &ast::Search,
        cardinality: Cardinality,
        found: Binding<SearchFound>,
        path: &str,
    ) -> Result<SearchDestination, LoweringError> {
        let initial_posture = entry_posture(value, path)?;
        let entry_effects = super::ordering::entry_effects(value.entry_effects.as_deref(), path)?;
        if !entry_effects.is_empty()
            && !matches!(
                value.to,
                ast::SearchDestination::Single(ast::SearchZone::Field)
            )
        {
            return Err(unsupported(path));
        }
        match &value.to {
            ast::SearchDestination::Single(zone) => Ok(SearchDestination::Single {
                movement: MoveSpec {
                    entry_effects,
                    ..movement(zone, initial_posture)
                },
            }),
            ast::SearchDestination::Partition {
                order: ast::Printed::Printed,
                partition,
            } => {
                if partition.len() < 2 {
                    return Err(unsupported(path));
                }
                let mut destinations = Vec::new();
                for part in partition {
                    destinations.push(SearchPartition {
                        decision: DecisionId(self.card_allocator.id(path)?),
                        zone: destination(&part.zone),
                        cardinality: quantity(part.n, part.upto, path)?,
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
                    movement: movement(&instead.zone, None),
                })
            }
        }
    }

    pub(super) fn evolution_discount(
        &mut self,
        installation: &ast::Apply,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let ast::InstalledEffect::EvolveCostMod(modifier) = &installation.effect else {
            return Err(unsupported(path));
        };
        if !matches!(modifier.on, ast::EvolutionCostSubject::Affected) {
            return Err(unsupported(path));
        }
        let installed_delta = modifier
            .delta
            .as_ref()
            .map(|value| value.as_i64().ok_or_else(|| unsupported(path)))
            .transpose()?;
        let change = match (installed_delta, modifier.set) {
            (Some(delta), None) => CostChange::Delta {
                amount: Expr::Literal(Int(delta)),
            },
            (None, Some(set)) if set >= 0 => CostChange::Set {
                amount: Expr::Literal(Int(set)),
            },
            _ => return Err(unsupported(path)),
        };
        let objects = self.objects(&installation.on, path)?;
        let affected = Binding::new(1, self.card_allocator.scope(path)?);
        let duration = numeric::duration(installation.until.as_ref(), path)?;
        Ok(Action::InstallEffect {
            subjects: InstallationSubjects::Objects { objects, affected },
            capture: InstallationTime::InstructionStart,
            effect: Box::new(Effect::CostModifier {
                kind: CostKind::Evolution,
                subjects: Selector::Bound {
                    reference: affected,
                },
                change,
                duration: duration.clone(),
                usage: None,
            }),
            duration,
        })
    }

    pub(super) fn discount(
        &mut self,
        installation: &ast::Apply,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let ast::InstalledEffect::CostMod(modifier) = &installation.effect else {
            return Err(unsupported(path));
        };
        let objects = self.objects(&installation.on, path)?;
        let ast::Playing::Affected(filter) = &modifier.when_playing else {
            return Err(unsupported(path));
        };
        let ast::Affected::Affected = filter.subjects;
        let affected = Binding::new(1, self.card_allocator.scope(path)?);
        let change = match (modifier.delta.as_ref(), modifier.set) {
            (Some(delta), None) => {
                let number = super::decode(delta.clone(), path)?;
                // Installed values introduce local query bindings inside the affected-set scope.
                let previous = self.scope;
                self.scope = affected.key().0;
                let amount = self.number(&number, path, &mut super::BTreeSet::new());
                self.scope = previous;
                CostChange::Delta { amount: amount? }
            }
            (None, Some(set)) if set >= 0 => CostChange::Set {
                amount: Expr::Literal(Int(set)),
            },
            _ => return Err(unsupported(path)),
        };
        if modifier.next_only.is_some()
            || !matches!(installation.until, None | Some(ast::Until::Turn))
        {
            return Err(unsupported(path));
        }
        let duration = numeric::duration(installation.until.as_ref(), path)?;
        Ok(Action::InstallEffect {
            subjects: InstallationSubjects::Objects { objects, affected },
            capture: InstallationTime::InstructionStart,
            effect: Box::new(Effect::CostModifier {
                kind: CostKind::Play,
                subjects: Selector::Bound {
                    reference: affected,
                },
                change,
                duration: duration.clone(),
                usage: None,
            }),
            duration,
        })
    }
}
