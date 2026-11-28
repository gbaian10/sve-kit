use super::{Action, BodyContext, LoweringError, ast, unsupported};
use crate::ir::{
    AbilitySource, CardKind, Cardinality, Expr, Instruction, InstructionContext, InstructionId,
    Located, LookPurpose, MemberProjection, Nat, Node, ObjectReceipt, OwnerZone, PlayerRef,
    PlayerRelation, PlayerSet, RandomnessPolicy, ReceiptId, ReceiptLocation, ReceiptReference,
    Selector, SourceSpan, StepwiseMode, TextQuantifier, Visibility, ZoneKind, ZoneRef,
};

impl BodyContext<'_> {
    pub(super) fn random_reveal(
        &mut self,
        step: &ast::Step,
        path: &str,
        source: &SourceSpan,
        origin: &AbilitySource,
    ) -> Result<Vec<Located<Node>>, LoweringError> {
        let ast::Step::Reveal(value) = step else {
            return Err(unsupported(path));
        };
        let (Some(ast::RevealRef::WholeHand(hand)), Some(random)) = (&value.r#ref, &value.random)
        else {
            return Err(unsupported(path));
        };
        if value.top.is_some() || matches!(hand.who, ast::Side::All) {
            return Err(unsupported(path));
        }
        let (relation, relative_to) = self.side_set(&hand.who, path)?;
        let population = Selector::Zone {
            players: PlayerSet {
                relation,
                relative_to,
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
                text_quantifier: TextQuantifier::Implicit,
            },
            zone: ZoneKind::Hand,
            view: super::super::SelectionView::Current {},
            binding: super::Binding::new(self.allocator.id(path)?, self.scope),
            predicate: Box::new(Expr::Literal(super::super::Bool(true))),
        };
        let sample = ReceiptId(self.card_allocator.id(path)?);
        let reveal = ReceiptId(self.card_allocator.id(path)?);
        let action = Action::Reveal {
            objects: Selector::ReceiptMembers {
                receipt: ObjectReceipt::Random {
                    receipt: ReceiptReference::new(sample),
                },
                projection: MemberProjection::Before,
            },
        };
        self.remember(step, reveal, &action, path)?;
        let visibility = Visibility::Only {
            players: PlayerSet {
                relation: PlayerRelation::SelfPlayer,
                relative_to: PlayerRef::Controller {},
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
                text_quantifier: TextQuantifier::Implicit,
            },
        };
        Ok([
            (
                sample,
                Action::Random {
                    population,
                    count: Nat(random.n),
                    rng: RandomnessPolicy::FreshRequest,
                    visibility,
                },
            ),
            (reveal, action),
        ]
        .into_iter()
        .map(|(receipt, action)| {
            let mut located = source.clone();
            path.clone_into(&mut located.author_path);
            Located {
                source: located,
                node: Node::Instruction {
                    instruction: Box::new(Instruction {
                        id: InstructionId(receipt.0),
                        receipt,
                        actor: PlayerRef::Controller {},
                        source: origin.clone(),
                        precondition: None,
                        context: InstructionContext::Effect,
                        action,
                    }),
                },
            }
        })
        .collect())
    }

    pub(super) fn public_observation(
        &self,
        name: &str,
        path: &str,
    ) -> Result<Selector, LoweringError> {
        let Some(receipt @ ObjectReceipt::Look { receipt: reference }) =
            self.observations.get(name)
        else {
            return Err(unsupported(path));
        };
        if !matches!(reference.location(), ReceiptLocation::Local { id } if self.receipt_reads.published_looks.contains(id))
        {
            return Err(unsupported(path));
        }
        Ok(Selector::ReceiptMembers {
            receipt: receipt.clone(),
            projection: MemberProjection::Before,
        })
    }

    pub(super) fn reveal_top(
        &mut self,
        value: &ast::Reveal,
        path: &str,
        source: &SourceSpan,
        origin: &AbilitySource,
    ) -> Result<Vec<Located<Node>>, LoweringError> {
        let top = value.top.as_ref().ok_or_else(|| unsupported(path))?;
        if self.scope != self.public_reveal_scope
            || value.r#ref.is_some()
            || value.random.is_some()
            || top.n == 0
            || value.bind.is_empty()
            || !matches!(top.who, ast::DeckHolder::Side(ast::LeaderSide::Controller))
            || self.observations.contains_key(&value.bind)
            || self.targets.contains_key(&value.bind)
            || self.searches.contains(&value.bind)
        {
            return Err(unsupported(path));
        }
        let look = ReceiptId(self.card_allocator.id(path)?);
        self.observations.insert(
            value.bind.clone(),
            ObjectReceipt::Look {
                receipt: ReceiptReference::new(look),
            },
        );
        self.receipt_reads.published_looks.insert(look);
        if self.scope != self.resolution_scope {
            self.branch_reveals.push(value.bind.clone());
        }
        let objects = self.public_observation(&value.bind, path)?;
        let reveal = ReceiptId(self.card_allocator.id(path)?);
        [
            (
                look,
                Action::Look {
                    viewer: PlayerRef::Controller {},
                    zone: ZoneRef {
                        player: PlayerRef::Controller {},
                        kind: ZoneKind::Deck,
                    },
                    count: Expr::Literal(Nat(top.n)),
                    mode: StepwiseMode::Exact,
                    purpose: LookPurpose::PrepareReveal,
                },
            ),
            (reveal, Action::Reveal { objects }),
        ]
        .into_iter()
        .map(|(receipt, action)| {
            let mut located = source.clone();
            path.clone_into(&mut located.author_path);
            Ok(Located {
                source: located,
                node: Node::Instruction {
                    instruction: Box::new(Instruction {
                        id: InstructionId(receipt.0),
                        receipt,
                        actor: PlayerRef::Controller {},
                        source: origin.clone(),
                        precondition: None,
                        context: InstructionContext::Effect,
                        action,
                    }),
                },
            })
        })
        .collect()
    }

    /// A branch-local reveal stays readable only inside the branch that published it.
    pub(super) fn forget_branch_reveals(&mut self, outer: usize) {
        for name in self.branch_reveals.split_off(outer) {
            if let Some(ObjectReceipt::Look { receipt }) = self.observations.remove(&name)
                && let ReceiptLocation::Local { id } = receipt.location()
            {
                self.receipt_reads.published_looks.remove(id);
            }
        }
    }

    pub(super) fn public_reveal_move(
        &mut self,
        value: &ast::Move,
        path: &str,
    ) -> Result<Option<Action>, LoweringError> {
        if let Some(ast::MoveObjects::FilteredObserved(observed)) = &value.r#ref {
            let Some(receipt @ ObjectReceipt::Reveal { .. }) =
                self.observations.get(&observed.r#in)
            else {
                return Err(unsupported(path));
            };
            if !matches!(value.to, ast::Destination::Field)
                || value.top.is_some()
                || value.position.is_some()
                || value.order.is_some()
                || value.acted.is_some()
                || value.entry_effects.is_some()
                || value.result.as_ref().is_none_or(String::is_empty)
            {
                return Err(unsupported(path));
            }
            let input = Selector::ReceiptMembers {
                receipt: receipt.clone(),
                projection: MemberProjection::Before,
            };
            let binding = super::Binding::new(self.allocator.id(path)?, self.scope);
            let filter = super::selection::ObjectFilter {
                kind: Some(match observed.kind {
                    ast::FieldKind::Follower => CardKind::Follower,
                    ast::FieldKind::Amulet => CardKind::Amulet,
                }),
                ..super::selection::ObjectFilter::default()
            };
            return Ok(Some(super::ordering::shared_put(
                Selector::Filter {
                    input: Box::new(input),
                    binding,
                    predicate: Box::new(super::selection::field_filter(&filter, binding)),
                },
                ZoneKind::Field,
            )));
        }
        let Some(ast::MoveObjects::Observed(observed)) = &value.r#ref else {
            return Ok(None);
        };
        let ast::Observation::Binding(name) = &observed.r#in else {
            return Ok(None);
        };
        if !self.observations.get(name).is_some_and(|r| matches!(r, ObjectReceipt::Look { receipt } if matches!(receipt.location(), ReceiptLocation::Local { id } if self.receipt_reads.published_looks.contains(id)))) { return Ok(None); }
        if observed.except.is_some() {
            return Ok(None);
        }
        if value.top.is_some()
            || value.position.is_some()
            || value.order.is_some()
            || value.acted.is_some()
        {
            return Err(unsupported(path));
        }
        let objects = self.public_observation(name, path)?;
        Ok(Some(match value.to {
            ast::Destination::Hand => super::ordering::owner_put(objects, OwnerZone::Hand),
            ast::Destination::Ex => super::ordering::shared_put(objects, ZoneKind::Ex),
            ast::Destination::Field if value.result.is_some() => {
                super::ordering::shared_put(objects, ZoneKind::Field)
            }
            ast::Destination::Field | ast::Destination::Deck | ast::Destination::Cemetery => {
                return Err(unsupported(path));
            }
        }))
    }
}
