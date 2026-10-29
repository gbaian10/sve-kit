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
                        group.kind.as_ref(),
                        None,
                        None,
                        None,
                        group.orig_cost_le,
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
            || self.targets.contains_key(&choice.bind)
            || self.searches.contains_key(&choice.bind)
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

pub(super) fn field_filter(
    kind: Option<&ast::SearchKind>,
    trait_id: Option<&str>,
    name: Option<&str>,
    contains: Option<&str>,
    cost_at_most: Option<u64>,
    candidate: super::Binding<super::super::ObjectRef>,
) -> super::Expr<super::Bool> {
    let base = super::search::object_filter(kind, trait_id, name, candidate);
    let mut predicates = Vec::new();
    if !matches!(base, super::Expr::Literal(super::Bool(true))) {
        predicates.push(base);
    }
    if let Some(contains) = contains {
        predicates.push(super::Expr::Compute(Box::new(
            super::Predicate::NameContains {
                object: super::ObjectView::Current {
                    object: Box::new(super::ObjectOperand::Bound { binding: candidate }),
                },
                name: super::Expr::Literal(super::super::CardName(contains.to_owned())),
            },
        )));
    }
    if let Some(limit) = cost_at_most {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        predicates.push(super::Expr::Compute(Box::new(
            super::Predicate::CompareInt {
                left: super::Expr::Compute(Box::new(super::IntOperation::InformationOrZero {
                    information: super::super::NumberRead::Cost {
                        object: super::ObjectView::Current {
                            object: Box::new(super::ObjectOperand::Bound { binding: candidate }),
                        },
                        view: super::super::CharacteristicView::Original,
                    },
                })),
                comparison: super::super::Comparison::LessEqual,
                right: super::Expr::Literal(super::Int(limit)),
            },
        )));
    }
    match predicates.as_mut_slice() {
        [] => super::Expr::Literal(super::Bool(true)),
        [_] => predicates.swap_remove(0),
        _ => super::Expr::Compute(Box::new(super::Predicate::And { predicates })),
    }
}
