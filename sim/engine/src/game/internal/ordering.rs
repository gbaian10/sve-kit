//! Private observations and one frozen, same-owner deck permutation.

use super::{
    Execution, Game, InputDomain, InstructionOutcome, Result, Wait, invalid, seat_name, unsupported,
};
use crate::ir::{
    Action, Binding, Capture, CaptureRef, CaptureTime, Control, Expr, Instruction, Int, Located,
    LookOutcome, MemberProjection, MoveDestination, Node, ObjectOperand, ObjectReceipt, ObjectRef,
    OrderAnswer, OrderOutcome, OwnerZone, PlayerId, PlayerRelation, Position, Program,
    ReceiptLocation, Requirements, ResultMember, ResultMemberId, RuntimeObjectView, ScopeId,
    Selector, SnapshotId, SourceSpan, StepwiseMode, Visibility, ZoneKind,
};
use alloc::collections::BTreeSet;

/// Printed information attached to a handle valid only with its decision token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderCard {
    /// Index in this occurrence's frozen domain; never a physical object identity.
    pub handle: u64,
    /// Catalogue identity shared by all copies of this printed card.
    pub card: String,
    /// Adopted rules name from the same catalogue.
    pub name: String,
}

#[derive(Clone, Debug)]
pub(super) struct OrderWait {
    instruction: Box<Instruction>,
    cards: Vec<ObjectRef>,
    player: PlayerId,
    deck: Vec<String>,
}

#[expect(
    clippy::too_many_lines,
    reason = "The contextual private-read guard checks the complete local node."
)]
pub(super) fn check_node(node: &Node, source: &SourceSpan) -> Result<()> {
    let private_spine = matches!(node, Node::Instruction { instruction }
        if matches!(instruction.action, Action::Order { .. }) || super::movement::observed_move(&instruction.action));
    // Control bodies are checked individually; their guards still need the same restriction.
    if let Node::Control { control } = node {
        if let Control::If { condition, .. } = control {
            return check_node(
                &Node::Capture {
                    capture: Capture::Boolean {
                        at: CaptureTime::AtResolution { scope: ScopeId(0) },
                        value: condition.clone(),
                        into: CaptureRef {
                            binding: Binding::new(0, ScopeId(0)),
                        },
                    },
                },
                source,
            );
        }
        return Ok(());
    }
    if let Node::Instruction { instruction } = node {
        let cards = match &instruction.action {
            Action::Order { cards, .. } => Some(cards),
            Action::Move { objects, .. } if private_spine => Some(objects),
            _ => None,
        };
        // Private selectors are admitted only on the spine evaluated with observation authority.
        let mut leaves = Vec::new();
        if let Some(cards) = cards {
            order_leaves(cards, &mut leaves);
        }
        for leaf in leaves {
            check_node(
                &Node::Capture {
                    capture: Capture::Objects {
                        at: CaptureTime::AtResolution { scope: ScopeId(0) },
                        value: leaf.clone(),
                        into: CaptureRef {
                            binding: Binding::new(0, ScopeId(0)),
                        },
                    },
                },
                source,
            )?;
        }
    }
    let mut inspected = node.clone();
    if let Node::Instruction { instruction } = &mut inspected
        && let Action::Destroy { objects: selector }
        | Action::Banish { objects: selector }
        | Action::Posture {
            objects: selector, ..
        }
        | Action::Modify {
            subjects: selector, ..
        } = &mut instruction.action
    {
        mask_public_field_predicate(selector);
    }
    if let Node::Instruction { instruction } = &mut inspected
        && (super::movement::observed_move(&instruction.action)
            || matches!(&instruction.action, Action::Move { movement, .. } if super::movement::field_move(movement) || movement.to == (MoveDestination::Owners { zone: OwnerZone::Hand })))
    {
        super::movement::check_action(&instruction.action)?;
        if let Action::Move { movement, .. } = &mut instruction.action {
            // This closed destination does not authorize Shared destinations in any other context.
            movement.to = MoveDestination::Owners {
                zone: OwnerZone::Cemetery,
            };
        }
    }
    if let Node::Instruction { instruction } = &mut inspected
        && let Action::Search { player, .. } = &instruction.action
    {
        super::search::check_action(&instruction.action)?;
        // The private grammar covers only Search's action, never its source or instruction metadata.
        instruction.action = Action::Draw {
            player: player.clone(),
            count: Expr::Literal(Int(0)),
            mode: StepwiseMode::Exact,
        };
    }
    if let Node::Instruction { instruction } = &mut inspected
        && let Action::Look { zone, .. } = &mut instruction.action
    {
        // Only the declared Look zone is authorized, never another Deck read in its count.
        zone.kind = ZoneKind::Field;
    }
    let requirements = Requirements::derive_program(&Located {
        node: Program {
            scope: ScopeId(0),
            steps: vec![Located {
                node: inspected,
                source: source.clone(),
            }],
        },
        source: source.clone(),
    });
    for requirement in &requirements.required {
        let code = requirement.capability.code;
        if (!private_spine && code == "shape.ObjectReceipt.Look")
            || code == "internal.ZoneKind.Deck"
            || (super::search::private_requirement(code)
                && !super::source::source_original(&requirements, requirement))
        {
            return Err(unsupported(
                "private observation outside an authorized selector spine",
            ));
        }
    }
    if let Node::Instruction { instruction } = node {
        match &instruction.action {
            // Only an audience of exactly the chooser has an admitted projection contract.
            Action::Order {
                visibility,
                chooser,
                ..
            } if !matches!(visibility, Visibility::Only { players }
                if players.relation == PlayerRelation::SelfPlayer && players.relative_to == *chooser) =>
            {
                return Err(unsupported("order audience other than the chooser"));
            }
            Action::Look { zone, mode, .. }
                if zone.kind != ZoneKind::Deck || *mode != StepwiseMode::Exact =>
            {
                return Err(unsupported("look zone or stepwise mode"));
            }
            Action::Order { destinations, .. } if !matches!(destinations.as_slice(), [placement] if matches!(placement.position, Position::Top {} | Position::Bottom {})) =>
            {
                return Err(unsupported("order placement"));
            }
            _ => {}
        }
    }
    Ok(())
}

fn mask_public_field_predicate(selector: &mut Selector) {
    match selector {
        Selector::Difference { input, except }
            if matches!(except.as_ref(), Selector::Objects { objects }
                if matches!(objects.as_slice(), [ObjectOperand::SourceHolder {}])) =>
        {
            mask_public_field_predicate(input);
        }
        selector
            if matches!(
                selector,
                Selector::Zone {
                    zone: ZoneKind::Field,
                    ..
                }
            ) && super::card::selection::field_selector(selector).is_ok() =>
        {
            // Only this public candidate grammar is evaluated per object by the field authority.
            if let Selector::Zone { predicate, .. } = selector {
                **predicate = Expr::Literal(super::Bool(true));
            }
        }
        _ => {}
    }
}

fn order_leaves<'selector>(selector: &'selector Selector, leaves: &mut Vec<&'selector Selector>) {
    match selector {
        Selector::ReceiptMembers {
            receipt: ObjectReceipt::Look { .. },
            ..
        } => {}
        Selector::Union { sets } => {
            for set in sets {
                order_leaves(set, leaves);
            }
        }
        Selector::Difference { input, except } => {
            order_leaves(input, leaves);
            order_leaves(except, leaves);
        }
        _ => leaves.push(selector),
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Ordering mutates the same authoritative deck and observation state."
)]
impl Game {
    pub(super) fn internal_look(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        let Action::Look {
            viewer,
            zone,
            count,
            ..
        } = &instruction.action
        else {
            return Err(unsupported("look action"));
        };
        let viewer = execution.player(self, viewer)?;
        let player = execution.player(self, &zone.player)?;
        if viewer != player {
            return Err(unsupported("foreign-owner private look"));
        }
        let seat = seat_name(player)?;
        let count = usize::try_from(execution.natural(self, count)?.0).map_err(invalid)?;
        let deck = self.internal_known_deck(seat)?;
        let mut looked = Vec::new();
        for id in deck.iter().take(count) {
            let object = execution.reference(self, id)?;
            self.learn(seat, id, true);
            looked.push(RuntimeObjectView {
                object,
                snapshot: SnapshotId(self.state.ir_epoch),
            });
        }
        execution.look_viewers.insert(instruction.receipt, viewer);
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Look(LookOutcome { looked }),
        );
        Ok(())
    }

    pub(super) fn internal_order(
        &mut self,
        execution: &mut Execution,
        instruction: Box<Instruction>,
    ) -> Result<()> {
        let Action::Order {
            decision,
            cards,
            destinations,
            chooser,
            visibility,
        } = &instruction.action
        else {
            return Err(unsupported("order action"));
        };
        let [placement] = destinations.as_slice() else {
            return Err(unsupported("multiple order placements"));
        };
        if placement.cards != *cards
            || placement.zone
                != (MoveDestination::Owners {
                    zone: OwnerZone::Deck,
                })
        {
            return Err(unsupported("order domain or destination"));
        }
        let player = execution.player(self, chooser)?;
        let Visibility::Only { players } = visibility else {
            return Err(unsupported("public order audience"));
        };
        if execution.players(self, players)? != vec![player] {
            return Err(unsupported("order audience"));
        }
        let cards = execution.observed_objects(self, cards, player)?;
        let mut unique = BTreeSet::new();
        let seat = seat_name(player)?;
        let deck = self.internal_known_deck(seat)?;
        let mut domain = Vec::new();
        for (index, &card) in cards.iter().enumerate() {
            if !unique.insert((card.id, card.generation)) {
                return Err(invalid("duplicate order domain"));
            }
            let id = execution.object_id(card)?;
            let current = self.object(id)?;
            if current.generation != card.generation.0
                || current.zone != "deck"
                || current.owner != seat
                || current.controller != seat
                || !deck.iter().any(|member| member == id)
            {
                return Err(unsupported(
                    "order requires current generations in the chooser's deck",
                ));
            }
            if !self
                .state
                .knowledge
                .get(seat)
                .is_some_and(|knowledge| knowledge.located.contains(id))
            {
                return Err(unsupported(
                    "private order requires current observation evidence",
                ));
            }
            domain.push(OrderCard {
                handle: u64::try_from(index).map_err(invalid)?,
                card: current.card.clone(),
                name: self.card_name(id)?,
            });
        }
        let decision = *decision;
        let wait = OrderWait {
            instruction,
            cards,
            player,
            deck,
        };
        if domain.is_empty() {
            return self.internal_order_answer(execution, &wait, Vec::new());
        }
        self.internal_wait(
            execution,
            decision,
            player,
            InputDomain::Order(domain),
            Wait::Order(wait),
        )
    }

    pub(super) fn internal_order_answer(
        &mut self,
        execution: &mut Execution,
        wait: &OrderWait,
        ordered: Vec<ObjectRef>,
    ) -> Result<()> {
        let seat = seat_name(wait.player)?;
        if self.internal_known_deck(seat)? != wait.deck {
            return Err(invalid("order snapshot changed"));
        }
        let ids = ordered
            .iter()
            .map(|&object| {
                let id = execution.object_id(object)?;
                if self.object(id)?.generation != object.generation.0 {
                    return Err(invalid("stale order generation"));
                }
                Ok(id.to_owned())
            })
            .collect::<Result<Vec<_>>>()?;
        let Action::Order { destinations, .. } = &wait.instruction.action else {
            return Err(unsupported("order continuation"));
        };
        let [placement] = destinations.as_slice() else {
            return Err(unsupported("order continuation placement"));
        };
        let mut remaining = wait
            .deck
            .iter()
            .filter(|id| !ids.contains(id))
            .cloned()
            .collect::<Vec<_>>();
        let deck = match placement.position {
            Position::Top {} => ids.into_iter().chain(remaining).collect::<Vec<_>>(),
            Position::Bottom {} => {
                remaining.extend(ids);
                remaining
            }
            Position::Index { .. } => return Err(unsupported("indexed order")),
        };
        self.player_mut(seat)?.zones.insert(
            "deck".into(),
            deck.into_iter().map(serde_json::Value::String).collect(),
        );
        // A private rearrangement revokes old locations even when the permutation happens to be identical.
        if !ordered.is_empty() {
            execution.look_viewers.clear();
            for knowledge in self.state.knowledge.values_mut() {
                for id in &wait.deck {
                    knowledge.located.remove(id);
                }
            }
        }
        let snapshot = SnapshotId(self.state.ir_epoch);
        let successful = ordered
            .into_iter()
            .enumerate()
            .map(|(index, object)| {
                Ok(ResultMember {
                    id: ResultMemberId(u64::try_from(index).map_err(invalid)?),
                    before: Some(RuntimeObjectView { object, snapshot }),
                    after: Some(RuntimeObjectView { object, snapshot }),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        execution.receipts.insert(
            wait.instruction.receipt,
            InstructionOutcome::Order(OrderOutcome {
                successful,
                moves: Vec::new(),
            }),
        );
        Ok(())
    }

    pub(super) fn internal_known_deck(&self, seat: &str) -> Result<Vec<String>> {
        self.zone(seat, "deck")
            .into_iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| unsupported("anonymous deck members"))
            })
            .collect()
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Observed selectors share execution receipts with ordinary value evaluation."
)]
impl Execution {
    pub(super) fn observed_objects(
        &self,
        game: &Game,
        selector: &Selector,
        actor: PlayerId,
    ) -> Result<Vec<ObjectRef>> {
        match selector {
            Selector::ReceiptMembers {
                receipt: ObjectReceipt::Look { receipt },
                projection: MemberProjection::Before | MemberProjection::After,
            } => {
                let ReceiptLocation::Local { id } = receipt.location() else {
                    return Err(unsupported("iteration look receipt"));
                };
                if self.look_viewers.get(id) != Some(&actor) {
                    return Err(unsupported("private look audience"));
                }
                let Some(InstructionOutcome::Look(outcome)) = self.receipts.get(id) else {
                    return Err(invalid("missing look receipt"));
                };
                Ok(outcome.looked.iter().map(|member| member.object).collect())
            }
            Selector::Difference { input, except } => {
                let input = self.observed_objects(game, input, actor)?;
                let except = self.observed_objects(game, except, actor)?;
                Ok(input
                    .into_iter()
                    .filter(|object| !except.contains(object))
                    .collect())
            }
            Selector::Union { sets } => {
                let mut seen = BTreeSet::new();
                let mut result = Vec::new();
                for set in sets {
                    for object in self.observed_objects(game, set, actor)? {
                        if seen.insert((object.id, object.generation)) {
                            result.push(object);
                        }
                    }
                }
                Ok(result)
            }
            _ => self.objects(game, selector),
        }
    }
}

impl OrderWait {
    pub(super) fn validate(&self, handles: &[u64]) -> Result<Vec<ObjectRef>> {
        let ordered = handles
            .iter()
            .map(|&handle| {
                self.cards
                    .get(usize::try_from(handle).map_err(invalid)?)
                    .copied()
                    .ok_or_else(|| invalid("foreign order handle"))
            })
            .collect::<Result<Vec<_>>>()?;
        if !(OrderAnswer {
            cards: ordered.clone(),
        })
        .matches_domain(&self.cards)
        {
            return Err(invalid("order answer is not a full permutation"));
        }
        Ok(ordered)
    }
}
