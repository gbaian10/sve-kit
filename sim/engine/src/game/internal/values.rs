use super::{
    Arithmetic, Binding, Bool, Capture, CaptureRef, Comparison, DamageRecipient, Execution, Expr,
    Game, InstructionOutcome, Int, IntFrozen, IntOperation, IntRead, MeasuredReceipt, Nat,
    NatFrozen, NatOperation, NatRead, ObjectFrozen, ObjectOperand, ObjectRef, PlayerId, PlayerRef,
    PlayerRelation, PlayerResource, PlayerSet, Predicate, ReceiptLocation, RecipientSet,
    ResolvedRecipients, Result, RuntimeDamageRecipient, Selector, Slot, ZoneKind, contains, int,
    invalid, player_id, seat_name, unsupported,
};
use crate::ir::{BoolFrozen, BoolRead, ExecutionRef, PaymentReference, SpellCountTime};
use crate::ir::{
    FollowPermission, MemberProjection, MovementReceipt, ObjectReceipt, ObjectView, PlayerRead,
    ReceiptId, Role, SelectionView,
};
use alloc::collections::BTreeSet;
use core::convert::identity;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Value evaluation and ordering share the same execution slots."
)]
impl Execution {
    pub(super) fn player(&self, game: &Game, player: &PlayerRef) -> Result<PlayerId> {
        match player {
            PlayerRef::FromExpression { value } => self.player_value(game, value),
            PlayerRef::Bound { .. } => Err(unsupported("bound player value")),
            PlayerRef::Controller {} => Ok(self.context.controller),
            PlayerRef::Owner {} => player_id(&game.object(&self.context.source)?.owner),
            PlayerRef::Active {} => player_id(game.active()),
            PlayerRef::RequireOne { players } => {
                let values = self.players(game, players)?;
                if values.len() == 1 {
                    values
                        .first()
                        .copied()
                        .ok_or_else(|| invalid("missing player"))
                } else {
                    Err(invalid("player set is not a singleton"))
                }
            }
        }
    }

    fn player_slot(&self, binding: Binding<PlayerRef>) -> Result<PlayerId> {
        match self.slots.get(&binding.key()) {
            Some(Slot::Player(player)) => Ok(*player),
            _ => Err(invalid("missing player capture")),
        }
    }

    fn player_value(&self, game: &Game, value: &Expr<PlayerRef>) -> Result<PlayerId> {
        match value {
            Expr::Literal(player) => self.player(game, player),
            Expr::Captured(reference) => self.player_slot(reference.binding),
            Expr::Live(read) => match read.as_ref() {
                PlayerRead::Role {
                    object: ObjectView::Current { object: operand },
                    role,
                } => {
                    let reference = self.object(game, operand)?;
                    let id = self.object_id(reference)?;
                    let object = game.object(id)?;
                    if object.generation != reference.generation.0 {
                        return Err(unsupported("stale player role read requires LKI"));
                    }
                    player_id(match role {
                        Role::Controller => &object.controller,
                        Role::Owner => &object.owner,
                    })
                }
                _ => Err(unsupported("player role view")),
            },
            _ => Err(unsupported("player value")),
        }
    }

    pub(super) fn players(&self, game: &Game, players: &PlayerSet) -> Result<Vec<PlayerId>> {
        let relative = self.player(game, &players.relative_to)?;
        let values = match players.relation {
            PlayerRelation::SelfPlayer => vec![relative],
            PlayerRelation::Opponents => vec![PlayerId {
                seat: if relative.seat == 1 { 2 } else { 1 },
            }],
            PlayerRelation::All => vec![PlayerId { seat: 1 }, PlayerId { seat: 2 }],
            PlayerRelation::Bound => return Err(unsupported("bound player sets")),
        };
        if !contains(
            players.cardinality,
            Nat(u64::try_from(values.len()).map_err(invalid)?),
        ) {
            return Err(invalid("player set cardinality does not match this game"));
        }
        Ok(values)
    }

    pub(super) fn object(&self, game: &Game, object: &ObjectOperand) -> Result<ObjectRef> {
        match object {
            ObjectOperand::SourceHolder {} => {
                let mut reference = self.reference(game, &self.context.source)?;
                reference.generation = self.context.generation;
                Ok(reference)
            }
            ObjectOperand::FromExpression { value } => match value.as_ref() {
                Expr::Frozen(frozen)
                    if matches!(frozen.as_ref(), ObjectFrozen::SourceHolder {}) =>
                {
                    self.object(game, &ObjectOperand::SourceHolder {})
                }
                Expr::Frozen(frozen) => {
                    let ObjectFrozen::EnterFieldObject { event } = frozen.as_ref() else {
                        return Err(unsupported("object expression"));
                    };
                    self.entry_object
                        .filter(|(binding, _)| *binding == event.key())
                        .map(|(_, saved)| saved)
                        .ok_or_else(|| invalid("missing entry object authority"))
                }
                _ => Err(unsupported("object expression")),
            },
            _ => Err(unsupported("object operand")),
        }
    }

    pub(super) fn objects(&self, game: &Game, selector: &Selector) -> Result<Vec<ObjectRef>> {
        let objects = match selector {
            Selector::Zone {
                zone: ZoneKind::Field,
                ..
            } if super::card::selection::field_selector(selector).is_ok() => {
                self.public_field_objects(game, selector)?
            }
            Selector::Objects { objects } => objects
                .iter()
                .map(|object| self.object(game, object))
                .collect::<Result<Vec<_>>>()?,
            Selector::Zone {
                players,
                zone,
                predicate,
                ..
            } => {
                let mut objects = Vec::new();
                // Other admitted predicates do not read a candidate binding.
                if self.boolean(game, predicate)?.0 {
                    for player in self.players(game, players)? {
                        for id in game.zone_ids(seat_name(player)?, zone_name(*zone)?) {
                            objects.push(self.reference(game, &id)?);
                        }
                    }
                }
                objects
            }
            Selector::Union { sets } => sets
                .iter()
                .map(|set| self.objects(game, set))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect(),
            Selector::Captured { reference } => self.object_slot(reference.binding)?,
            Selector::Bound { reference } if self.targets.contains_key(&reference.key()) => {
                let members = self.object_slot(*reference)?;
                self.targets
                    .get(&reference.key())
                    .ok_or_else(|| invalid("missing target producer"))?
                    .check_current(game, self, &members)?;
                members
            }
            Selector::Bound { reference } => match self.slots.get(&reference.key()) {
                Some(Slot::Created(members)) => members.clone(),
                _ => return Err(invalid("missing token creation producer")),
            },
            Selector::ReceiptMembers {
                receipt: ObjectReceipt::Movement { receipt },
                projection: MemberProjection::Before,
            } => self
                .successful_moves(receipt)?
                .map(|movement| movement.before)
                .collect(),
            Selector::MoveAfter {
                receipt,
                permission: FollowPermission::SameEffectExplicitReference {},
            } => {
                let moves = self.successful_moves(receipt)?.collect::<Vec<_>>();
                // A bounced card's new hand generation must not become trackable (second batch §5).
                if matches!(receipt, MovementReceipt::Move { .. })
                    && moves.iter().any(|movement| {
                        movement
                            .actual_to
                            .is_some_and(|to| to.kind == ZoneKind::Hand)
                    })
                {
                    return Err(unsupported("hidden hand after follow"));
                }
                moves
                    .into_iter()
                    .filter_map(|movement| movement.after)
                    .collect()
            }
            Selector::Difference { input, except } => {
                let input = self.objects(game, input)?;
                let except = self.objects(game, except)?;
                input
                    .into_iter()
                    .filter(|object| !except.contains(object))
                    .collect()
            }
            _ => return Err(unsupported("selector")),
        };
        let mut seen = BTreeSet::new();
        Ok(objects
            .into_iter()
            .filter(|object| seen.insert((object.id, object.generation)))
            .collect())
    }

    fn object_slot(&self, binding: Binding<Selector>) -> Result<Vec<ObjectRef>> {
        if let Some(Slot::Objects { members, .. }) = self.slots.get(&binding.key()) {
            Ok(members.clone())
        } else {
            Err(invalid("missing typed object binding"))
        }
    }

    pub(super) fn recipients(
        &self,
        game: &Game,
        recipients: &RecipientSet,
    ) -> Result<ResolvedRecipients> {
        let values = match recipients {
            RecipientSet::Objects { objects } => {
                self.check_damage_capture(objects)?;
                self.objects(game, objects)?
                    .into_iter()
                    .map(|object| RuntimeDamageRecipient::Object { object })
                    .collect()
            }
            RecipientSet::Leaders { players } => self
                .players(game, players)?
                .into_iter()
                .map(|player| RuntimeDamageRecipient::Leader { player })
                .collect(),
            RecipientSet::Union { sets } => sets
                .iter()
                .map(|set| self.recipients(game, set))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flat_map(|set| set.members().to_vec())
                .collect(),
            RecipientSet::Explicit { recipients } => recipients
                .iter()
                .map(|recipient| match recipient {
                    DamageRecipient::Object { object } => Ok(RuntimeDamageRecipient::Object {
                        object: self.object(game, object)?,
                    }),
                    DamageRecipient::Leader { player } => Ok(RuntimeDamageRecipient::Leader {
                        player: self.player(game, player)?,
                    }),
                    _ => Err(unsupported("damage recipient")),
                })
                .collect::<Result<Vec<_>>>()?,
            RecipientSet::Bound { recipients } => match self.slots.get(&recipients.key()) {
                Some(Slot::Recipients {
                    members,
                    characteristics,
                }) => {
                    for member in members {
                        if let RuntimeDamageRecipient::Object { object } = member {
                            let id = self.object_id(*object)?;
                            let current = game.object(id)?;
                            if current.generation != object.generation.0 {
                                continue;
                            }
                            let saved = characteristics
                                .iter()
                                .find(|(reference, _)| reference == object)
                                .ok_or_else(|| invalid("missing recipient target producer"))?;
                            if current.zone != "field"
                                || super::card::selection::characteristics(game, id)? != saved.1
                            {
                                return Err(unsupported(
                                    "selected recipient target characteristics changed",
                                ));
                            }
                        }
                    }
                    members.clone()
                }
                _ => return Err(invalid("missing typed recipient binding")),
            },
        };
        Ok(ResolvedRecipients::new(values))
    }

    fn check_damage_capture(&self, selector: &Selector) -> Result<()> {
        match selector {
            Selector::Captured { reference } => {
                if let Some(Slot::Objects {
                    on_field_at_capture: false,
                    ..
                }) = self.slots.get(&reference.binding.key())
                {
                    return Err(unsupported("non-field damage capture"));
                }
            }
            Selector::Union { sets } => {
                for set in sets {
                    self.check_damage_capture(set)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn captures(&mut self, game: &Game, captures: &[Capture]) -> Result<()> {
        let entry = self.clone();
        let mut values = BTreeSet::new();
        for capture in captures {
            let mut isolated = entry.clone();
            isolated.capture(game, capture)?;
            for (key, value) in isolated.slots {
                if !entry.slots.contains_key(&key) {
                    if !values.insert(key) {
                        return Err(invalid("duplicate capture slot"));
                    }
                    self.slots.insert(key, value);
                }
            }
        }
        Ok(())
    }

    pub(super) fn capture(&mut self, game: &Game, capture: &Capture) -> Result<()> {
        let (key, value) = match capture {
            Capture::Integer { value, into, .. } => (
                into.binding.key(),
                Slot::Integer(self.integer(game, value)?),
            ),
            Capture::Natural { value, into, .. } => (
                into.binding.key(),
                Slot::Natural(self.natural(game, value)?),
            ),
            Capture::Boolean { value, into, .. } => (
                into.binding.key(),
                Slot::Boolean(self.boolean(game, value)?),
            ),
            Capture::Player { value, into, .. } => (
                into.binding.key(),
                Slot::Player(self.player_value(game, value)?),
            ),
            Capture::Objects { value, into, .. } => {
                let members = self.objects(game, value)?;
                let on_field_at_capture =
                    members
                        .iter()
                        .try_fold(true, |on_field, &object| -> Result<bool> {
                            let current = game.object(self.object_id(object)?)?;
                            Ok(on_field
                                && current.zone == "field"
                                && current.generation == object.generation.0)
                        })?;
                (
                    into.binding.key(),
                    Slot::Objects {
                        members,
                        on_field_at_capture,
                    },
                )
            }
            _ => return Err(unsupported("capture domain")),
        };
        self.slots.insert(key, value);
        Ok(())
    }

    pub(super) fn integer(&self, game: &Game, expr: &Expr<Int>) -> Result<Int> {
        match expr {
            Expr::Literal(value) => Ok(*value),
            Expr::Bound(binding) | Expr::Captured(CaptureRef { binding }) => {
                if let Some(Slot::Integer(value)) = self.slots.get(&binding.key()) {
                    Ok(*value)
                } else {
                    Err(invalid("missing signed binding"))
                }
            }
            Expr::Live(read) => match read.as_ref() {
                IntRead::PlayerResource { player, property } => {
                    let player = game.player(seat_name(self.player(game, player)?)?)?;
                    Ok(Int(match property {
                        PlayerResource::Life => int(&player.leader["life"]),
                        PlayerResource::PpCurrent => int(&player.pp["current"]),
                        PlayerResource::PpMaximum => int(&player.pp["max"]),
                        PlayerResource::Ep => player.ep,
                        PlayerResource::Sep => player.sep,
                    }))
                }
                IntRead::HistorySum { .. } => Err(unsupported("signed read")),
            },
            Expr::Frozen(frozen) => match frozen.as_ref() {
                IntFrozen::DamageAmount { event } => {
                    let (_, hit) = self
                        .damage_event
                        .filter(|(binding, _)| *binding == event.key())
                        .ok_or_else(|| invalid("missing damage event authority"))?;
                    Ok(Int(i64::try_from(hit.amount.0.get()).map_err(invalid)?))
                }
                IntFrozen::ReceiptRequested { receipt } => {
                    self.quantity(receipt).map(|(requested, _)| requested)
                }
                _ => Err(unsupported("signed projection")),
            },
            Expr::Compute(operation) => match operation.as_ref() {
                IntOperation::InformationOrZero { information } => {
                    self.number_information(game, information).map(Int)
                }
                IntOperation::FromNat { value } => Ok(Int(i64::try_from(
                    self.natural(game, value)?.0,
                )
                .map_err(invalid)?)),
                IntOperation::Conditional {
                    condition,
                    then_value,
                    else_value,
                } => self.integer(
                    game,
                    if self.boolean(game, condition)?.0 {
                        then_value
                    } else {
                        else_value
                    },
                ),
                IntOperation::Arithmetic {
                    operation,
                    arguments,
                } => {
                    let values = arguments
                        .iter()
                        .map(|argument| self.integer(game, argument).map(|value| value.0))
                        .collect::<Result<Vec<_>>>()?;
                    arithmetic(*operation, &values).map(Int)
                }
                IntOperation::Sum { .. } => Err(unsupported("signed operation")),
            },
        }
    }

    pub(super) fn natural(&self, game: &Game, expr: &Expr<Nat>) -> Result<Nat> {
        match expr {
            Expr::Literal(value) => Ok(*value),
            Expr::Bound(binding) | Expr::Captured(CaptureRef { binding }) => {
                if let Some(Slot::Natural(value)) = self.slots.get(&binding.key()) {
                    Ok(*value)
                } else {
                    Err(invalid("missing natural binding"))
                }
            }
            Expr::Frozen(frozen) => match frozen.as_ref() {
                NatFrozen::SpellChain {
                    player,
                    execution: ExecutionRef::Current {},
                    at: SpellCountTime::AtPlay,
                } => {
                    let seat = self.player(game, player)?.seat;
                    self.play_counts
                        .get(&seat)
                        .map(|counts| counts.0)
                        .ok_or_else(|| invalid("missing AtPlay spell counts"))
                }
                NatFrozen::NecroCharge {
                    player,
                    execution: ExecutionRef::Current {},
                    at: SpellCountTime::AtPlay,
                } => {
                    let seat = self.player(game, player)?.seat;
                    self.play_counts
                        .get(&seat)
                        .map(|counts| counts.1)
                        .ok_or_else(|| invalid("missing AtPlay cemetery count"))
                }
                NatFrozen::ReceiptActual { receipt } => {
                    self.quantity(receipt).map(|(_, actual)| actual)
                }
                _ => Err(unsupported("natural projection")),
            },
            Expr::Live(read) => match read.as_ref() {
                NatRead::ZoneCount {
                    selector:
                        Selector::Zone {
                            players,
                            zone: zone @ (ZoneKind::Hand | ZoneKind::Deck),
                            view: SelectionView::Current {},
                            predicate,
                            ..
                        },
                } => {
                    if **predicate != Expr::Literal(Bool(true)) {
                        return Err(unsupported("filtered hidden zone count"));
                    }
                    let zone = if *zone == ZoneKind::Hand {
                        "hand"
                    } else {
                        "deck"
                    };
                    let count = self.players(game, players)?.into_iter().try_fold(
                        0_u64,
                        |count, player| {
                            let length = u64::try_from(game.zone(seat_name(player)?, zone).len())
                                .map_err(invalid)?;
                            count
                                .checked_add(length)
                                .ok_or_else(|| invalid("zone count overflow"))
                        },
                    )?;
                    Ok(Nat(count))
                }
                NatRead::ZoneCount { selector } => Ok(Nat(u64::try_from(
                    self.objects(game, selector)?.len(),
                )
                .map_err(invalid)?)),
                _ => Err(unsupported("natural read")),
            },
            Expr::Compute(operation) => match operation.as_ref() {
                NatOperation::Count { objects } => Ok(Nat(u64::try_from(
                    self.objects(game, objects)?.len(),
                )
                .map_err(invalid)?)),
                NatOperation::CheckedInt { value } => Ok(Nat(u64::try_from(
                    self.integer(game, value)?.0,
                )
                .map_err(invalid)?)),
                NatOperation::Conditional {
                    condition,
                    then_value,
                    else_value,
                } => self.natural(
                    game,
                    if self.boolean(game, condition)?.0 {
                        then_value
                    } else {
                        else_value
                    },
                ),
                _ => Err(unsupported("natural operation")),
            },
        }
    }

    pub(super) fn boolean(&self, game: &Game, expr: &Expr<Bool>) -> Result<Bool> {
        match expr {
            Expr::Literal(value) => Ok(*value),
            Expr::Bound(binding) | Expr::Captured(CaptureRef { binding }) => {
                if let Some(Slot::Boolean(value)) = self.slots.get(&binding.key()) {
                    Ok(*value)
                } else {
                    Err(invalid("missing boolean binding"))
                }
            }
            Expr::Frozen(read) => match read.as_ref() {
                BoolFrozen::PaymentComplete {
                    receipt: PaymentReference::Local { receipt },
                } => {
                    let ReceiptLocation::Local { id } = receipt.location() else {
                        return Err(unsupported("iterated optional payment"));
                    };
                    match self.receipts.get(id) {
                        Some(InstructionOutcome::ExchangePayment(payment)) => Ok(payment.complete),
                        _ => Err(invalid("missing optional payment result")),
                    }
                }
                BoolFrozen::PaymentComplete {
                    receipt: PaymentReference::CardPlay { face, receipt },
                } => match &self.card_payment {
                    Some((paid_face, paid_receipt))
                        if self.spell && paid_face == face && paid_receipt == receipt =>
                    {
                        match self.receipts.get(paid_receipt) {
                            Some(InstructionOutcome::ExchangePayment(payment)) => {
                                Ok(payment.complete)
                            }
                            _ => Err(invalid("missing card payment result")),
                        }
                    }
                    _ => Err(invalid("unavailable card play payment")),
                },
                _ => Err(unsupported("boolean receipt")),
            },
            Expr::Compute(operation) => Ok(Bool(match operation.as_ref() {
                Predicate::And { predicates } => predicates
                    .iter()
                    .map(|predicate| self.boolean(game, predicate).map(|value| value.0))
                    .collect::<Result<Vec<_>>>()?
                    .into_iter()
                    .all(identity),
                Predicate::Or { predicates } => predicates
                    .iter()
                    .map(|predicate| self.boolean(game, predicate).map(|value| value.0))
                    .collect::<Result<Vec<_>>>()?
                    .into_iter()
                    .any(identity),
                Predicate::Not { predicate } => !self.boolean(game, predicate)?.0,
                Predicate::CompareInt {
                    left,
                    comparison,
                    right,
                } => compare(
                    self.integer(game, left)?.0,
                    *comparison,
                    self.integer(game, right)?.0,
                ),
                Predicate::CompareNat {
                    left,
                    comparison,
                    right,
                } => compare(
                    self.natural(game, left)?.0,
                    *comparison,
                    self.natural(game, right)?.0,
                ),
                Predicate::Exists { objects } => !self.objects(game, objects)?.is_empty(),
                _ => return Err(unsupported("predicate")),
            })),
            Expr::Live(read) => match read.as_ref() {
                BoolRead::Crimson { player } => {
                    let seat = seat_name(self.player(game, player)?)?;
                    Ok(Bool(
                        game.active() == seat
                            && game
                                .state
                                .counters
                                .get(&format!("{seat}.leader_hp_decreased"))
                                .is_some_and(|count| *count > 0),
                    ))
                }
                _ => Err(unsupported("boolean expression")),
            },
        }
    }

    fn quantity(&self, receipt: &MeasuredReceipt) -> Result<(Int, Nat)> {
        let (id, damage) = match receipt {
            MeasuredReceipt::Cards { receipt } => (local_receipt(receipt.location())?, false),
            MeasuredReceipt::Damage { receipt } => (local_receipt(receipt.location())?, true),
            MeasuredReceipt::Counters { .. } => return Err(unsupported("quantity receipt")),
        };
        match (self.receipts.get(id), damage) {
            (Some(InstructionOutcome::Cards(outcome)), false) => {
                Ok((outcome.requested, outcome.actual))
            }
            (Some(InstructionOutcome::Damage(outcome)), true) => {
                Ok((outcome.requested, outcome.actual))
            }
            _ => Err(invalid("missing or mismatched typed quantity receipt")),
        }
    }
}

fn local_receipt<T>(location: &ReceiptLocation<T>) -> Result<&ReceiptId> {
    let ReceiptLocation::Local { id } = location else {
        return Err(unsupported("iteration receipt"));
    };
    Ok(id)
}

fn arithmetic(operation: Arithmetic, values: &[i64]) -> Result<i64> {
    let (&first, rest) = values
        .split_first()
        .ok_or_else(|| invalid("empty arithmetic"))?;
    rest.iter().try_fold(first, |left, &right| {
        match operation {
            Arithmetic::Add => left.checked_add(right),
            Arithmetic::Subtract => left.checked_sub(right),
            Arithmetic::Multiply => left.checked_mul(right),
            Arithmetic::Minimum => Some(left.min(right)),
            Arithmetic::Maximum => Some(left.max(right)),
            _ => None,
        }
        .ok_or_else(|| invalid("signed arithmetic overflow"))
    })
}
pub(super) fn compare<T>(left: T, comparison: Comparison, right: T) -> bool
where
    T: Ord + Copy,
{
    match comparison {
        Comparison::Equal => left == right,
        Comparison::NotEqual => left != right,
        Comparison::Less => left < right,
        Comparison::LessEqual => left <= right,
        Comparison::Greater => left > right,
        Comparison::GreaterEqual => left >= right,
    }
}
fn zone_name(zone: ZoneKind) -> Result<&'static str> {
    match zone {
        ZoneKind::Field => Ok("field"),
        ZoneKind::Ex => Ok("ex"),
        ZoneKind::Cemetery => Ok("cemetery"),
        ZoneKind::Banish => Ok("banish"),
        _ => Err(unsupported("hidden or unsupported selector zone")),
    }
}
