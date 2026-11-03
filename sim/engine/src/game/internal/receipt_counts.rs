use super::Node;
use super::{Execution, Fact, Game, InstructionOutcome, Result, invalid, unsupported};
use crate::ir::observation_selection::private_look_choice;
use crate::ir::{Action, ChoiceDomain, Control, Selector, ZoneKind};
use crate::ir::{
    Binding, Bool, CardKind, CardName, CharacteristicView, Characteristics, ClassId, ClassRead,
    Comparison, Expr, Info, Int, IntOperation, KindRead, MemberProjection, MoveOutcome,
    MovementReceipt, NamesRead, NumberRead, ObjectReceipt, ObjectRef, ObjectView, Predicate,
    QuantityUnit, ReceiptId, ReceiptLocation, ReceiptReference, RuntimeObjectView, TraitId,
    TraitsRead,
};
use crate::ir::{DamageAmount, MoveDestination, Nat, NatOperation, OwnerZone, PlayerRef};
use alloc::collections::BTreeSet;
use core::convert::identity;

/// The member information a receipt condition may read.
struct MemberCard {
    names: Vec<CardName>,
    kind: CardKind,
    traits: Vec<TraitId>,
    class: Option<ClassId>,
}

impl From<&Characteristics> for MemberCard {
    fn from(characters: &Characteristics) -> Self {
        Self {
            names: characters.names.clone(),
            kind: characters.kind,
            traits: characters.traits.clone(),
            class: characters.class.clone(),
        }
    }
}

fn local_receipt(receipt: &ObjectReceipt) -> Result<ReceiptId> {
    let id = match receipt {
        ObjectReceipt::Movement {
            receipt: MovementReceipt::Discard { receipt },
        } => match receipt.location() {
            ReceiptLocation::Local { id } => Some(id),
            ReceiptLocation::Iteration { .. } => None,
        },
        ObjectReceipt::Movement {
            receipt: MovementReceipt::Move { receipt },
        } => match receipt.location() {
            ReceiptLocation::Local { id } => Some(id),
            ReceiptLocation::Iteration { .. } => None,
        },
        _ => return Err(unsupported("receipt member condition")),
    };
    id.copied()
        .ok_or_else(|| unsupported("iterated receipt member condition"))
}

/// The public mask admits only discard receipts; deck movements need their producer.
pub(super) fn check(
    receipt: &ObjectReceipt,
    projection: MemberProjection,
    member: Binding<RuntimeObjectView>,
    predicate: &Expr<Bool>,
) -> Result<()> {
    if !matches!(
        receipt,
        ObjectReceipt::Movement {
            receipt: MovementReceipt::Discard { .. }
        }
    ) {
        return Err(unsupported(
            "receipt member condition without a checked producer",
        ));
    }
    check_shape(receipt, projection, member, predicate)
}

fn check_shape(
    receipt: &ObjectReceipt,
    projection: MemberProjection,
    member: Binding<RuntimeObjectView>,
    predicate: &Expr<Bool>,
) -> Result<()> {
    local_receipt(receipt)?;
    if projection != MemberProjection::Before {
        return Err(unsupported("receipt member projection"));
    }
    matches(predicate, member, None, None).map(|_| ())
}

/// Movement receipts whose members provably left the deck, in program order.
#[derive(Default)]
pub(super) struct DeckReads {
    looks: Vec<Binding<Selector>>,
    moves: BTreeSet<ReceiptId>,
    cemetery: BTreeSet<ReceiptId>,
}

impl DeckReads {
    /// Records a deck-top movement or a movement of a private Look choice.
    pub(super) fn record(&mut self, node: &Node) {
        match node {
            Node::Decision { decision } if private_look_choice(decision) => {
                if let ChoiceDomain::Objects { binding, .. } = &decision.domain {
                    self.looks.push(*binding);
                }
            }
            Node::Instruction { instruction } => {
                let looked = matches!(&instruction.action,
                    Action::Move { objects: Selector::Bound { reference }, .. }
                        if self.looks.contains(reference));
                if looked || super::movement::deck_top(&instruction.action).is_some() {
                    self.moves.insert(instruction.receipt);
                    if matches!(&instruction.action, Action::Move { objects: Selector::Ordered { zone, .. }, movement, .. }
                        if zone.kind == ZoneKind::Deck && zone.player == (PlayerRef::Controller {}) && movement.to == (MoveDestination::Owners { zone: OwnerZone::Cemetery }))
                    {
                        self.cemetery.insert(instruction.receipt);
                    }
                }
            }
            _ => {}
        }
    }

    /// Masks an If guard's deck movement conditions once their producers are proven.
    pub(super) fn mask(&self, node: &Node) -> Result<Node> {
        let mut inspected = node.clone();
        if let Node::Control {
            control: Control::If { condition, .. },
        } = &mut inspected
        {
            self.mask_condition(condition)?;
        }
        if let Node::Instruction { instruction } = &mut inspected
            && let Action::Damage {
                amount: DamageAmount::Uniform { amount },
                ..
            } = &mut instruction.action
            && let Expr::Compute(operation) = amount
            && let IntOperation::FromNat {
                value: Expr::Compute(operation),
            } = operation.as_mut()
            && let NatOperation::ReceiptCount {
                receipt,
                projection,
                member,
                predicate,
            } = operation.as_ref()
        {
            if !self.cemetery.contains(&local_receipt(receipt)?) {
                return Err(unsupported(
                    "filtered count outside a deck-to-cemetery producer",
                ));
            }
            check_count(receipt, *projection, *member, predicate)?;
            *amount = Expr::Literal(Int(0));
        }
        Ok(inspected)
    }

    fn mask_condition(&self, condition: &mut Expr<Bool>) -> Result<()> {
        let Expr::Compute(predicate) = condition else {
            return Ok(());
        };
        match predicate.as_mut() {
            Predicate::And { predicates } | Predicate::Or { predicates } => {
                for child in predicates {
                    self.mask_condition(child)?;
                }
            }
            Predicate::Not { predicate } => self.mask_condition(predicate)?,
            Predicate::ReceiptAny {
                receipt:
                    receipt @ ObjectReceipt::Movement {
                        receipt: MovementReceipt::Move { .. },
                    },
                projection,
                member,
                predicate,
            } => {
                if !self.moves.contains(&local_receipt(receipt)?) {
                    return Err(unsupported(
                        "receipt member condition outside a deck movement",
                    ));
                }
                check_shape(receipt, *projection, *member, predicate)?;
                *condition = Expr::Literal(Bool(false));
            }
            _ => {}
        }
        Ok(())
    }
}

pub(super) fn any(
    game: &Game,
    execution: &Execution,
    receipt: &ObjectReceipt,
    projection: MemberProjection,
    member: Binding<RuntimeObjectView>,
    predicate: &Expr<Bool>,
) -> Result<bool> {
    check_shape(receipt, projection, member, predicate)?;
    let members = members(game, execution, receipt, None)?;
    for (card, before) in &members {
        if matches(
            predicate,
            member,
            Some(card),
            Some((game, execution, *before)),
        )? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn members(
    game: &Game,
    execution: &Execution,
    receipt: &ObjectReceipt,
    destination: Option<ZoneKind>,
) -> Result<Vec<(MemberCard, ObjectRef)>> {
    let id = local_receipt(receipt)?;
    match execution.receipts.get(&id) {
        Some(InstructionOutcome::Discard(outcome)) => outcome
            .successful
            .iter()
            .map(|result| {
                let before = result
                    .before
                    .ok_or_else(|| invalid("missing discard before member"))?;
                let characters = execution
                    .facts
                    .iter()
                    .find_map(|fact| match fact {
                        Fact::Discard(event) if event.card_before == before.object => {
                            Some(&event.characters)
                        }
                        _ => None,
                    })
                    .ok_or_else(|| invalid("missing discarded characteristics"))?;
                Ok((MemberCard::from(characters), before.object))
            })
            .collect::<Result<Vec<_>>>(),
        Some(InstructionOutcome::Move(outcome)) => outcome
            .moved
            .iter()
            .filter(|movement| {
                movement.after.is_some()
                    && destination
                        .is_none_or(|zone| movement.actual_to.is_some_and(|to| to.kind == zone))
            })
            .map(|movement| {
                // Printed information stands in only for a card that left the deck.
                let from_deck = execution.facts.iter().any(|fact| {
                    matches!(fact, Fact::Movement(moved)
                        if moved.movement.before == movement.before
                            && moved.from.kind == ZoneKind::Deck)
                });
                if !from_deck {
                    return Err(unsupported("receipt member that did not leave the deck"));
                }
                Ok((
                    printed_card(game, execution, movement.before)?,
                    movement.before,
                ))
            })
            .collect::<Result<Vec<_>>>(),
        _ => Err(invalid("missing receipt member producer")),
    }
}

fn check_count(
    receipt: &ObjectReceipt,
    projection: MemberProjection,
    member: Binding<RuntimeObjectView>,
    predicate: &Expr<Bool>,
) -> Result<()> {
    if !matches!(
        receipt,
        ObjectReceipt::Movement {
            receipt: MovementReceipt::Move { .. }
        }
    ) || !matches!(predicate, Expr::Compute(outer) if matches!(outer.as_ref(), Predicate::And { predicates } if matches!(predicates.as_slice(), [Expr::Compute(inner)] if matches!(inner.as_ref(), Predicate::HasTrait { .. }))))
    {
        return Err(unsupported("filtered deck movement trait count"));
    }
    check_shape(receipt, projection, member, predicate)
}

pub(super) fn count(
    game: &Game,
    execution: &Execution,
    receipt: &ObjectReceipt,
    projection: MemberProjection,
    member: Binding<RuntimeObjectView>,
    predicate: &Expr<Bool>,
) -> Result<Nat> {
    check_count(receipt, projection, member, predicate)?;
    let members = members(game, execution, receipt, Some(ZoneKind::Cemetery))?;
    let mut count = 0_u64;
    for (card, before) in &members {
        if matches(
            predicate,
            member,
            Some(card),
            Some((game, execution, *before)),
        )? {
            count = count
                .checked_add(1)
                .ok_or_else(|| invalid("receipt member count overflow"))?;
        }
    }
    Ok(Nat(count))
}

/// Cards leaving the deck carry no modifications, so their printing is their information.
fn printed_card(game: &Game, execution: &Execution, before: ObjectRef) -> Result<MemberCard> {
    let card = &game.object(execution.object_id(before)?)?.card;
    let printed = game.catalog.face(card, 0)?;
    let kind = match printed["card_type"].as_str().unwrap_or_default() {
        kind if kind.contains("フォロワー") => CardKind::Follower,
        kind if kind.starts_with("スペル") => CardKind::Spell,
        kind if kind.starts_with("アミュレット") => CardKind::Amulet,
        _ => return Err(unsupported("receipt member card kind")),
    };
    let text = |value: &serde_json::Value| {
        value
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| invalid("unknown printed receipt member text"))
    };
    Ok(MemberCard {
        names: vec![CardName(text(&printed["name"])?)],
        kind,
        traits: printed["traits"]
            .as_array()
            .ok_or_else(|| invalid("unknown printed traits"))?
            .iter()
            .map(|value| text(value).map(TraitId))
            .collect::<Result<_>>()?,
        class: Some(ClassId(text(&printed["card_class"])?)),
    })
}

pub(super) fn paid_material_attack(
    execution: &Execution,
    receipt: &ReceiptReference<MoveOutcome>,
) -> Result<Int> {
    let ReceiptLocation::Local { id } = receipt.location() else {
        return Err(unsupported("iterated paid material attack"));
    };
    let Some(InstructionOutcome::Payment {
        quantity,
        subjects,
        characters,
    }) = execution.receipts.get(id)
    else {
        return Err(invalid("missing material payment"));
    };
    let ([_], [characters]) = (subjects.as_slice(), characters.as_slice()) else {
        return Err(invalid("material attack requires one frozen follower"));
    };
    if quantity.unit != QuantityUnit::Cards || quantity.amount != Int(1) {
        return Err(invalid("material attack quantity"));
    }
    match characters.attack {
        Info::Present(amount) => Ok(amount),
        Info::AbsentByRules => Err(invalid("missing frozen material attack")),
    }
}

fn snapshot(object: &ObjectView, member: Binding<RuntimeObjectView>) -> Result<()> {
    if !matches!(object,ObjectView::Snapshot{value} if value.as_ref()==&Expr::Bound(member)) {
        return Err(unsupported("receipt member snapshot"));
    }
    Ok(())
}

fn matches(
    expression: &Expr<Bool>,
    member: Binding<RuntimeObjectView>,
    characters: Option<&MemberCard>,
    original: Option<(&Game, &Execution, ObjectRef)>,
) -> Result<bool> {
    let Expr::Compute(predicate) = expression else {
        return match expression {
            Expr::Literal(Bool(value)) => Ok(*value),
            _ => Err(unsupported("receipt member predicate")),
        };
    };
    match predicate.as_ref() {
        Predicate::And { predicates } | Predicate::Or { predicates } => {
            // Validate every branch even when an earlier value determines the result.
            let values = predicates
                .iter()
                .map(|item| matches(item, member, characters, original))
                .collect::<Result<Vec<_>>>()?;
            Ok(if matches!(predicate.as_ref(), Predicate::And { .. }) {
                values.into_iter().all(identity)
            } else {
                values.into_iter().any(identity)
            })
        }
        Predicate::Not { predicate } => {
            matches(predicate, member, characters, original).map(|value| !value)
        }
        Predicate::CompareInt {
            left: Expr::Compute(operation),
            comparison,
            right: Expr::Literal(Int(threshold)),
        } if *threshold >= 0 && *comparison == Comparison::GreaterEqual => {
            original_cost_matches(operation, member, *threshold, original)
        }
        Predicate::KindEquals {
            left: Expr::Live(read),
            right: Expr::Literal(expected),
        } => {
            let KindRead::Kind { object, .. } = read.as_ref();
            snapshot(object, member)?;
            Ok(characters.is_some_and(|characters| characters.kind == *expected))
        }
        Predicate::ClassEquals {
            left: Expr::Live(read),
            right: Expr::Literal(expected),
        } => {
            let ClassRead::Class { object, .. } = read.as_ref();
            snapshot(object, member)?;
            Ok(
                characters.is_some_and(|characters| match (&characters.class, expected) {
                    (Some(actual), Info::Present(expected)) => actual == expected,
                    (None, Info::AbsentByRules) => true,
                    _ => false,
                }),
            )
        }
        Predicate::HasTrait {
            traits: Expr::Live(read),
            trait_id,
        } => {
            let TraitsRead::Traits { object, .. } = read.as_ref();
            snapshot(object, member)?;
            Ok(characters.is_some_and(|characters| characters.traits.contains(trait_id)))
        }
        Predicate::NamesIntersect {
            left: Expr::Live(read),
            right: Expr::Literal(expected),
        } => {
            let NamesRead::Names { object } = read.as_ref();
            snapshot(object, member)?;
            Ok(characters.is_some_and(|characters| {
                characters.names.iter().any(|name| expected.contains(name))
            }))
        }
        Predicate::NameContains {
            object,
            name: Expr::Literal(name),
        } => {
            snapshot(object, member)?;
            Ok(characters.is_some_and(|characters| {
                characters
                    .names
                    .iter()
                    .any(|candidate| candidate.0.contains(&name.0))
            }))
        }
        _ => Err(unsupported("receipt member predicate shape")),
    }
}

fn original_cost_matches(
    operation: &IntOperation,
    member: Binding<RuntimeObjectView>,
    threshold: i64,
    original: Option<(&Game, &Execution, ObjectRef)>,
) -> Result<bool> {
    let IntOperation::InformationOrZero {
        information:
            NumberRead::Cost {
                object,
                view: CharacteristicView::Original,
            },
    } = operation
    else {
        return Err(unsupported("receipt member original cost comparison"));
    };
    snapshot(object, member)?;
    let Some((game, execution, before)) = original else {
        return Ok(false);
    };
    // The immutable base printing survives movement and effective-cost changes.
    let card = &game.object(execution.object_id(before)?)?.card;
    let cost = super::cost::printed_cost(&game.catalog.face(card, 0)?["cost"])?;
    Ok(cost >= threshold)
}
