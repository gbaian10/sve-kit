use super::super::{Decision, DecisionId, DecisionKind, Declinable, Visibility};
use super::groups::{Groups, TotalBounds};
use super::{BodyContext, Located, LoweringError, Node, PlayerRef, SourceSpan, ast, unsupported};
use crate::ir::public_numbers;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Resolution choices share the ability's object binding environment."
)]
impl BodyContext<'_> {
    pub(super) fn optional(
        &mut self,
        optional: &ast::Optional,
        path: &str,
        source: &SourceSpan,
        ability_source: &super::AbilitySource,
    ) -> Result<Located<Node>, LoweringError> {
        let actor = self.participant(optional.who.as_ref(), path)?;
        let parent = self.scope;
        // `then` continues the same optional body, so payment results and other local names
        // stay readable until the whole optional ends.
        let saved = self.local_bindings();
        self.scope = self.card_allocator.scope(path)?;
        let binding = super::Binding::new(self.allocator.id(path)?, self.scope);
        let mut body = self.step_list(
            &optional.steps,
            &format!("{path}/optional/do"),
            source,
            ability_source,
        )?;
        if let Some(continuation) = &optional.then {
            body.steps.extend(
                self.step_list(
                    continuation,
                    &format!("{path}/optional/then"),
                    source,
                    ability_source,
                )?
                .steps,
            );
        }
        self.scope = parent;
        self.restore_bindings(saved);
        let bounds = super::Cardinality {
            minimum: super::Nat(0),
            maximum: Some(super::Nat(1)),
        };
        let id = self.card_allocator.id(path)?;
        let mut located = source.clone();
        path.clone_into(&mut located.author_path);
        Ok(Located {
            source: located,
            node: Node::Control {
                control: super::super::Control::May {
                    body,
                    receipt: super::ReceiptId(id),
                    choice: Decision {
                        id: DecisionId(id),
                        captures: vec![],
                        actor,
                        domain: super::super::ChoiceDomain::Integer {
                            cardinality: bounds,
                            binding,
                        },
                        kind: DecisionKind::ResolutionChoice {
                            may: Declinable::MayDecline,
                            cardinality: bounds,
                            precondition: None,
                        },
                        visibility: Visibility::Public {},
                    },
                },
            },
        })
    }

    pub(super) fn choose(
        &mut self,
        choice: &ast::Choose,
        path: &str,
        source: &SourceSpan,
    ) -> Result<Located<Node>, LoweringError> {
        let (from, union) = match &choice.from {
            ast::ChooseFrom::Advance(_) => return Err(unsupported(path)),
            ast::ChooseFrom::Query(query) => (query.as_ref(), None),
            ast::ChooseFrom::Union { any_of } => match any_of.as_slice() {
                [first, _, ..] if private_union(any_of) => (first, Some(any_of.as_slice())),
                _ => return Err(unsupported(path)),
            },
        };
        let hand = matches!(
            from.zone,
            Some(ast::TargetZone::Single(ast::TargetSingleZone::Hand))
        );
        let actor = self.chooser(choice, from, hand, path)?;
        if let Some(groups) = &choice.groups {
            if union.is_some() {
                return Err(unsupported(path));
            }
            return self.grouped_observed_choose(choice, from, groups, actor, path, source);
        }
        // Exact zero needs the distinct ForcedZeroUnits instruction-start contract.
        if choice.n == Some(0) || choice.reveal == Some(true) || choice.order == Some(true) {
            return Err(unsupported(path));
        }
        let (n, upto) = if choice.may == Some(true) && !choice.any {
            match (choice.n, choice.upto) {
                (Some(1), None) => (None, Some(1)),
                (None, Some(_)) => (None, choice.upto),
                _ => return Err(unsupported(path)),
            }
        } else {
            (choice.n, choice.upto)
        };
        let target = ast::Target {
            any: choice.any,
            groups: None,
            bind: choice.bind.clone(),
            from: from.clone(),
            any_of: None,
            distinct_by: choice.distinct_by.clone(),
            cost_sum_le: choice.cost_sum_le,
            roles: None,
            n: n.map(ast::NumericArgument::Literal),
            upto: upto.map(ast::NumericArgument::Literal),
            option: None,
        };
        let (domain, cardinality) = match union {
            None => self.object_choice(&target, path, true)?,
            Some(branches) if hand => self.hand_union_choice(&target, branches, path)?,
            Some(branches) => self.observed_object_choice(&target, branches, path, true)?,
        };
        if matches!(domain, super::super::ChoiceDomain::Objects { .. })
            && exact_own_cemetery_choice(choice, cardinality)
        {
            self.cemetery_banish_targets.insert(choice.bind.clone());
        }
        if hand
            && actor != (PlayerRef::Controller {})
            && let Some(binding) = self.targets.get(&choice.bind)
        {
            self.field_entry
                .holders
                .insert(binding.key(), actor.clone());
        }
        let visibility = self.choice_visibility(target.from.observation.as_deref(), hand, &actor);
        let mut located = source.clone();
        path.clone_into(&mut located.author_path);
        Ok(Located {
            source: located,
            node: Node::Decision {
                decision: Decision {
                    id: DecisionId(self.card_allocator.id(path)?),
                    captures: vec![],
                    actor,
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

    /// Hands and looked cards are shown only to the chooser; a public reveal is shown to all.
    fn choice_visibility(
        &self,
        observation: Option<&str>,
        hand: bool,
        actor: &PlayerRef,
    ) -> Visibility {
        let revealed = observation.is_some_and(|name| {
            matches!(
                self.observations.get(name),
                Some(super::super::ObjectReceipt::Reveal { .. })
            )
        });
        if (observation.is_some() && !revealed) || hand {
            Visibility::Only {
                players: super::super::PlayerSet {
                    relation: super::super::PlayerRelation::SelfPlayer,
                    relative_to: actor.clone(),
                    cardinality: super::super::Cardinality {
                        minimum: super::super::Nat(1),
                        maximum: Some(super::super::Nat(1)),
                    },
                    text_quantifier: super::super::TextQuantifier::Implicit,
                },
            }
        } else {
            Visibility::Public {}
        }
    }

    /// A private source is shown only to its holder: another player may choose from its own
    /// hand, while looked cards stay with the controller who looked. Publicly revealed cards
    /// may be chosen by any participant.
    fn chooser(
        &mut self,
        choice: &ast::Choose,
        from: &ast::TargetQuery,
        hand: bool,
        path: &str,
    ) -> Result<PlayerRef, LoweringError> {
        let actor = self.participant(choice.by.as_ref(), path)?;
        let revealed = from.observation.as_ref().is_some_and(|name| {
            matches!(
                self.observations.get(name),
                Some(super::super::ObjectReceipt::Reveal { .. })
            )
        });
        let own_hand = hand
            && match (&choice.by, &from.who) {
                (
                    Some(ast::Participant::Side(ast::LeaderSide::Opponent)),
                    Some(ast::Side::Opponent),
                ) => true,
                (Some(ast::Participant::Bound(chooser)), Some(ast::Side::Bound(holder))) => {
                    chooser == holder
                }
                _ => false,
            };
        if ((from.observation.is_some() && !revealed) || hand)
            && actor != (PlayerRef::Controller {})
            && !own_hand
        {
            return Err(unsupported(path));
        }
        Ok(actor)
    }

    fn observed_groups(
        &mut self,
        groups: &[ast::ChooseGroup],
        receipt: &super::super::ObjectReceipt,
        cardinality: super::Cardinality,
        exclude_source_name: bool,
        path: &str,
    ) -> Result<super::super::SelectionConstraint, LoweringError> {
        let group_cardinality = super::super::Cardinality {
            minimum: super::super::Nat(0),
            maximum: Some(super::super::Nat(1)),
        };
        let mut lowered_groups = Groups::new(TotalBounds::Explicit(cardinality));
        for group in groups {
            if (group.kind.is_none() && group.r#trait.is_none() && group.name.is_none())
                || group.r#trait.as_deref() == Some("")
                || group.name.as_deref() == Some("")
            {
                return Err(unsupported(path));
            }
            lowered_groups.push(
                group_cardinality,
                || {
                    let binding = super::Binding::new(1, self.card_allocator.scope(path)?);
                    let predicate = field_filter(
                        &ObjectFilter {
                            kind: group.kind.as_ref().map(super::search::card_kind),
                            name: group.name.as_deref(),
                            trait_id: group.r#trait.as_deref(),
                            class: group.class.as_deref(),
                            original_cost: [None, group.orig_cost_le, None],
                            ..ObjectFilter::default()
                        },
                        binding,
                    );
                    let predicate = if exclude_source_name {
                        super::search::exclude_source_name(predicate, binding)
                    } else {
                        predicate
                    };
                    Ok(super::super::Selector::Filter {
                        input: Box::new(super::super::Selector::ReceiptMembers {
                            receipt: receipt.clone(),
                            projection: super::super::MemberProjection::Before,
                        }),
                        binding,
                        predicate: Box::new(predicate),
                    })
                },
                path,
            )?;
        }
        Ok(lowered_groups.finish(None))
    }

    fn grouped_observed_choose(
        &mut self,
        choice: &ast::Choose,
        from: &ast::TargetQuery,
        groups: &[ast::ChooseGroup],
        actor: PlayerRef,
        path: &str,
        source: &SourceSpan,
    ) -> Result<Located<Node>, LoweringError> {
        let observation = from.observation.as_ref().ok_or_else(|| unsupported(path))?;
        let group_count = u64::try_from(groups.len()).map_err(|_overflow| unsupported(path))?;
        if !matches!(group_count, 2 | 3)
            || choice.any
            || choice.may != Some(true)
            || choice.n.is_some()
            || choice.upto != Some(group_count)
            || choice.reveal == Some(true)
            || choice.order == Some(true)
            || choice.distinct_by.is_some()
            || choice.cost_sum_le.is_some()
            || from.who.is_some()
            || from.zone.is_some()
            || from.leader.is_some()
            || from.leaders.is_some()
            || from.leader_or
            || from
                .kind
                .as_ref()
                .is_some_and(|kind| groups.iter().any(|group| group.kind.as_ref() != Some(kind)))
            || from.keyword.is_some()
            || from.r#trait.is_some()
            || !from.traits_all.is_empty()
            || from.traits_any.is_some()
            || from.token.is_some()
            || !from.names.is_empty()
            || from.name.is_some()
            || from.name_contains.is_some()
            || from.has_evolve.is_some()
            || from.has_state_filter()
            || from.except.is_some()
            || from.other
            || from.has_numeric_filter()
            || from.class.is_some()
            || from.not_class.is_some()
            || self.targets.contains_key(&choice.bind)
            || self.searches.contains(&choice.bind)
            || self.observations.contains_key(&choice.bind)
        {
            return Err(unsupported(path));
        }
        let receipt = self
            .observations
            .get(observation)
            .filter(|receipt| {
                matches!(
                    receipt,
                    super::super::ObjectReceipt::Look { .. }
                        | super::super::ObjectReceipt::Reveal { .. }
                )
            })
            .ok_or_else(|| unsupported(path))?
            .clone();
        let cardinality = super::super::Cardinality {
            minimum: super::super::Nat(0),
            maximum: Some(super::super::Nat(group_count)),
        };
        let visibility = self.choice_visibility(Some(observation), false, &actor);
        let constraint = self.observed_groups(
            groups,
            &receipt,
            cardinality,
            from.not_same_name_as_this,
            path,
        )?;
        let binding = super::Binding::new(self.allocator.id(path)?, self.scope);
        if self.targets.insert(choice.bind.clone(), binding).is_some() {
            return Err(unsupported(path));
        }
        // Grouped picks stay members of their observation, like a single observed choice.
        self.observed_targets.insert(choice.bind.clone());
        self.field_entry.multi.insert(choice.bind.clone());
        let domain = super::super::ChoiceDomain::Objects {
            constraint,
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
                    actor,
                    domain,
                    kind: DecisionKind::ResolutionChoice {
                        may: Declinable::MayDecline,
                        cardinality,
                        precondition: None,
                    },
                    visibility,
                },
            },
        })
    }
}

const fn exact_own_cemetery_choice(choice: &ast::Choose, cardinality: super::Cardinality) -> bool {
    let ast::ChooseFrom::Query(from) = &choice.from else {
        return false;
    };
    matches!(
        choice.by,
        None | Some(ast::Participant::Side(ast::LeaderSide::Controller))
    ) && !matches!(choice.may, Some(true))
        && from.observation.is_none()
        && matches!(from.who, Some(ast::Side::Controller))
        && matches!(
            from.zone,
            Some(ast::TargetZone::Single(ast::TargetSingleZone::Cemetery))
        )
        && cardinality.minimum.0 > 0
        && matches!(cardinality.maximum, Some(maximum) if maximum.0 == cardinality.minimum.0)
}

#[derive(Default)]
pub(super) struct ObjectFilter<'query> {
    pub face_up: Option<bool>,
    pub boxed: Option<bool>,
    pub racing: Option<bool>,
    pub acted: Option<bool>,
    pub evolved: Option<bool>,
    pub entered_this_turn: bool,
    pub kind: Option<super::super::CardKind>,
    pub trait_id: Option<&'query str>,
    /// 「Aタイプを持たない」: the candidate lacks this trait.
    pub not_trait: Option<&'query str>,
    pub all_traits: &'query [String],
    /// 「AかB」: the candidate has at least one of these traits.
    pub any_traits: &'query [String],
    pub class: Option<&'query str>,
    pub not_class: Option<&'query str>,
    pub token: Option<bool>,
    pub name: Option<&'query str>,
    pub names: &'query [String],
    pub contains: Option<&'query str>,
    pub cost: [Option<u64>; 3],
    pub original_cost: [Option<u64>; 3],
    pub hp_at_most: Option<u64>,
    pub power_at_most: Option<u64>,
    pub power_at_least: Option<u64>,
    pub keyword: Option<super::super::KeywordId>,
    pub has_evolve: bool,
    /// これと同名を除く: the candidate shares no name with the ability holder.
    pub not_same_name_as_this: bool,
}

/// Every branch reads one private Look receipt, or the controller's own hand (「手札のAかB」, BP18-098).
fn private_union(any_of: &[ast::TargetQuery]) -> bool {
    let first = any_of
        .first()
        .and_then(|branch| branch.observation.as_ref());
    (first.is_some()
        && any_of
            .iter()
            .all(|branch| branch.observation.as_ref() == first))
        || [ast::Side::Controller, ast::Side::Opponent]
            .iter()
            .any(|holder| any_of.iter().all(|branch| own_hand_branch(branch, holder)))
}

/// Every branch reads one holder's own hand, so the chooser's authority covers the union.
fn own_hand_branch(branch: &ast::TargetQuery, holder: &ast::Side) -> bool {
    branch.observation.is_none()
        && branch.who.as_ref() == Some(holder)
        && matches!(
            branch.zone,
            Some(ast::TargetZone::Single(ast::TargetSingleZone::Hand))
        )
}

impl<'query> From<&'query ast::TargetQuery> for ObjectFilter<'query> {
    fn from(query: &'query ast::TargetQuery) -> Self {
        Self {
            face_up: query.face_up,
            kind: query.kind.as_ref().map(super::search::card_kind),
            keyword: query.keyword.map(|keyword| keyword.0),
            has_evolve: query.has_evolve.is_some(),
            trait_id: query.r#trait.as_deref(),
            all_traits: &query.traits_all,
            any_traits: query
                .traits_any
                .as_ref()
                .map_or(&[], |union| union.0.as_slice()),
            class: query.class.as_deref(),
            not_class: query.not_class.as_deref(),
            token: query.token,
            name: query.name.as_deref(),
            names: &query.names,
            contains: query.name_contains.as_deref(),
            cost: [
                query.cost_eq,
                query
                    .cost_le
                    .as_ref()
                    .and_then(ast::NumericArgument::literal),
                query.cost_ge,
            ],
            original_cost: [
                query.orig_cost_eq,
                query
                    .orig_cost_le
                    .as_ref()
                    .and_then(ast::NumericArgument::literal),
                query.orig_cost_ge,
            ],
            hp_at_most: query.hp_le,
            power_at_most: query.power_le,
            power_at_least: query.power_ge,
            acted: query.acted,
            evolved: query.evolved,
            entered_this_turn: query.entered_this_turn.is_some(),
            boxed: query.boxed,
            ..Self::default()
        }
    }
}

impl<'query> From<&'query ast::PublicQuery> for ObjectFilter<'query> {
    fn from(query: &'query ast::PublicQuery) -> Self {
        let power = query.r#where.map(|condition| condition.cmp);
        Self {
            power_at_most: power
                .filter(|cmp| cmp.op == ast::PowerBound::Le)
                .map(|cmp| cmp.right),
            power_at_least: power
                .filter(|cmp| cmp.op == ast::PowerBound::Ge)
                .map(|cmp| cmp.right),
            racing: query.racing,
            kind: query.kind.as_ref().map(super::search::card_kind),
            keyword: query.keyword.map(|keyword| keyword.0),
            trait_id: query.r#trait.as_deref(),
            any_traits: query
                .traits_any
                .as_ref()
                .map_or(&[], |union| union.0.as_slice()),
            all_traits: &query.traits_all,
            evolved: query.evolved,
            name: query.name.as_deref(),
            names: &query.names,
            contains: query.name_contains.as_deref(),
            class: query.class.as_deref(),
            not_class: query.not_class.as_deref(),
            token: query.token,
            cost: [query.cost_eq, query.cost_le, query.cost_ge],
            original_cost: [query.orig_cost_eq, query.orig_cost_le, query.orig_cost_ge],
            hp_at_most: query.hp_le,
            not_same_name_as_this: query.not_same_name_as_this,
            ..Self::default()
        }
    }
}

impl<'query> From<&'query ast::SelectedQuery> for ObjectFilter<'query> {
    fn from(query: &'query ast::SelectedQuery) -> Self {
        Self {
            kind: query.kind.as_ref().map(super::search::card_kind),
            trait_id: query.r#trait.as_deref(),
            all_traits: &query.traits_all,
            class: query.class.as_deref(),
            not_class: query.not_class.as_deref(),
            name: query.name.as_deref(),
            contains: query.name_contains.as_deref(),
            acted: query.acted,
            ..Self::default()
        }
    }
}

impl<'query> From<&'query ast::CountQuery> for ObjectFilter<'query> {
    fn from(query: &'query ast::CountQuery) -> Self {
        Self {
            face_up: query.face_up,
            class: query.class.as_deref(),
            not_class: query.not_class.as_deref(),
            kind: query.kind.map(ast::CountKind::kind),
            keyword: query.keyword.map(|keyword| keyword.0),
            trait_id: query.r#trait.as_deref(),
            any_traits: query
                .traits_any
                .as_ref()
                .map_or(&[], |union| union.0.as_slice()),
            name: query.name.as_deref(),
            contains: query.name_contains.as_deref(),
            token: query.token.or_else(|| query.not_token.map(|token| !token)),
            original_cost: [query.orig_cost_eq, None, query.orig_cost_ge],
            ..Self::default()
        }
    }
}

impl<'query> From<&'query ast::SearchQuery> for ObjectFilter<'query> {
    fn from(query: &'query ast::SearchQuery) -> Self {
        Self {
            kind: query.kind.as_ref().map(super::search::card_kind),
            has_evolve: query.has_evolve.is_some(),
            trait_id: query.r#trait.as_deref(),
            all_traits: &query.traits_all,
            class: query.class.as_deref(),
            contains: query.name_contains.as_deref(),
            name: query.name.as_deref(),
            cost: [
                query.cost_eq,
                query
                    .cost_le
                    .as_ref()
                    .and_then(ast::NumericArgument::literal),
                query.cost_ge,
            ],
            original_cost: [
                query.orig_cost_eq,
                query
                    .orig_cost_le
                    .as_ref()
                    .and_then(ast::NumericArgument::literal),
                query.orig_cost_ge,
            ],
            ..Self::default()
        }
    }
}

pub(super) fn field_filter(
    filter: &ObjectFilter<'_>,
    candidate: super::Binding<super::super::ObjectRef>,
) -> super::Expr<super::Bool> {
    let object = super::ObjectView::Current {
        object: Box::new(super::ObjectOperand::Bound { binding: candidate }),
    };
    view_filter(filter, object)
}

pub(super) fn view_filter(
    filter: &ObjectFilter<'_>,
    object: super::ObjectView,
) -> super::Expr<super::Bool> {
    use super::super::{CharacteristicView, Comparison, NumberRead};
    let base =
        super::search::view_filter(filter.kind, filter.trait_id, filter.name, object.clone());
    let mut predicates = Vec::new();
    if !matches!(base, super::Expr::Literal(super::Bool(true))) {
        predicates.push(base);
    }
    if !filter.names.is_empty() {
        predicates.push(super::Expr::Compute(Box::new(
            super::Predicate::NamesIntersect {
                left: super::Expr::Live(Box::new(super::super::NamesRead::Names {
                    object: object.clone(),
                })),
                right: super::Expr::Literal(
                    filter
                        .names
                        .iter()
                        .cloned()
                        .map(super::super::CardName)
                        .collect(),
                ),
            },
        )));
    }
    predicates.extend(trait_filters(filter, &object));
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
    predicates.extend(state_filters(filter, &object));
    predicates.extend(numeric_limits(filter, &object));
    for keyword in filter.keyword.into_iter().chain(
        filter
            .has_evolve
            .then_some(super::super::KeywordId::Evolution),
    ) {
        predicates.push(super::Expr::Compute(Box::new(
            super::Predicate::HasAbility {
                abilities: super::Expr::Live(Box::new(super::super::AbilitiesRead::Abilities {
                    object: object.clone(),
                    view: CharacteristicView::Effective,
                })),
                predicate: Box::new(super::super::AbilityPredicate {
                    kind: None,
                    keyword: Some(keyword),
                    except: vec![],
                    holder: None,
                    origin: None,
                }),
            },
        )));
    }
    if filter.not_same_name_as_this {
        predicates.push(holder_name_exclusion(object.clone()));
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

/// Every trait of `traits_all`, then 「AかB」 as one OR so a candidate still counts once.
fn trait_filters(
    filter: &ObjectFilter<'_>,
    object: &super::ObjectView,
) -> Vec<super::Expr<super::Bool>> {
    let read =
        |trait_id: &String| super::search::view_filter(None, Some(trait_id), None, object.clone());
    let mut predicates = filter.all_traits.iter().map(read).collect::<Vec<_>>();
    if let Some(trait_id) = filter.not_trait {
        predicates.push(super::Expr::Compute(Box::new(super::Predicate::Not {
            predicate: read(&trait_id.to_owned()),
        })));
    }
    if !filter.any_traits.is_empty() {
        predicates.push(super::Expr::Compute(Box::new(super::Predicate::Or {
            predicates: filter.any_traits.iter().map(read).collect(),
        })));
    }
    predicates
}

/// これと同名を除く: same shape as targets, reading the holder's current names.
fn holder_name_exclusion(object: super::ObjectView) -> super::Expr<super::Bool> {
    super::Expr::Compute(Box::new(super::Predicate::Not {
        predicate: super::Expr::Compute(Box::new(super::Predicate::NamesIntersect {
            left: super::Expr::Live(Box::new(super::super::NamesRead::Names { object })),
            right: super::Expr::Live(Box::new(super::super::NamesRead::Names {
                object: super::ObjectView::Current {
                    object: Box::new(super::ObjectOperand::SourceHolder {}),
                },
            })),
        })),
    }))
}

fn state_filters(
    filter: &ObjectFilter<'_>,
    object: &super::ObjectView,
) -> Vec<super::Expr<super::Bool>> {
    let mut predicates = Vec::new();
    for (expected, property) in [
        (filter.token, super::super::BooleanState::TokenStatus),
        (filter.face_up, super::super::BooleanState::FaceUp),
        (filter.boxed, super::super::BooleanState::Boxed),
        (filter.racing, super::super::BooleanState::Racing),
        (
            filter.entered_this_turn.then_some(true),
            super::super::BooleanState::EnteredThisTurn,
        ),
    ] {
        if let Some(expected) = expected {
            let predicate = super::Expr::Live(Box::new(super::super::BoolRead::State {
                object: object.clone(),
                property,
            }));
            predicates.push(if expected {
                predicate
            } else {
                super::Expr::Compute(Box::new(super::Predicate::Not { predicate }))
            });
        }
    }
    if let Some(acted) = filter.acted {
        predicates.push(super::Expr::Compute(Box::new(
            super::Predicate::PostureEquals {
                left: super::Expr::Live(Box::new(super::super::PostureRead::Posture {
                    object: object.clone(),
                })),
                right: super::Expr::Literal(if acted {
                    super::super::Posture::Acted
                } else {
                    super::super::Posture::Standing
                }),
            },
        )));
    }
    if let Some(evolved) = filter.evolved {
        // Each state is a positive follower read, so amulets match neither side of the filter.
        let state = |expected| {
            super::Expr::Compute(Box::new(super::Predicate::EvolutionEquals {
                left: super::Expr::Live(Box::new(super::super::EvolutionRead::Evolution {
                    object: object.clone(),
                })),
                right: super::Expr::Literal(expected),
            }))
        };
        predicates.push(if evolved {
            super::Expr::Compute(Box::new(super::Predicate::Or {
                predicates: vec![
                    state(super::super::EvolutionState::Evolved),
                    state(super::super::EvolutionState::SuperEvolved),
                ],
            }))
        } else {
            state(super::super::EvolutionState::Base)
        });
    }
    predicates
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
    for (limit, comparison) in [
        (filter.power_at_most, Comparison::LessEqual),
        (filter.power_at_least, Comparison::GreaterEqual),
    ] {
        if let Some(limit) = limit {
            predicates.push(number_limit(
                NumberRead::Stats {
                    object: object.clone(),
                    view: CharacteristicView::Effective,
                    property: super::super::Stat::Attack,
                },
                comparison,
                limit,
            ));
        }
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

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Public numeric filters share body definitions and checked expression lowering."
)]
impl BodyContext<'_> {
    pub(super) fn numeric_limits(
        &mut self,
        predicate: super::Expr<super::Bool>,
        candidate: super::Binding<super::super::ObjectRef>,
        cost: Option<&ast::NumericArgument>,
        original_cost: Option<&ast::NumericArgument>,
        path: &str,
    ) -> Result<super::Expr<super::Bool>, LoweringError> {
        use super::super::{
            CharacteristicView, Comparison, IntOperation, NumberRead, ObjectOperand, ObjectView,
            Predicate,
        };
        use super::Expr;
        let mut predicates = vec![predicate];
        for (argument, view) in [
            (cost, CharacteristicView::Effective),
            (original_cost, CharacteristicView::Original),
        ] {
            if let Some(ast::NumericArgument::Expression(value)) = argument {
                let number: ast::Number = super::decode(value.as_ref().clone(), path)?;
                let right = self.number(&number, path, &mut super::BTreeSet::new())?;
                if !public_numbers::check(&right) {
                    return Err(unsupported(path));
                }
                predicates.push(Expr::Compute(Box::new(Predicate::CompareInt {
                    left: Expr::Compute(Box::new(IntOperation::InformationOrZero {
                        information: NumberRead::Cost {
                            object: ObjectView::Current {
                                object: Box::new(ObjectOperand::Bound { binding: candidate }),
                            },
                            view,
                        },
                    })),
                    comparison: Comparison::LessEqual,
                    right,
                })));
            }
        }
        if predicates.len() == 1 {
            Ok(predicates.remove(0))
        } else {
            Ok(Expr::Compute(Box::new(Predicate::And { predicates })))
        }
    }
}
