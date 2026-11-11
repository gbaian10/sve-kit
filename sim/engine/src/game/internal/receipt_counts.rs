use super::Node;
use super::receipt_members::{
    MemberCard, MemberView, distinct_cost_receipt, local_receipt, members,
};
use super::{Execution, Game, Result, invalid, unsupported};
use crate::ir::observation_selection::private_look_choice;
use crate::ir::receipt_reads;
use crate::ir::{Action, ChoiceDomain, Control, Selector, ZoneKind};
use crate::ir::{
    Binding, Bool, Capture, CharacteristicPatch, CharacteristicView, ClassRead, Comparison, Expr,
    Info, Int, IntOperation, KindRead, MemberProjection, MovementReceipt, NamesRead, NumberRead,
    NumericChange, ObjectProperty, ObjectReceipt, ObjectView, Predicate, ReceiptId,
    RuntimeObjectView, TraitsRead,
};
use crate::ir::{DamageAmount, MoveDestination, Nat, NatOperation, OwnerZone, PlayerRef};
use alloc::collections::BTreeSet;
use core::convert::identity;

/// Discards and Fusion costs have public before-members. Deck moves need a checked producer.
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
        } | ObjectReceipt::Fusion { .. }
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

/// Only previously proven producers authorize the corresponding member reads.
#[derive(Default)]
pub(super) struct ReceiptReads {
    looks: Vec<Binding<Selector>>,
    moves: BTreeSet<ReceiptId>,
    field_cemetery: BTreeSet<ReceiptId>,
    cemetery: BTreeSet<ReceiptId>,
    ex: BTreeSet<ReceiptId>,
    field: BTreeSet<ReceiptId>,
    observations: BTreeSet<ReceiptId>,
    published: BTreeSet<ReceiptId>,
}

impl ReceiptReads {
    /// Keeps producer evidence in program order, separate from the member data.
    pub(super) fn record(&mut self, node: &Node) {
        match node {
            Node::Decision { decision } if private_look_choice(decision) => {
                if let ChoiceDomain::Objects { binding, .. } = &decision.domain {
                    self.looks.push(*binding);
                }
            }
            Node::Instruction { instruction } => {
                if instruction.precondition.is_none()
                    && receipt_reads::field_cemetery(&instruction.action)
                {
                    self.field_cemetery.insert(instruction.receipt);
                }
                if instruction.precondition.is_none()
                    && matches!(instruction.action, Action::Look { .. })
                {
                    self.observations.insert(instruction.receipt);
                }
                if let Action::Reveal { objects } = &instruction.action
                    && instruction.precondition.is_none()
                    && let Some(id) = super::reveal::look_receipt(objects)
                    && self.observations.contains(&id)
                {
                    self.published.insert(id);
                }
                let looked = matches!(&instruction.action,
                    Action::Move { objects: Selector::Bound { reference }, .. }
                        if self.looks.contains(reference))
                    || matches!(&instruction.action, Action::Move { objects, .. } if super::reveal::look_receipt(objects).is_some_and(|id| self.published.contains(&id)));
                if looked || super::movement::deck_top(&instruction.action).is_some() {
                    self.moves.insert(instruction.receipt);
                    if let Action::Move { movement, .. } = &instruction.action {
                        if movement.to
                            == (MoveDestination::Owners {
                                zone: OwnerZone::Ex,
                            })
                            || matches!(&movement.to, MoveDestination::Shared { zone }
                            if zone.player == (PlayerRef::Controller {}) && zone.kind == ZoneKind::Ex)
                        {
                            self.ex.insert(instruction.receipt);
                        }
                        if super::movement::field_move(movement) {
                            self.field.insert(instruction.receipt);
                        }
                    }
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

    /// Masks reads only after their deck movement producers are proven.
    pub(super) fn mask(&self, node: &Node) -> Result<Node> {
        let mut inspected = node.clone();
        self.mask_published_numbers(&mut inspected)?;
        if let Node::Control {
            control: Control::If { condition, .. },
        } = &mut inspected
        {
            self.mask_condition(condition)?;
        }
        if let Node::Instruction { instruction } = &mut inspected
            && let Action::Life {
                change: NumericChange::Add { amount },
                ..
            }
            | Action::Damage {
                amount: DamageAmount::Uniform { amount },
                ..
            } = &mut instruction.action
            && let Expr::Compute(operation) = amount
            && let IntOperation::Sum { objects, property } = operation.as_ref()
            && let Some((id, zone)) = super::aggregates::followed_sum_plan(objects, *property)
            && match zone {
                ZoneKind::Ex => self.ex.contains(&id),
                ZoneKind::Field => self.field.contains(&id),
                _ => false,
            }
        {
            *amount = Expr::Literal(Int(0));
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

    fn mask_number(&self, number: &mut Expr<Int>, deck_count: bool) -> Result<()> {
        if let Expr::Compute(operation) = number {
            match operation.as_mut() {
                IntOperation::Sum { objects, property }
                    if super::reveal::look_sum(objects, *property).is_some() =>
                {
                    let id = super::reveal::look_sum(objects, *property)
                        .ok_or_else(|| unsupported("public cost sum"))?;
                    if !self.published.contains(&id) {
                        return Err(unsupported("cost sum before revelation"));
                    }
                    *number = Expr::Literal(Int(0));
                }
                IntOperation::FromNat {
                    value: Expr::Compute(natural),
                } => match natural.as_ref() {
                    NatOperation::ReceiptCount {
                        receipt,
                        projection,
                        member,
                        predicate,
                    } if deck_count => {
                        if !self.cemetery.contains(&local_receipt(receipt)?) {
                            return Err(unsupported(
                                "filtered count outside a deck-to-cemetery producer",
                            ));
                        }
                        check_count(receipt, *projection, *member, predicate)?;
                        *number = Expr::Literal(Int(0));
                    }
                    NatOperation::DistinctCount {
                        objects,
                        property: ObjectProperty::Names,
                    } if let Some(id) = super::reveal::look_receipt(objects) => {
                        if !self.published.contains(&id) {
                            return Err(unsupported("distinct names before revelation"));
                        }
                        *number = Expr::Literal(Int(0));
                    }
                    NatOperation::DistinctCount {
                        objects:
                            Selector::ReceiptMembers {
                                receipt,
                                projection,
                            },
                        property: ObjectProperty::Cost,
                    } => {
                        let id = distinct_cost_receipt(receipt, *projection)?;
                        if !self.cemetery.contains(&id) {
                            return Err(unsupported(
                                "distinct original costs without an own deck-to-cemetery producer",
                            ));
                        }
                        *number = Expr::Literal(Int(0));
                    }
                    _ => {}
                },
                IntOperation::Arithmetic { arguments, .. } => {
                    for argument in arguments {
                        self.mask_number(argument, deck_count)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn mask_change(&self, change: &mut NumericChange) -> Result<()> {
        let (NumericChange::Add { amount } | NumericChange::Set { amount }) = change;
        self.mask_number(amount, false)
    }

    fn mask_patch(&self, patch: &mut CharacteristicPatch) -> Result<()> {
        match patch {
            CharacteristicPatch::Numeric { change, .. } => self.mask_change(change),
            CharacteristicPatch::Combined { patches } => {
                for component in patches {
                    self.mask_patch(component)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn mask_published_numbers(&self, node: &mut Node) -> Result<()> {
        match node {
            Node::Capture {
                capture: Capture::Integer { value, .. },
            } => self.mask_number(value, false),
            Node::Instruction { instruction } => match &mut instruction.action {
                Action::Damage {
                    amount: DamageAmount::Uniform { amount },
                    ..
                } => self.mask_number(amount, false),
                Action::Resource { amount, .. } => self.mask_number(amount, true),
                Action::Life { change, .. } => self.mask_change(change),
                Action::Modify { patch, .. } => self.mask_patch(patch),
                _ => Ok(()),
            },
            _ => Ok(()),
        }
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
                receipt: receipt @ ObjectReceipt::Look { .. },
                projection,
                member,
                predicate,
            } => {
                if !self.published.contains(&local_receipt(receipt)?) {
                    return Err(unsupported("look condition without revelation"));
                }
                check_shape(receipt, *projection, *member, predicate)?;
                *condition = Expr::Literal(Bool(false));
            }
            Predicate::CompareInt { left, right, .. } => {
                self.mask_number(left, false)?;
                self.mask_number(right, false)?;
            }
            Predicate::ReceiptAny {
                receipt:
                    receipt @ ObjectReceipt::Movement {
                        receipt: MovementReceipt::Move { .. },
                    },
                projection,
                member,
                predicate,
            } => {
                let id = local_receipt(receipt)?;
                let field = self.field_cemetery.contains(&id);
                if !field && !self.moves.contains(&id) {
                    return Err(unsupported(
                        "receipt member condition outside a checked movement",
                    ));
                }
                if field {
                    check_original_predicate(predicate, *member)?;
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
    for view in &members {
        let card = view.card();
        if matches(predicate, member, card, Some((game, execution, view)))? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn check_count(
    receipt: &ObjectReceipt,
    projection: MemberProjection,
    member: Binding<RuntimeObjectView>,
    predicate: &Expr<Bool>,
) -> Result<()> {
    if matches!(receipt, ObjectReceipt::Fusion { .. }) {
        return check_shape(receipt, projection, member, predicate);
    }
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
    for view in &members {
        let card = view.card();
        if matches(predicate, member, card, Some((game, execution, view)))? {
            count = count
                .checked_add(1)
                .ok_or_else(|| invalid("receipt member count overflow"))?;
        }
    }
    Ok(Nat(count))
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
    original: Option<(&Game, &Execution, &MemberView)>,
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
        } if *threshold >= 0
            && matches!(comparison, Comparison::Equal | Comparison::GreaterEqual) =>
        {
            original_cost_matches(operation, member, *comparison, *threshold, original)
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
    comparison: Comparison,
    threshold: i64,
    original: Option<(&Game, &Execution, &MemberView)>,
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
    let Some((game, execution, view)) = original else {
        return Ok(false);
    };
    let cost = view.original_cost(game, execution)?;
    Ok(super::values::compare(cost, comparison, threshold))
}

fn check_original_predicate(
    predicate: &Expr<Bool>,
    member: Binding<RuntimeObjectView>,
) -> Result<()> {
    let Expr::Compute(predicate) = predicate else {
        return match predicate {
            Expr::Literal(_) => Ok(()),
            _ => Err(unsupported("field receipt original cost condition")),
        };
    };
    match predicate.as_ref() {
        Predicate::And { predicates } | Predicate::Or { predicates } => {
            for child in predicates {
                check_original_predicate(child, member)?;
            }
            Ok(())
        }
        Predicate::Not { predicate } => check_original_predicate(predicate, member),
        Predicate::CompareInt {
            left: Expr::Compute(operation),
            comparison,
            right: Expr::Literal(Int(threshold)),
        } if *threshold >= 0
            && matches!(comparison, Comparison::Equal | Comparison::GreaterEqual) =>
        {
            original_cost_matches(operation, member, *comparison, *threshold, None).map(|_| ())
        }
        _ => Err(unsupported("field receipt only retains original printing")),
    }
}
