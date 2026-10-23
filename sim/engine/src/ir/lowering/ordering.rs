use super::super::{
    DecisionId, MemberProjection, MoveDestination, MoveReason, MoveSpec, MovementReceipt,
    ObjectReceipt, OwnerZone, Placement, Position, ReceiptReference, Visibility,
};
use super::{
    Action, BodyContext, Cardinality, LoweringError, Nat, PlayerRef, PlayerRelation, PlayerSet,
    ReceiptId, Selector, TextQuantifier, ast, unsupported,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Observation receipts are only exposed to the private movement spine."
)]
impl BodyContext<'_> {
    pub(super) fn remember(
        &mut self,
        step: &ast::Step,
        receipt: ReceiptId,
        path: &str,
    ) -> Result<(), LoweringError> {
        let saved = match step {
            ast::Step::Look(value) => Some((
                &value.bind,
                ObjectReceipt::Look {
                    receipt: ReceiptReference::new(receipt),
                },
            )),
            ast::Step::Move(value) => value.result.as_ref().map(|name| {
                (
                    name,
                    ObjectReceipt::Movement {
                        receipt: match value.to {
                            ast::ObservedDestination::Deck => MovementReceipt::Order {
                                receipt: ReceiptReference::new(receipt),
                            },
                            ast::ObservedDestination::Ex => MovementReceipt::Move {
                                receipt: ReceiptReference::new(receipt),
                            },
                        },
                    },
                )
            }),
            ast::Step::Search(value) => {
                if value.result.as_ref().is_some_and(|name| {
                    self.observations.contains_key(name) || self.targets.contains_key(name)
                }) {
                    return Err(unsupported(path));
                }
                return self.search_result(step, receipt, path);
            }
            ast::Step::If(_)
            | ast::Step::Choice(_)
            | ast::Step::Draw(_)
            | ast::Step::Damage(_)
            | ast::Step::Destroy(_)
            | ast::Step::Buff(_)
            | ast::Step::SetStat(_)
            | ast::Step::Apply(_) => None,
        };
        if let Some((name, saved_receipt)) = saved
            && (self.targets.contains_key(name)
                || self.searches.contains_key(name)
                || self
                    .observations
                    .insert(name.clone(), saved_receipt)
                    .is_some())
        {
            return Err(unsupported(path));
        }
        Ok(())
    }

    fn observed(
        &self,
        value: &ast::Observed,
        movement: bool,
        path: &str,
    ) -> Result<Selector, LoweringError> {
        let (name, before) = match &value.r#in {
            ast::Observation::Binding(name) => (name, false),
            ast::Observation::Before {
                objects_of,
                view: ast::Before::Before,
            } => (objects_of, true),
        };
        let receipt = self
            .observations
            .get(name)
            .ok_or_else(|| unsupported(path))?;
        if (!before || movement) && !matches!(receipt, ObjectReceipt::Look { .. }) {
            return Err(unsupported(path));
        }
        let input = Selector::ReceiptMembers {
            receipt: receipt.clone(),
            projection: MemberProjection::Before,
        };
        if let Some(except) = &value.except {
            Ok(Selector::Difference {
                input: Box::new(input),
                except: Box::new(self.observed(except, movement, path)?),
            })
        } else {
            Ok(input)
        }
    }

    pub(super) fn observed_move(
        &mut self,
        value: &ast::Move,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let cards = self.observed(
            &value.r#ref,
            matches!(value.to, ast::ObservedDestination::Ex),
            path,
        )?;
        match value.to {
            ast::ObservedDestination::Ex if value.position.is_none() && value.order.is_none() => {
                Ok(Action::Move {
                    objects: cards,
                    movement: MoveSpec {
                        to: MoveDestination::Shared {
                            zone: super::super::ZoneRef {
                                player: PlayerRef::Controller {},
                                kind: super::ZoneKind::Ex,
                            },
                        },
                        initial_posture: None,
                        entry_effects: vec![],
                        position: None,
                        order: None,
                    },
                    reason: MoveReason::Put,
                })
            }
            ast::ObservedDestination::Deck if value.order.is_some() => {
                let position = match value.position {
                    Some(ast::Placement::Top) => Position::Top {},
                    Some(ast::Placement::Bottom) => Position::Bottom {},
                    None => return Err(unsupported(path)),
                };
                Ok(Action::Order {
                    decision: DecisionId(self.card_allocator.id(path)?),
                    cards: cards.clone(),
                    destinations: vec![Placement {
                        cards,
                        zone: MoveDestination::Owners {
                            zone: OwnerZone::Deck,
                        },
                        position,
                    }],
                    chooser: PlayerRef::Controller {},
                    visibility: Visibility::Only {
                        players: PlayerSet {
                            relation: PlayerRelation::SelfPlayer,
                            relative_to: PlayerRef::Controller {},
                            cardinality: Cardinality {
                                minimum: Nat(1),
                                maximum: Some(Nat(1)),
                            },
                            text_quantifier: TextQuantifier::Implicit,
                        },
                    },
                })
            }
            ast::ObservedDestination::Deck | ast::ObservedDestination::Ex => Err(unsupported(path)),
        }
    }
}
