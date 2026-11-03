use super::super::{
    DecisionId, MemberProjection, MoveDestination, MoveReason, MoveSpec, MovementEvent,
    MovementReceipt, ObjectOperand, ObjectReceipt, OwnerZone, Placement, Position, PositiveNat,
    ReceiptReference, Visibility,
};
use super::{
    Action, BodyContext, Cardinality, LoweringError, Nat, PlayerRef, PlayerRelation, PlayerSet,
    ReceiptId, Selector, TextQuantifier, ZoneKind, ast, unsupported,
};
use core::num::NonZeroU64;

#[derive(Clone, Copy)]
enum ObservationUse {
    Order,
    OrderRemainder,
    Move,
    Remainder,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Observation receipts are only exposed to the private movement spine."
)]
impl BodyContext<'_> {
    #[expect(
        clippy::too_many_lines,
        reason = "The exhaustive author-step match keeps observation receipts closed."
    )]
    pub(super) fn remember(
        &mut self,
        step: &ast::Step,
        receipt: ReceiptId,
        path: &str,
    ) -> Result<(), LoweringError> {
        if let ast::Step::Move(value) = step
            && matches!(value.to, ast::Destination::Ex)
            && let Some(name) = &value.result
        {
            self.ex_outputs.insert(
                name.clone(),
                Selector::MoveAfter {
                    receipt: MovementReceipt::Move {
                        receipt: ReceiptReference::new(receipt),
                    },
                    permission: super::super::FollowPermission::SameEffectExplicitReference {},
                },
            );
        }
        if let ast::Step::Move(value) = step
            && value.result.is_some()
            && (value.top.is_some()
                || matches!(&value.r#ref, Some(ast::MoveObjects::Field(ast::Objects::Bound(name)))
                    if self.observed_targets.contains(name)))
        {
            self.receipt_reads.deck_moves.insert(receipt);
            if matches!(value.to, ast::Destination::Cemetery)
                && matches!(
                    value.top.as_ref().map(|top| &top.who),
                    Some(ast::DeckHolder::Side(ast::LeaderSide::Controller))
                )
            {
                self.receipt_reads.deck_cemetery.insert(receipt);
            }
        }
        let saved = match step {
            ast::Step::Destroy(ast::ObjectAction::Long {
                result: Some(name), ..
            }) => Some((
                name,
                ObjectReceipt::Movement {
                    receipt: MovementReceipt::Destroy {
                        receipt: ReceiptReference::new(receipt),
                    },
                },
            )),
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
            ast::Step::StackPlus(_)
            | ast::Step::Counter(_)
            | ast::Step::Reveal(_)
            | ast::Step::Optional(_)
            | ast::Step::Pay(_)
            | ast::Step::EachPlayer(_)
            | ast::Step::GainDrive(_)
            | ast::Step::Race(_)
            | ast::Step::Evolve(_)
            | ast::Step::If(_)
            | ast::Step::Choose(_)
            | ast::Step::Choice(_)
            | ast::Step::Either(_)
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
            | ast::Step::Transform(_)
            | ast::Step::Act(_)
            | ast::Step::Stand(_)
            | ast::Step::Buff(_)
            | ast::Step::Grant(_)
            | ast::Step::SetStat(_)
            | ast::Step::Apply(_)
            | ast::Step::PlayerEffect(_)
            | ast::Step::Create(_)
            | ast::Step::Equip(_) => None,
        };
        if let Some((name, saved_receipt)) = saved
            && (self.targets.contains_key(name)
                || self.searches.contains(name)
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
        usage: ObservationUse,
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
        let look = matches!(receipt, ObjectReceipt::Look { .. });
        if matches!(
            receipt,
            ObjectReceipt::Movement {
                receipt: MovementReceipt::Order { .. }
            }
        ) && !matches!(
            usage,
            ObservationUse::OrderRemainder | ObservationUse::Remainder
        ) {
            return Err(unsupported(path));
        }
        if !look
            && (!before
                || matches!(usage, ObservationUse::Move)
                || matches!(usage, ObservationUse::Remainder)
                    && !matches!(
                        receipt,
                        ObjectReceipt::Movement {
                            receipt: MovementReceipt::Move { .. } | MovementReceipt::Order { .. }
                        }
                    ))
        {
            return Err(unsupported(path));
        }
        let input = Selector::ReceiptMembers {
            receipt: receipt.clone(),
            projection: MemberProjection::Before,
        };
        if let Some(except) = &value.except {
            Ok(Selector::Difference {
                input: Box::new(input),
                except: Box::new(self.observed(
                    except,
                    if matches!(
                        usage,
                        ObservationUse::Order | ObservationUse::OrderRemainder
                    ) {
                        ObservationUse::OrderRemainder
                    } else {
                        ObservationUse::Remainder
                    },
                    path,
                )?),
            })
        } else {
            Ok(input)
        }
    }

    fn self_return(&self, objects: &ast::Objects, path: &str) -> Result<Action, LoweringError> {
        if !matches!(objects, ast::Objects::This(_))
            || (self.leave_event.is_some() && !self.departure_to_cemetery)
        {
            return Err(unsupported(path));
        }
        let object = match self.leave_event {
            Some(event) => ObjectOperand::FollowEventMove {
                event: MovementEvent::LeaveField { event },
            },
            None if self.source_can_enter_field => ObjectOperand::SourceHolder {},
            None => return Err(unsupported(path)),
        };
        Ok(Action::Move {
            objects: Selector::Objects {
                objects: vec![object],
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

    /// Only a chosen target may enter the field, so the movement stays tied to its decision.
    fn field_entry_move(&mut self, value: &ast::Move, path: &str) -> Result<Action, LoweringError> {
        let Some(ast::MoveObjects::Field(objects @ ast::Objects::Bound(name))) = &value.r#ref
        else {
            return Err(unsupported(path));
        };
        if !self.targets.contains_key(name)
            || (self.field_entry.multi.contains(name)
                && self.field_entry.adjacent.as_deref() != Some(name.as_str()))
            || value.top.is_some()
            || value.position.is_some()
            || value.order.is_some()
            || value.result.is_none()
        {
            return Err(unsupported(path));
        }
        Ok(shared_put(self.objects(objects, path)?, ZoneKind::Field))
    }

    pub(super) fn banish(
        &mut self,
        value: &ast::BanishAction,
        path: &str,
    ) -> Result<Action, LoweringError> {
        Ok(Action::Banish {
            objects: match value {
                ast::BanishAction::Top { top } if own_deck(top) => self.deck_slice(top, path)?,
                ast::BanishAction::Top { .. } => return Err(unsupported(path)),
                ast::BanishAction::Objects(objects) => self.action_objects(objects, path)?,
            },
        })
    }

    pub(super) fn observed_move(
        &mut self,
        value: &ast::Move,
        path: &str,
    ) -> Result<Action, LoweringError> {
        if matches!(value.to, ast::Destination::Field) {
            if let Some(ast::MoveObjects::Field(objects @ ast::Objects::This(_))) = &value.r#ref {
                return self.self_return(objects, path);
            }
            return self.field_entry_move(value, path);
        }
        if let Some(top) = &value.top {
            return self.top_move(value, top, path);
        }
        if let Some(ast::MoveObjects::Field(objects)) = &value.r#ref {
            if matches!(objects, ast::Objects::Bound(name) if self.hand_targets.contains(name))
                && matches!(
                    value.to,
                    ast::Destination::Hand | ast::Destination::Cemetery
                )
            {
                return Err(unsupported(path));
            }
            if matches!(value.to, ast::Destination::Deck) {
                return self.selected_deck_move(value, objects, path);
            }
            if value.position.is_some()
                || value.order.is_some()
                || !self.supports_move_result(value, objects)
            {
                return Err(unsupported(path));
            }
            let zone = match value.to {
                ast::Destination::Hand => OwnerZone::Hand,
                ast::Destination::Cemetery => OwnerZone::Cemetery,
                ast::Destination::Ex => OwnerZone::Ex,
                ast::Destination::Deck | ast::Destination::Field => {
                    return Err(unsupported(path));
                }
            };
            if matches!(objects, ast::Objects::This(_))
                && zone == OwnerZone::Ex
                && self.leave_event.is_some()
                && !self.departure_to_cemetery
            {
                return Err(unsupported(path));
            }
            let objects = match (objects, self.leave_event, zone) {
                (ast::Objects::This(_), Some(event), OwnerZone::Ex) => Selector::Objects {
                    objects: vec![ObjectOperand::FollowEventMove {
                        event: MovementEvent::LeaveField { event },
                    }],
                },
                _ => self.objects(objects, path)?,
            };
            return Ok(owner_put(objects, zone));
        }
        let Some(ast::MoveObjects::Observed(observed)) = &value.r#ref else {
            return Err(unsupported(path));
        };
        let cards = self.observed(
            observed,
            if matches!(value.to, ast::Destination::Deck) {
                ObservationUse::Order
            } else {
                ObservationUse::Move
            },
            path,
        )?;
        match value.to {
            ast::Destination::Ex if value.position.is_none() && value.order.is_none() => {
                Ok(shared_put(cards, ZoneKind::Ex))
            }
            ast::Destination::Cemetery if value.position.is_none() && value.order.is_none() => {
                Ok(owner_put(cards, OwnerZone::Cemetery))
            }
            ast::Destination::Deck if value.order.is_some() => {
                self.observed_order(cards, value.position.as_ref(), path)
            }
            ast::Destination::Field
            | ast::Destination::Deck
            | ast::Destination::Ex
            | ast::Destination::Hand
            | ast::Destination::Cemetery => Err(unsupported(path)),
        }
    }
    fn selected_deck_move(
        &mut self,
        value: &ast::Move,
        objects: &ast::Objects,
        path: &str,
    ) -> Result<Action, LoweringError> {
        if matches!(objects, ast::Objects::Bound(name) if self.observed_targets.contains(name)) {
            // The Order receipt lets a later remainder subtract the reordered cards.
            if value.order.is_none() {
                return Err(unsupported(path));
            }
            let cards = self.objects(objects, path)?;
            return self.observed_order(cards, value.position.as_ref(), path);
        }
        if value.order.is_some() || value.result.is_some() {
            return Err(unsupported(path));
        }
        let objects = if let (ast::Objects::This(_), Some(event)) = (objects, self.leave_event) {
            if !self.departure_to_cemetery {
                return Err(unsupported(path));
            }
            Selector::Objects {
                objects: vec![ObjectOperand::FollowEventMove {
                    event: MovementEvent::LeaveField { event },
                }],
            }
        } else {
            self.objects(objects, path)?
        };
        Ok(Action::Move {
            objects,
            movement: MoveSpec {
                to: MoveDestination::Owners {
                    zone: OwnerZone::Deck,
                },
                initial_posture: None,
                entry_effects: vec![],
                position: Some(deck_position(value.position.as_ref(), path)?),
                order: None,
            },
            reason: MoveReason::Put,
        })
    }

    fn observed_order(
        &mut self,
        cards: Selector,
        placement: Option<&ast::Placement>,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let position = deck_position(placement, path)?;
        if matches!(position, Position::Index { .. }) {
            return Err(unsupported(path));
        }
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

    pub(super) fn reveal(
        &mut self,
        value: &ast::Reveal,
        path: &str,
    ) -> Result<Action, LoweringError> {
        match &value.r#ref {
            ast::RevealRef::Bound(name) => Ok(Action::Reveal {
                objects: Selector::Bound {
                    reference: *self.targets.get(name).ok_or_else(|| unsupported(path))?,
                },
            }),
            ast::RevealRef::WholeHand(hand) => {
                let ast::HandZone::Hand = hand.zone;
                let relation = match hand.who {
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
    }

    /// Another player's top cards may only go to their owner's cemetery; EX stays own-deck only.
    fn top_move(
        &mut self,
        value: &ast::Move,
        top: &ast::DeckTop,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let zone = match value.to {
            ast::Destination::Cemetery => OwnerZone::Cemetery,
            ast::Destination::Ex if own_deck(top) => OwnerZone::Ex,
            ast::Destination::Ex
            | ast::Destination::Field
            | ast::Destination::Deck
            | ast::Destination::Hand => {
                return Err(unsupported(path));
            }
        };
        // A top-to-cemetery result is read only by receipt member conditions.
        if value.r#ref.is_some() || value.position.is_some() || value.order.is_some() {
            return Err(unsupported(path));
        }
        Ok(Action::Move {
            objects: self.deck_slice(top, path)?,
            movement: MoveSpec {
                to: MoveDestination::Owners { zone },
                initial_posture: None,
                entry_effects: vec![],
                position: None,
                order: None,
            },
            reason: MoveReason::Put,
        })
    }

    fn deck_slice(&mut self, top: &ast::DeckTop, path: &str) -> Result<Selector, LoweringError> {
        if top.n == 0 {
            return Err(unsupported(path));
        }
        let player = match &top.who {
            ast::DeckHolder::Side(ast::LeaderSide::Controller) => PlayerRef::Controller {},
            ast::DeckHolder::Side(ast::LeaderSide::Opponent) => {
                super::super::events::timing::opponent()
            }
            // The set survives in the IR; resolution fails loudly unless it has exactly one seat.
            ast::DeckHolder::Group(ast::DeckGroup::Opponents) => PlayerRef::RequireOne {
                players: Box::new(PlayerSet {
                    relation: PlayerRelation::Opponents,
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(0),
                        maximum: None,
                    },
                    text_quantifier: TextQuantifier::All,
                }),
            },
            ast::DeckHolder::OwnerOf { owner_of } => {
                self.selected_player(owner_of, super::super::Role::Owner, path)?
            }
        };
        Ok(Selector::Ordered {
            zone: super::super::ZoneRef {
                player,
                kind: ZoneKind::Deck,
            },
            start: Position::Top {},
            count: super::Expr::Literal(Nat(top.n)),
            direction: super::super::SliceDirection::Forward,
        })
    }

    fn supports_move_result(&self, value: &ast::Move, objects: &ast::Objects) -> bool {
        value.result.is_none()
            || matches!(value.to, ast::Destination::Ex)
            || matches!(objects, ast::Objects::Bound(name)
                if self.observed_targets.contains(name)
                    && matches!(value.to, ast::Destination::Hand | ast::Destination::Cemetery))
    }
}

const fn own_deck(top: &ast::DeckTop) -> bool {
    matches!(top.who, ast::DeckHolder::Side(ast::LeaderSide::Controller))
}

const fn shared_put(objects: Selector, kind: ZoneKind) -> Action {
    Action::Move {
        objects,
        movement: MoveSpec {
            to: MoveDestination::Shared {
                zone: super::super::ZoneRef {
                    player: PlayerRef::Controller {},
                    kind,
                },
            },
            initial_posture: None,
            entry_effects: vec![],
            position: None,
            order: None,
        },
        reason: MoveReason::Put,
    }
}

const fn owner_put(objects: Selector, zone: OwnerZone) -> Action {
    Action::Move {
        objects,
        movement: MoveSpec {
            to: MoveDestination::Owners { zone },
            initial_posture: None,
            entry_effects: vec![],
            position: None,
            order: None,
        },
        reason: MoveReason::Put,
    }
}

fn deck_position(position: Option<&ast::Placement>, path: &str) -> Result<Position, LoweringError> {
    match position {
        Some(ast::Placement::Edge(ast::DeckEdge::Top)) => Ok(Position::Top {}),
        Some(ast::Placement::Edge(ast::DeckEdge::Bottom)) => Ok(Position::Bottom {}),
        Some(ast::Placement::Nth { nth }) => NonZeroU64::new(*nth)
            .map(|index| Position::Index {
                index: PositiveNat(index),
            })
            .ok_or_else(|| unsupported(path)),
        None => Err(unsupported(path)),
    }
}
