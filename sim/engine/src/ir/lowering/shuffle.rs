//! The author exposes only complete deck shuffles and closed unordered placements.
use super::{
    AbilitySource, Action, BodyContext, Expr, Located, LoweringError, Node, SourceSpan, ast,
    unsupported,
};
use crate::ir::shuffle as shape;
use crate::ir::{
    Binding, Bool, Cardinality, FollowPermission, Instruction, InstructionContext, InstructionId,
    MoveDestination, MoveReason, MoveSpec, MovementEvent, MovementReceipt, Nat, ObjectOperand,
    ObjectReceipt, OwnerZone, PlayerRef, PlayerRelation, PlayerSet, Position, RandomnessPolicy,
    ReceiptId, ReceiptReference, SelectionView, Selector, ShuffleScope, TextQuantifier, ZoneKind,
};
use core::slice::from_ref;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Paired instructions share the enclosing lowering scope and allocator."
)]
impl BodyContext<'_> {
    pub(super) fn shuffle(
        &self,
        value: &ast::Shuffle,
        path: &str,
    ) -> Result<Action, LoweringError> {
        Ok(Action::Shuffle {
            deck: self.return_player(&value.who, path)?,
            scope: ShuffleScope::FullZone {},
            randomness: RandomnessPolicy::FreshRequest,
        })
    }

    fn return_player(
        &self,
        player: &ast::DeckHolder,
        path: &str,
    ) -> Result<PlayerRef, LoweringError> {
        match player {
            ast::DeckHolder::Side(ast::LeaderSide::Controller) => Ok(PlayerRef::Controller {}),
            ast::DeckHolder::Bound(name) => self
                .players
                .get(name)
                .map(|binding| PlayerRef::Bound { binding: *binding })
                .ok_or_else(|| unsupported(path)),
            ast::DeckHolder::Side(_)
            | ast::DeckHolder::Group(_)
            | ast::DeckHolder::OwnerOf { .. } => Err(unsupported(path)),
        }
    }

    pub(super) const fn unordered_deck(value: &ast::Move) -> bool {
        matches!(value.to, ast::Destination::Deck)
            && (matches!(value.order, Some(ast::MoveOrder::Shuffle))
                || (value.order.is_none() && value.position.is_none()))
    }

    pub(super) fn shuffle_pair(
        &mut self,
        movement: &ast::Move,
        following: Option<(&ast::Step, &str)>,
        path: &str,
        source: &SourceSpan,
        ability_source: &AbilitySource,
    ) -> Result<Vec<Located<Node>>, LoweringError> {
        if movement.top.is_some() {
            return Err(unsupported(path));
        }
        let (objects, position) = if matches!(movement.order, Some(ast::MoveOrder::Shuffle)) {
            if !matches!(
                movement.position,
                Some(ast::Placement::Edge(ast::DeckEdge::Bottom))
            ) {
                return Err(unsupported(path));
            }
            let Some(ast::MoveObjects::Observed(observed)) = &movement.r#ref else {
                return Err(unsupported(path));
            };
            let cards = self.shuffled_bottom_cards(observed, path)?;
            if !shape::observation(&cards) {
                return Err(unsupported(path));
            }
            (cards, Some(Position::Bottom {}))
        } else {
            let Some((ast::Step::Shuffle(_), _)) = following else {
                return Err(unsupported(path));
            };
            let cards = self.shuffle_return_cards(movement, path)?;
            (cards, None)
        };
        let movement_action = Action::Move {
            objects,
            movement: MoveSpec {
                to: MoveDestination::Owners {
                    zone: OwnerZone::Deck,
                },
                position,
                order: None,
                initial_posture: None,
                entry_effects: vec![],
            },
            reason: MoveReason::Put,
        };
        if movement.result.is_some() && shape::whole_return(&movement_action).is_none() {
            return Err(unsupported(path));
        }
        let first = self.shuffle_instruction(movement_action, path, source, ability_source)?;
        let Node::Instruction { instruction } = &first.node else {
            return Err(unsupported(path));
        };
        let receipt = instruction.receipt;
        if movement.result.is_some() {
            self.receipt_reads.whole_returns.insert(receipt);
        }
        if let Some(name) = &movement.result
            && (self.targets.contains_key(name)
                || self.players.contains_key(name)
                || self.receipt_reads.evolutions.contains_key(name)
                || self.searches.contains(name)
                || self
                    .observations
                    .insert(
                        name.clone(),
                        ObjectReceipt::Movement {
                            receipt: MovementReceipt::Move {
                                receipt: ReceiptReference::new(receipt),
                            },
                        },
                    )
                    .is_some())
        {
            return Err(unsupported(path));
        }
        let (action, shuffle_path) = if position.is_some() {
            (
                Action::Shuffle {
                    deck: PlayerRef::Controller {},
                    randomness: RandomnessPolicy::FreshRequest,
                    scope: ShuffleScope::Subset {
                        cards: Selector::MoveAfter {
                            receipt: MovementReceipt::Move {
                                receipt: ReceiptReference::new(receipt),
                            },
                            permission: FollowPermission::SameEffectExplicitReference {},
                        },
                    },
                },
                format!("{path}/move/order"),
            )
        } else {
            let Some((ast::Step::Shuffle(shuffle), next)) = following else {
                return Err(unsupported(path));
            };
            (self.shuffle(shuffle, next)?, next.to_owned())
        };
        Ok(vec![
            first,
            self.shuffle_instruction(action, &shuffle_path, source, ability_source)?,
        ])
    }

    fn shuffle_return_cards(
        &mut self,
        movement: &ast::Move,
        path: &str,
    ) -> Result<Selector, LoweringError> {
        Ok(match &movement.r#ref {
            Some(ast::MoveObjects::Whole(whole)) => {
                let zones = match &whole.zone {
                    ast::ReturnZones::One(zone) => from_ref(zone),
                    ast::ReturnZones::Many(zones) => zones,
                };
                if zones.is_empty()
                    || zones.len() > 2
                    || matches!(zones, [first, second] if first == second)
                {
                    return Err(unsupported(path));
                }
                let player = self.return_player(&whole.who, path)?;
                let mut sets = Vec::new();
                for zone in zones {
                    sets.push(Selector::Zone {
                        players: PlayerSet {
                            relative_to: player.clone(),
                            relation: PlayerRelation::SelfPlayer,
                            cardinality: Cardinality {
                                minimum: Nat(1),
                                maximum: Some(Nat(1)),
                            },
                            text_quantifier: TextQuantifier::Implicit,
                        },
                        zone: match zone {
                            ast::ReturnZone::Hand => ZoneKind::Hand,
                            ast::ReturnZone::Cemetery => ZoneKind::Cemetery,
                        },
                        view: SelectionView::Current {},
                        binding: Binding::new(self.allocator.id(path)?, self.scope),
                        predicate: Box::new(Expr::Literal(Bool(true))),
                    });
                }
                if sets.len() == 1 {
                    sets.remove(0)
                } else {
                    Selector::Union { sets }
                }
            }
            Some(ast::MoveObjects::Field(objects @ ast::Objects::Bound(name)))
                if self.deck_return_targets.contains(name) =>
            {
                self.objects(objects, path)?
            }
            Some(ast::MoveObjects::Field(ast::Objects::This(_))) if self.departure_to_cemetery => {
                let event = self.leave_event.ok_or_else(|| unsupported(path))?;
                Selector::Objects {
                    objects: vec![ObjectOperand::FollowEventMove {
                        event: MovementEvent::LeaveField { event },
                    }],
                }
            }
            _ => return Err(unsupported(path)),
        })
    }

    fn shuffle_instruction(
        &mut self,
        action: Action,
        path: &str,
        source: &SourceSpan,
        ability_source: &AbilitySource,
    ) -> Result<Located<Node>, LoweringError> {
        let id = self.card_allocator.id(path)?;
        Ok(Self::located_step(
            source,
            path,
            Node::Instruction {
                instruction: Box::new(Instruction {
                    id: InstructionId(id),
                    receipt: ReceiptId(id),
                    actor: PlayerRef::Controller {},
                    source: ability_source.clone(),
                    context: InstructionContext::Effect,
                    precondition: None,
                    action,
                }),
            },
        ))
    }
}
