use super::super::{
    DecisionId, MemberProjection, MoveDestination, MoveReason, MoveSpec, MovementEvent,
    MovementReceipt, ObjectOperand, ObjectReceipt, OwnerZone, Placement, Position,
    ReceiptReference, Visibility,
};
use super::{
    Action, BodyContext, Cardinality, LoweringError, Nat, PlayerRef, PlayerRelation, PlayerSet,
    ReceiptId, Selector, TextQuantifier, ZoneKind, ast, unsupported,
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
                            ast::Destination::Deck => MovementReceipt::Order {
                                receipt: ReceiptReference::new(receipt),
                            },
                            ast::Destination::Field
                            | ast::Destination::Ex
                            | ast::Destination::Hand
                            | ast::Destination::Cemetery => MovementReceipt::Move {
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
            ast::Step::Reveal(_)
            | ast::Step::If(_)
            | ast::Step::Choose(_)
            | ast::Step::Choice(_)
            | ast::Step::Draw(_)
            | ast::Step::Pp(_)
            | ast::Step::Discard(_)
            | ast::Step::PpMax(_)
            | ast::Step::Ep(_)
            | ast::Step::Damage(_)
            | ast::Step::Life(_)
            | ast::Step::SetLife(_)
            | ast::Step::Destroy(_)
            | ast::Step::Banish(_)
            | ast::Step::Act(_)
            | ast::Step::Stand(_)
            | ast::Step::Buff(_)
            | ast::Step::Grant(_)
            | ast::Step::SetStat(_)
            | ast::Step::Apply(_)
            | ast::Step::Create(_) => None,
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

    fn lastword_return(&self, objects: &ast::Objects, path: &str) -> Result<Action, LoweringError> {
        let (ast::Objects::This(_), Some(event)) = (objects, self.leave_event) else {
            return Err(unsupported(path));
        };
        Ok(Action::Move {
            objects: Selector::Objects {
                objects: vec![ObjectOperand::FollowEventMove {
                    event: MovementEvent::LeaveField { event },
                }],
            },
            movement: MoveSpec {
                to: MoveDestination::Shared {
                    zone: super::super::ZoneRef {
                        player: PlayerRef::Controller {},
                        kind: ZoneKind::Field,
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

    pub(super) fn observed_move(
        &mut self,
        value: &ast::Move,
        path: &str,
    ) -> Result<Action, LoweringError> {
        if let Some(top) = &value.top {
            return top_to_cemetery(value, top, path);
        }
        if let Some(ast::MoveObjects::Field(objects)) = &value.r#ref {
            if value.position.is_some() || value.order.is_some() || value.result.is_some() {
                return Err(unsupported(path));
            }
            if matches!(value.to, ast::Destination::Field) {
                return self.lastword_return(objects, path);
            }
            let zone = match value.to {
                ast::Destination::Hand => OwnerZone::Hand,
                ast::Destination::Cemetery => OwnerZone::Cemetery,
                ast::Destination::Ex => OwnerZone::Ex,
                ast::Destination::Field | ast::Destination::Deck => return Err(unsupported(path)),
            };
            let objects = match (objects, self.leave_event, zone) {
                (ast::Objects::This(_), Some(event), OwnerZone::Ex) => Selector::Objects {
                    objects: vec![ObjectOperand::FollowEventMove {
                        event: MovementEvent::LeaveField { event },
                    }],
                },
                _ => self.objects(objects, path)?,
            };
            return Ok(Action::Move {
                objects,
                movement: MoveSpec {
                    to: MoveDestination::Owners { zone },
                    initial_posture: None,
                    entry_effects: vec![],
                    position: None,
                    order: None,
                },
                reason: MoveReason::Put,
            });
        }
        let Some(ast::MoveObjects::Observed(observed)) = &value.r#ref else {
            return Err(unsupported(path));
        };
        let cards = self.observed(observed, matches!(value.to, ast::Destination::Ex), path)?;
        match value.to {
            ast::Destination::Ex if value.position.is_none() && value.order.is_none() => {
                Ok(Action::Move {
                    objects: cards,
                    movement: MoveSpec {
                        to: MoveDestination::Shared {
                            zone: super::super::ZoneRef {
                                player: PlayerRef::Controller {},
                                kind: ZoneKind::Ex,
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
            ast::Destination::Deck if value.order.is_some() => {
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
            ast::Destination::Field
            | ast::Destination::Deck
            | ast::Destination::Ex
            | ast::Destination::Hand
            | ast::Destination::Cemetery => Err(unsupported(path)),
        }
    }
    pub(super) fn reveal(
        &mut self,
        value: &ast::Reveal,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let ast::HandZone::Hand = value.r#ref.zone;
        let relation = match value.r#ref.who {
            ast::Side::Controller => PlayerRelation::SelfPlayer,
            ast::Side::Opponent => PlayerRelation::Opponents,
            ast::Side::All => return Err(unsupported(path)),
        };
        Ok(Action::Reveal {
            objects: Selector::Zone {
                players: PlayerSet {
                    relation,
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(1),
                        maximum: Some(Nat(1)),
                    },
                    text_quantifier: TextQuantifier::Implicit,
                },
                zone: ZoneKind::Hand,
                view: super::SelectionView::Current {},
                binding: super::Binding::new(self.allocator.id(path)?, self.scope),
                predicate: Box::new(super::Expr::Literal(super::super::Bool(true))),
            },
        })
    }
}

fn top_to_cemetery(
    value: &ast::Move,
    top: &ast::DeckTop,
    path: &str,
) -> Result<Action, LoweringError> {
    if value.r#ref.is_some()
        || top.n == 0
        || !matches!(top.who, ast::SelfPlayer::Controller)
        || !matches!(value.to, ast::Destination::Cemetery)
        || value.position.is_some()
        || value.order.is_some()
        || value.result.is_some()
    {
        return Err(unsupported(path));
    }
    Ok(Action::Move {
        objects: Selector::Ordered {
            zone: super::super::ZoneRef {
                player: PlayerRef::Controller {},
                kind: ZoneKind::Deck,
            },
            start: Position::Top {},
            count: super::Expr::Literal(Nat(top.n)),
            direction: super::super::SliceDirection::Forward,
        },
        movement: MoveSpec {
            to: MoveDestination::Owners {
                zone: OwnerZone::Cemetery,
            },
            initial_posture: None,
            entry_effects: vec![],
            position: None,
            order: None,
        },
        reason: MoveReason::Put,
    })
}
