use super::super::{Decision, DecisionId, DecisionKind, Declinable, Visibility};
use super::{BodyContext, Located, LoweringError, Node, PlayerRef, SourceSpan, ast, unsupported};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Resolution choices share the ability's object binding environment."
)]
impl BodyContext<'_> {
    pub(super) fn choose(
        &mut self,
        choice: &ast::Choose,
        path: &str,
        source: &SourceSpan,
    ) -> Result<Located<Node>, LoweringError> {
        if let Some(groups) = &choice.groups {
            return self.grouped_observed_choose(choice, groups, path, source);
        }
        // Exact zero needs the distinct ForcedZeroUnits instruction-start contract.
        if choice.n == Some(0) || choice.reveal == Some(true) || choice.order == Some(true) {
            return Err(unsupported(path));
        }
        let (n, upto) = if choice.may == Some(true) {
            match (choice.n, choice.upto) {
                (Some(1), None) => (None, Some(1)),
                (None, Some(_)) => (None, choice.upto),
                _ => return Err(unsupported(path)),
            }
        } else {
            (choice.n, choice.upto)
        };
        let target = ast::Target {
            bind: choice.bind.clone(),
            from: choice.from.clone(),
            distinct_by: choice.distinct_by.clone(),
            n,
            upto,
            option: None,
        };
        let (domain, cardinality) = self.object_choice(&target, path, true)?;
        let visibility = if target.from.observation.is_some() {
            Visibility::Only {
                players: super::super::PlayerSet {
                    relation: super::super::PlayerRelation::SelfPlayer,
                    relative_to: PlayerRef::Controller {},
                    cardinality: super::super::Cardinality {
                        minimum: super::super::Nat(1),
                        maximum: Some(super::super::Nat(1)),
                    },
                    text_quantifier: super::super::TextQuantifier::Implicit,
                },
            }
        } else {
            Visibility::Public {}
        };
        let mut located = source.clone();
        path.clone_into(&mut located.author_path);
        Ok(Located {
            source: located,
            node: Node::Decision {
                decision: Decision {
                    id: DecisionId(self.card_allocator.id(path)?),
                    captures: vec![],
                    actor: PlayerRef::Controller {},
                    domain,
                    kind: DecisionKind::ResolutionChoice {
                        may: if choice.may == Some(true) {
                            Declinable::MayDecline
                        } else {
                            Declinable::Required
                        },
                        cardinality,
                        precondition: None,
                    },
                    visibility,
                },
            },
        })
    }

    fn observed_cost_groups(
        &mut self,
        groups: &[ast::ChooseGroup],
        receipt: &super::super::ObjectReceipt,
        path: &str,
    ) -> Result<Vec<super::super::SelectionGroup>, LoweringError> {
        let group_cardinality = super::super::Cardinality {
            minimum: super::super::Nat(0),
            maximum: Some(super::super::Nat(1)),
        };
        let mut lowered_groups = Vec::with_capacity(groups.len());
        for group in groups {
            if !matches!(&group.kind, Some(ast::SearchKind::Follower))
                || group.orig_cost_le.is_none()
            {
                return Err(unsupported(path));
            }
            let binding = super::Binding::new(1, self.card_allocator.scope(path)?);
            lowered_groups.push(super::super::SelectionGroup {
                selector: super::super::Selector::Filter {
                    input: Box::new(super::super::Selector::ReceiptMembers {
                        receipt: receipt.clone(),
                        projection: super::super::MemberProjection::Before,
                    }),
                    binding,
                    predicate: Box::new(field_filter(
                        &ObjectFilter {
                            kind: group.kind.as_ref(),
                            original_cost: [None, group.orig_cost_le, None],
                            ..ObjectFilter::default()
                        },
                        binding,
                    )),
                },
                cardinality: group_cardinality,
            });
        }
        Ok(lowered_groups)
    }

    fn grouped_observed_choose(
        &mut self,
        choice: &ast::Choose,
        groups: &[ast::ChooseGroup],
        path: &str,
        source: &SourceSpan,
    ) -> Result<Located<Node>, LoweringError> {
        let observation = choice
            .from
            .observation
            .as_ref()
            .ok_or_else(|| unsupported(path))?;
        if groups.len() != 2
            || choice.may != Some(true)
            || choice.n.is_some()
            || choice.upto != Some(2)
            || choice.reveal == Some(true)
            || choice.order == Some(true)
            || choice.distinct_by.is_some()
            || choice.from.who.is_some()
            || choice.from.zone.is_some()
            || choice.from.leader.is_some()
            || choice.from.leaders.is_some()
            || choice.from.leader_or
            || !matches!(&choice.from.kind, Some(ast::SearchKind::Follower))
            || choice.from.r#trait.is_some()
            || choice.from.name.is_some()
            || choice.from.name_contains.is_some()
            || choice.from.other
            || choice.from.has_numeric_filter()
            || choice.from.class.is_some()
            || choice.from.not_class.is_some()
            || choice.from.has_evolve.is_some()
            || self.targets.contains_key(&choice.bind)
            || self.searches.contains(&choice.bind)
            || self.observations.contains_key(&choice.bind)
        {
            return Err(unsupported(path));
        }
        let receipt = self
            .observations
            .get(observation)
            .filter(|receipt| matches!(receipt, super::super::ObjectReceipt::Look { .. }))
            .ok_or_else(|| unsupported(path))?
            .clone();
        let cardinality = super::super::Cardinality {
            minimum: super::super::Nat(0),
            maximum: Some(super::super::Nat(2)),
        };
        let lowered_groups = self.observed_cost_groups(groups, &receipt, path)?;
        let binding = super::Binding::new(self.allocator.id(path)?, self.scope);
        if self.targets.insert(choice.bind.clone(), binding).is_some() {
            return Err(unsupported(path));
        }
        let domain = super::super::ChoiceDomain::Objects {
            constraint: super::super::SelectionConstraint {
                groups: lowered_groups,
                cardinality,
                distinct_by: None,
                disjoint: super::super::Disjointness::Disjoint,
                aggregate_bounds: vec![],
            },
            binding,
            target_roles: None,
        };
        let mut located = source.clone();
        path.clone_into(&mut located.author_path);
        Ok(Located {
            source: located,
            node: Node::Decision {
                decision: Decision {
                    id: DecisionId(self.card_allocator.id(path)?),
                    captures: vec![],
                    actor: PlayerRef::Controller {},
                    domain,
                    kind: DecisionKind::ResolutionChoice {
                        may: Declinable::MayDecline,
                        cardinality,
                        precondition: None,
                    },
                    visibility: Visibility::Only {
                        players: super::super::PlayerSet {
                            relation: super::super::PlayerRelation::SelfPlayer,
                            relative_to: PlayerRef::Controller {},
                            cardinality: super::super::Cardinality {
                                minimum: super::super::Nat(1),
                                maximum: Some(super::super::Nat(1)),
                            },
                            text_quantifier: super::super::TextQuantifier::Implicit,
                        },
                    },
                },
            },
        })
    }
}

#[derive(Default)]
pub(super) struct ObjectFilter<'query> {
    pub has_evolve: bool,
    pub kind: Option<&'query ast::SearchKind>,
    pub trait_id: Option<&'query str>,
    pub all_traits: &'query [String],
    pub class: Option<&'query str>,
    pub not_class: Option<&'query str>,
    pub token: Option<bool>,
    pub name: Option<&'query str>,
    pub contains: Option<&'query str>,
    pub cost: [Option<u64>; 3],
    pub original_cost: [Option<u64>; 3],
    pub hp_at_most: Option<u64>,
    pub power_at_most: Option<u64>,
    pub keyword: Option<super::super::KeywordId>,
}

impl<'query> From<&'query ast::TargetQuery> for ObjectFilter<'query> {
    fn from(query: &'query ast::TargetQuery) -> Self {
        Self {
            has_evolve: query.has_evolve.is_some(),
            kind: query.kind.as_ref(),
            trait_id: query.r#trait.as_deref(),
            all_traits: &[],
            class: query.class.as_deref(),
            not_class: query.not_class.as_deref(),
            token: None,
            name: query.name.as_deref(),
            contains: query.name_contains.as_deref(),
            cost: [query.cost_eq, query.cost_le, query.cost_ge],
            original_cost: [query.orig_cost_eq, query.orig_cost_le, query.orig_cost_ge],
            hp_at_most: query.hp_le,
            ..Self::default()
        }
    }
}

impl<'query> From<&'query ast::PublicQuery> for ObjectFilter<'query> {
    fn from(query: &'query ast::PublicQuery) -> Self {
        Self {
            has_evolve: query.has_evolve.is_some(),
            kind: query.kind.as_ref(),
            trait_id: query.r#trait.as_deref(),
            name: query.name.as_deref(),
            contains: query.name_contains.as_deref(),
            class: query.class.as_deref(),
            not_class: query.not_class.as_deref(),
            token: query.token,
            ..Self::default()
        }
    }
}

impl<'query> From<&'query ast::CountQuery> for ObjectFilter<'query> {
    fn from(query: &'query ast::CountQuery) -> Self {
        Self {
            has_evolve: query.has_evolve.is_some(),
            kind: query.kind.as_ref(),
            trait_id: query.r#trait.as_deref(),
            name: query.name.as_deref(),
            contains: query.name_contains.as_deref(),
            token: query.token.or_else(|| query.not_token.map(|token| !token)),
            original_cost: [query.orig_cost_eq, None, None],
            ..Self::default()
        }
    }
}

impl<'query> From<&'query ast::SearchQuery> for ObjectFilter<'query> {
    fn from(query: &'query ast::SearchQuery) -> Self {
        Self {
            has_evolve: query.has_evolve.is_some(),
            kind: query.kind.as_ref(),
            trait_id: query.r#trait.as_deref(),
            all_traits: &query.traits_all,
            class: query.class.as_deref(),
            contains: query.name_contains.as_deref(),
            name: query.name.as_deref(),
            cost: [query.cost_eq, query.cost_le, query.cost_ge],
            original_cost: [query.orig_cost_eq, query.orig_cost_le, query.orig_cost_ge],
            ..Self::default()
        }
    }
}

pub(super) fn field_filter(
    filter: &ObjectFilter<'_>,
    candidate: super::Binding<super::super::ObjectRef>,
) -> super::Expr<super::Bool> {
    use super::super::{CharacteristicView, Comparison, NumberRead};
    let object = super::ObjectView::Current {
        object: Box::new(super::ObjectOperand::Bound { binding: candidate }),
    };
    let base = super::search::object_filter(filter.kind, filter.trait_id, filter.name, candidate);
    let mut predicates = Vec::new();
    if !matches!(base, super::Expr::Literal(super::Bool(true))) {
        predicates.push(base);
    }
    for trait_id in filter.all_traits {
        predicates.push(super::search::object_filter(
            None,
            Some(trait_id),
            None,
            candidate,
        ));
    }
    if let Some(contains) = filter.contains {
        predicates.push(super::Expr::Compute(Box::new(
            super::Predicate::NameContains {
                object: object.clone(),
                name: super::Expr::Literal(super::super::CardName(contains.to_owned())),
            },
        )));
    }
    for (class, negate) in [(filter.class, false), (filter.not_class, true)] {
        if let Some(class) = class {
            let predicate = super::Expr::Compute(Box::new(super::Predicate::ClassEquals {
                left: super::Expr::Live(Box::new(super::super::ClassRead::Class {
                    object: object.clone(),
                    view: CharacteristicView::Effective,
                })),
                right: super::Expr::Literal(super::super::Info::Present(super::super::ClassId(
                    class.to_owned(),
                ))),
            }));
            predicates.push(if negate {
                super::Expr::Compute(Box::new(super::Predicate::Not { predicate }))
            } else {
                predicate
            });
        }
    }
    if let Some(token) = filter.token {
        let predicate = super::Expr::Live(Box::new(super::super::BoolRead::State {
            object: object.clone(),
            property: super::super::BooleanState::TokenStatus,
        }));
        predicates.push(if token {
            predicate
        } else {
            super::Expr::Compute(Box::new(super::Predicate::Not { predicate }))
        });
    }
    predicates.extend(numeric_limits(filter, &object));
    if filter.has_evolve {
        predicates.push(ability_filter(
            &object,
            super::super::KeywordId::Evolution,
            Some(super::super::AbilityKind::Activated),
        ));
    }
    if let Some(keyword) = filter.keyword {
        predicates.push(ability_filter(&object, keyword, None));
    }
    if let Some(limit) = filter.hp_at_most {
        predicates.push(number_limit(
            NumberRead::Stats {
                object,
                view: CharacteristicView::Effective,
                property: super::super::Stat::Life,
            },
            Comparison::LessEqual,
            limit,
        ));
    }
    match predicates.as_mut_slice() {
        [] => super::Expr::Literal(super::Bool(true)),
        [_] => predicates.swap_remove(0),
        _ => super::Expr::Compute(Box::new(super::Predicate::And { predicates })),
    }
}

fn ability_filter(
    object: &super::ObjectView,
    keyword: super::super::KeywordId,
    kind: Option<super::super::AbilityKind>,
) -> super::Expr<super::Bool> {
    super::Expr::Compute(Box::new(super::Predicate::HasAbility {
        abilities: super::Expr::Live(Box::new(super::super::AbilitiesRead::Abilities {
            object: object.clone(),
            view: super::super::CharacteristicView::Effective,
        })),
        predicate: Box::new(super::super::AbilityPredicate {
            kind,
            keyword: Some(keyword),
            except: vec![],
            holder: None,
            origin: None,
        }),
    }))
}

fn numeric_limits(
    filter: &ObjectFilter<'_>,
    object: &super::ObjectView,
) -> Vec<super::Expr<super::Bool>> {
    use super::super::{CharacteristicView, Comparison, NumberRead};
    let mut predicates = Vec::new();
    for (view, limits) in [
        (CharacteristicView::Effective, filter.cost),
        (CharacteristicView::Original, filter.original_cost),
    ] {
        for (limit, comparison) in limits.into_iter().zip([
            Comparison::Equal,
            Comparison::LessEqual,
            Comparison::GreaterEqual,
        ]) {
            if let Some(limit) = limit {
                predicates.push(number_limit(
                    NumberRead::Cost {
                        object: object.clone(),
                        view,
                    },
                    comparison,
                    limit,
                ));
            }
        }
    }
    if let Some(limit) = filter.power_at_most {
        predicates.push(number_limit(
            NumberRead::Stats {
                object: object.clone(),
                view: CharacteristicView::Effective,
                property: super::super::Stat::Attack,
            },
            Comparison::LessEqual,
            limit,
        ));
    }
    predicates
}

fn number_limit(
    information: super::super::NumberRead,
    comparison: super::super::Comparison,
    limit: u64,
) -> super::Expr<super::Bool> {
    super::Expr::Compute(Box::new(super::Predicate::CompareInt {
        left: super::Expr::Compute(Box::new(super::IntOperation::InformationOrZero {
            information,
        })),
        comparison,
        right: super::Expr::Literal(super::Int(i64::try_from(limit).unwrap_or(i64::MAX))),
    }))
}
