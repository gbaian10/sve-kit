use super::{
    Arithmetic, Binding, Bool, Capture, CaptureRef, Comparison, DamageRecipient, Execution, Expr,
    Game, InstructionOutcome, Int, IntFrozen, IntOperation, IntRead, MeasuredReceipt, Nat,
    NatFrozen, NatOperation, NatRead, ObjectFrozen, ObjectOperand, ObjectRef, PlayerId, PlayerRef,
    PlayerRelation, PlayerResource, PlayerSet, Predicate, ReceiptLocation, RecipientSet,
    ResolvedRecipients, Result, RuntimeDamageRecipient, Selector, Slot, ZoneKind, contains, int,
    invalid, player_id, seat_name, unsupported,
};
use crate::ir::AttackEvent;
use crate::ir::events::attack::target_plan;
use crate::ir::{
    BoolFrozen, BoolRead, EntryMethod, EntryMethodFields, EntryOrigin, EntryOriginFields,
    ExecutionRef, PaymentReference, QuantityUnit, SpellCountTime, ValueEquality,
};
use crate::ir::{
    FollowPermission, MemberProjection, MovementEvent, MovementReceipt, ObjectProperty,
    ObjectReceipt, ObjectView, PlayerRead, ReceiptId, Role, SelectionView, SnapshotFields,
};
use crate::ir::{NumericProperty, Posture, PostureRead, Stat};
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
            PlayerRef::Bound { binding } => self.player_slot(*binding),
            PlayerRef::Controller {} => Ok(self.context.controller),
            PlayerRef::Owner {} => player_id(&game.object(&self.context.source)?.owner),
            PlayerRef::Active {} => player_id(game.active()),
            PlayerRef::ProposedDamageRecipient {} => {
                Err(unsupported("proposed damage recipient outside replacement"))
            }
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
                PlayerRead::Role { object: view, role } => {
                    let reference = match view {
                        ObjectView::Current { object } => self.object(game, object)?,
                        ObjectView::Selected { selection } => {
                            self.selected_object(game, *selection)?
                        }
                        _ => return Err(unsupported("player role view")),
                    };
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
                PlayerRead::Active {} => Err(unsupported("player role view")),
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

    pub(super) fn selected_object(
        &self,
        game: &Game,
        selection: Binding<Selector>,
    ) -> Result<ObjectRef> {
        let members = self.objects(
            game,
            &Selector::Bound {
                reference: selection,
            },
        )?;
        match members.as_slice() {
            [object] => Ok(*object),
            _ => Err(invalid("selected information requires one object")),
        }
    }

    pub(super) fn object(&self, game: &Game, object: &ObjectOperand) -> Result<ObjectRef> {
        match object {
            ObjectOperand::FollowEventMove {
                event: MovementEvent::LeaveField { event },
            } => {
                let (_, after) = self
                    .leave_move
                    .as_ref()
                    .filter(|(binding, _)| *binding == event.key())
                    .ok_or_else(|| unsupported("missing triggering movement authority"))?;
                Ok(*after)
            }
            ObjectOperand::Bound { binding } => {
                let Some(Slot::Objects { members, .. }) = self.slots.get(&binding.key()) else {
                    return Err(invalid("missing object binding producer"));
                };
                match members.as_slice() {
                    [reference] => Ok(*reference),
                    _ => Err(invalid("object binding requires one member")),
                }
            }
            ObjectOperand::Selected { selection }
                if self.targets.contains_key(&selection.key()) =>
            {
                self.selected_object(game, *selection)
            }
            ObjectOperand::Selected { selection } => {
                let members = self.object_slot(*selection)?;
                match members.as_slice() {
                    [reference] => Ok(*reference),
                    _ => Err(invalid("selected operand requires one object")),
                }
            }
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
                Expr::Frozen(_) => self
                    .event_object
                    .as_ref()
                    .filter(|(expression, _)| expression == value.as_ref())
                    .map(|(_, saved)| *saved)
                    .ok_or_else(|| unsupported("missing triggering object authority")),
                _ => Err(unsupported("object expression")),
            },
            _ => Err(unsupported("object operand")),
        }
    }

    fn attack_target_objects(
        &self,
        game: &Game,
        binding: Binding<AttackEvent>,
    ) -> Result<Vec<ObjectRef>> {
        let event = self
            .attack_event
            .filter(|(saved, _)| *saved == binding)
            .map(|(_, event)| event)
            .ok_or_else(|| unsupported("missing attack target authority"))?;
        let RuntimeDamageRecipient::Object { object } = event.target else {
            return Ok(Vec::new());
        };
        let id = self.object_id(object)?;
        let current = game.object(id)?;
        Ok(
            if current.generation == object.generation.0
                && current.zone == "field"
                && game.object_type(id)?.contains("フォロワー")
            {
                vec![object]
            } else {
                Vec::new()
            },
        )
    }

    fn unfiltered_zone_objects(
        &self,
        game: &Game,
        players: &PlayerSet,
        zone: ZoneKind,
        predicate: &Expr<Bool>,
    ) -> Result<Vec<ObjectRef>> {
        let mut objects = Vec::new();
        // This fallback's admitted predicates do not read a candidate binding.
        if self.boolean(game, predicate)?.0 {
            for player in self.players(game, players)? {
                for id in game.zone_ids(seat_name(player)?, zone_name(zone)?) {
                    objects.push(self.reference(game, &id)?);
                }
            }
        }
        Ok(objects)
    }

    pub(super) fn objects(&self, game: &Game, selector: &Selector) -> Result<Vec<ObjectRef>> {
        if let Some((binding, _)) = target_plan(selector) {
            return self.attack_target_objects(game, binding);
        }
        let objects = match selector {
            Selector::Zone {
                zone: ZoneKind::Field,
                ..
            } if super::card::selection::field_selector(selector).is_ok() => {
                self.public_field_objects(game, selector)?
            }
            Selector::Zone {
                zone: ZoneKind::Ex | ZoneKind::Cemetery | ZoneKind::Evolve,
                view: SelectionView::Current {},
                ..
            } if super::card::selection::public_zone_selector(selector).is_ok() => {
                self.public_zone_objects(game, selector)?
            }
            Selector::Filter { .. }
                if super::card::selection::public_zone_selector(selector).is_ok() =>
            {
                self.public_target_objects(game, selector, false)?
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
            } => self.unfiltered_zone_objects(game, players, *zone, predicate)?,
            Selector::Extremum {
                input,
                property,
                kind,
            } => super::aggregates::extremum(self, game, input, *property, *kind)?,
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
            } => self.movement_before(receipt)?,
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
            Selector::Intersection { sets } => self.intersection(game, sets)?,
            _ => return Err(unsupported("selector")),
        };
        let mut seen = BTreeSet::new();
        Ok(objects
            .into_iter()
            .filter(|object| seen.insert((object.id, object.generation)))
            .collect())
    }

    // Members compare by generation, so a returned object is not the one first selected.
    fn intersection(&self, game: &Game, sets: &[Selector]) -> Result<Vec<ObjectRef>> {
        let mut sets = sets.iter();
        let mut members = sets
            .next()
            .map(|set| self.objects(game, set))
            .transpose()?
            .unwrap_or_default();
        for set in sets {
            let other = self.objects(game, set)?;
            members.retain(|object| other.contains(object));
        }
        Ok(members)
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
            Selector::Captured {
                reference: CaptureRef { binding: reference },
            }
            | Selector::Bound { reference } => {
                if let Some(Slot::Objects {
                    on_field_at_capture: false,
                    ..
                }) = self.slots.get(&reference.key())
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
                IntRead::HistorySum { query } => super::history_turn::life_sum(self, game, query),
            },
            Expr::Frozen(frozen) => match frozen.as_ref() {
                IntFrozen::DamageAmount { event } => {
                    let (_, hit) = self
                        .damage_event
                        .filter(|(binding, _)| *binding == event.key())
                        .ok_or_else(|| invalid("missing damage event authority"))?;
                    Ok(Int(i64::try_from(hit.amount.0.get()).map_err(invalid)?))
                }
                IntFrozen::PaidMaterialAttack { receipt } => {
                    super::receipt_counts::paid_material_attack(self, receipt)
                }
                IntFrozen::ReceiptRequested { receipt } => {
                    self.quantity(receipt).map(|(requested, _)| requested)
                }
                IntFrozen::DiscardedCost { receipt } => {
                    super::receipt_counts::discarded_cost(self, receipt)
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
                IntOperation::Sum { objects, property } if target_plan(objects).is_some() => {
                    self.attack_target_sum(game, objects, *property).map(Int)
                }
                IntOperation::Sum { objects, property } => {
                    super::aggregates::sum(self, game, objects, *property).map(Int)
                }
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
            },
        }
    }

    fn attack_target_sum(
        &self,
        game: &Game,
        objects: &Selector,
        property: NumericProperty,
    ) -> Result<i64> {
        let stat = match property {
            NumericProperty::Attack => Stat::Attack,
            NumericProperty::Life => Stat::Life,
            NumericProperty::Count | NumericProperty::Cost | NumericProperty::OriginalCost => {
                return Err(unsupported("attack target numeric property"));
            }
        };
        self.objects(game, objects)?
            .into_iter()
            .try_fold(0_i64, |sum, object| {
                let value = game.internal_numeric_stat(self.object_id(object)?, stat)?;
                sum.checked_add(value)
                    .ok_or_else(|| invalid("numeric aggregate overflow"))
            })
    }

    fn live_natural(&self, game: &Game, read: &NatRead) -> Result<Nat> {
        match read {
            NatRead::TurnOrdinal { player } => {
                let seat = seat_name(self.player(game, player)?)?;
                game.state.turn["elapsed_turns"][seat]
                    .as_u64()
                    .map(Nat)
                    .ok_or_else(|| invalid("missing player turn ordinal"))
            }
            NatRead::Counter { holder, kind } => self.counter(game, holder, kind),
            NatRead::Combo { player, execution } => {
                super::history_turn::combo(self, game, player, *execution)
            }
            NatRead::HistoryCount { query, count } => {
                super::history_turn::subjects(self, game, query, *count)
            }
            NatRead::ZoneCount {
                selector:
                    Selector::Zone {
                        players,
                        zone: zone @ (ZoneKind::Hand | ZoneKind::Deck | ZoneKind::Banish),
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
                } else if *zone == ZoneKind::Deck {
                    "deck"
                } else {
                    "banish"
                };
                let count =
                    self.players(game, players)?
                        .into_iter()
                        .try_fold(0_u64, |count, player| {
                            let length = u64::try_from(game.zone(seat_name(player)?, zone).len())
                                .map_err(invalid)?;
                            count
                                .checked_add(length)
                                .ok_or_else(|| invalid("zone count overflow"))
                        })?;
                Ok(Nat(count))
            }
            NatRead::ZoneCount { selector } => Ok(Nat(u64::try_from(
                self.objects(game, selector)?.len(),
            )
            .map_err(invalid)?)),
            NatRead::ZoneCapacity { .. } => Err(unsupported("natural read")),
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
                NatFrozen::SnapshotCounter {
                    object: Expr::Frozen(view),
                    kind,
                } if matches!(view.as_ref(), SnapshotFields::SourceInformation {}) => {
                    self.source_counter(game, kind)
                }
                NatFrozen::ReceiptActual { receipt } => {
                    self.quantity(receipt).map(|(_, actual)| actual)
                }
                _ => Err(unsupported("natural projection")),
            },
            Expr::Live(read) => self.live_natural(game, read),
            Expr::Compute(operation) => match operation.as_ref() {
                NatOperation::Count { objects } => Ok(Nat(u64::try_from(
                    self.objects(game, objects)?.len(),
                )
                .map_err(invalid)?)),
                NatOperation::DistinctCount { objects, property } => {
                    let members = self.public_zone_objects(game, objects)?;
                    let count = match property {
                        ObjectProperty::Names => {
                            let mut names = BTreeSet::new();
                            for member in members {
                                names.extend(game.card_names(self.object_id(member)?)?);
                            }
                            names.len()
                        }
                        ObjectProperty::Cost => {
                            let mut costs = BTreeSet::new();
                            for member in members {
                                let printed = game.internal_cost_print(self.object_id(member)?)?;
                                let cost = printed
                                    .get("cost")
                                    .ok_or_else(|| invalid("missing printed cost"))?;
                                if !cost.is_null() && cost.as_str() != Some("-") {
                                    costs.insert(super::cost::printed_cost(cost)?);
                                }
                            }
                            costs.len()
                        }
                        _ => return Err(unsupported("distinct count property")),
                    };
                    Ok(Nat(u64::try_from(count).map_err(invalid)?))
                }
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
                NatOperation::ReceiptActualSum { .. } => Err(unsupported("natural operation")),
            },
        }
    }

    fn entered_origin(&self, value: &Expr<EntryOrigin>) -> Result<EntryOrigin> {
        match value {
            Expr::Literal(origin) => Ok(*origin),
            Expr::Frozen(field) => match field.as_ref() {
                EntryOriginFields::EnterFieldFrom { event } => self
                    .entry_origin
                    .filter(|(binding, _)| *binding == event.key())
                    .map(|(_, origin)| origin)
                    .ok_or_else(|| invalid("missing entry origin authority")),
                EntryOriginFields::EnterExFrom { .. } => {
                    Err(unsupported("entry origin event family"))
                }
            },
            _ => Err(unsupported("entry origin expression")),
        }
    }

    fn entered_method(&self, value: &Expr<EntryMethod>) -> Result<EntryMethod> {
        match value {
            Expr::Literal(method) => Ok(*method),
            Expr::Frozen(field) => match field.as_ref() {
                EntryMethodFields::EnterFieldMethod { event } => self
                    .entry_method
                    .filter(|(binding, _)| *binding == event.key())
                    .map(|(_, method)| method)
                    .ok_or_else(|| invalid("missing entry method authority")),
            },
            _ => Err(unsupported("entry method expression")),
        }
    }

    pub(in crate::game::internal) fn inline_material_handles(
        &self,
        members: &[ObjectRef],
        ids: &[String],
    ) -> Result<Vec<u64>> {
        ids.iter()
            .map(|id| {
                let index = members
                    .iter()
                    .position(|member| {
                        self.object_id(*member)
                            .is_ok_and(|candidate| candidate == id)
                    })
                    .ok_or_else(|| {
                        crate::EngineFailure::IllegalPlay("foreign cost material".into())
                    })?;
                u64::try_from(index).map_err(invalid)
            })
            .collect()
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
                Predicate::ReceiptAny {
                    receipt,
                    projection,
                    member,
                    predicate,
                } => super::receipt_counts::any(
                    game,
                    self,
                    receipt,
                    *projection,
                    *member,
                    predicate,
                )?,
                Predicate::Exists { objects } => !self.objects(game, objects)?.is_empty(),
                Predicate::PlayerEquals { left, right } => {
                    self.player_value(game, left)? == self.player_value(game, right)?
                }
                Predicate::Equal { operands } => self.event_equality(operands)?,
                Predicate::PostureEquals {
                    left: Expr::Live(read),
                    right: Expr::Literal(posture),
                } => self.posture_equals(game, read, *posture)?,
                _ => return Err(unsupported("predicate")),
            })),
            Expr::Live(read) => self.live_boolean(game, read),
        }
    }

    fn posture_equals(&self, game: &Game, read: &PostureRead, posture: Posture) -> Result<bool> {
        let PostureRead::Posture {
            object: ObjectView::Current { object },
        } = read
        else {
            return Err(unsupported("posture view"));
        };
        let reference = self.object(game, object)?;
        let current = game.object(self.object_id(reference)?)?;
        Ok(current.generation == reference.generation.0
            && current.zone == "field"
            && current.state["acted"].as_bool().unwrap_or(false) == (posture == Posture::Acted))
    }

    fn live_boolean(&self, game: &Game, read: &BoolRead) -> Result<Bool> {
        match read {
            BoolRead::FieldEntryOrigin {
                object: ObjectOperand::SourceHolder {},
                zone: ZoneKind::Cemetery,
            } => {
                let reference = self.object(game, &ObjectOperand::SourceHolder {})?;
                let current = game.object(self.object_id(reference)?)?;
                Ok(Bool(
                    current.zone == "field"
                        && current.generation == reference.generation.0
                        && current.state["entered_from"].as_str() == Some("cemetery"),
                ))
            }
            BoolRead::CardPlayOrigin {
                zone,
                execution: ExecutionRef::Current {},
            } if self.spell || self.field_entry => Ok(Bool(
                self.play_origin
                    .ok_or_else(|| invalid("missing card play origin"))?
                    == *zone,
            )),
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
        }
    }

    fn event_equality(&self, operands: &ValueEquality) -> Result<bool> {
        match operands {
            ValueEquality::EntryMethod { left, right } => {
                Ok(self.entered_method(left)? == self.entered_method(right)?)
            }
            ValueEquality::EntryOrigin { left, right } => {
                Ok(self.entered_origin(left)? == self.entered_origin(right)?)
            }
            _ => Err(unsupported("equality domain")),
        }
    }

    fn paid_quantity(
        &self,
        reference: &PaymentReference,
        unit: QuantityUnit,
    ) -> Result<(Int, Nat)> {
        let receipt = match reference {
            PaymentReference::Local { receipt } => *local_receipt(receipt.location())?,
            PaymentReference::CardPlay { face, receipt }
                if self.spell
                    && self
                        .card_payment
                        .as_ref()
                        .is_some_and(|(paid_face, paid_receipt)| {
                            paid_face == face && paid_receipt == receipt
                        }) =>
            {
                *receipt
            }
            PaymentReference::CardPlay { .. } | PaymentReference::Entry { .. } => {
                return Err(invalid("unavailable payment amount"));
            }
        };
        let Some(InstructionOutcome::ExchangePayment(payment)) = self.receipts.get(&receipt) else {
            return Err(invalid("missing payment amount"));
        };
        let mut amount = 0_u64;
        for step in &payment.steps {
            if step.execution != self.id {
                return Err(unsupported("foreign payment step"));
            }
            let count = match self.receipts.get(&step.slot) {
                Some(InstructionOutcome::Payment { quantity, .. }) if quantity.unit == unit => {
                    u64::try_from(quantity.amount.0).map_err(invalid)?
                }
                Some(InstructionOutcome::Payment { .. }) => 0,
                Some(InstructionOutcome::Discard(outcome)) if unit == QuantityUnit::Cards => {
                    u64::try_from(outcome.moves.len()).map_err(invalid)?
                }
                _ => return Err(unsupported("payment amount outcome")),
            };
            amount = amount
                .checked_add(count)
                .ok_or_else(|| invalid("payment amount overflow"))?;
        }
        Ok((Int(i64::try_from(amount).map_err(invalid)?), Nat(amount)))
    }

    fn quantity(&self, receipt: &MeasuredReceipt) -> Result<(Int, Nat)> {
        if let MeasuredReceipt::Payment { receipt, unit } = receipt {
            return self.paid_quantity(receipt, *unit);
        }
        let (id, damage) = match receipt {
            MeasuredReceipt::Cards { receipt } => (local_receipt(receipt.location())?, false),
            MeasuredReceipt::Damage { receipt } => (local_receipt(receipt.location())?, true),
            MeasuredReceipt::Counters { .. } | MeasuredReceipt::Payment { .. } => {
                return Err(unsupported("quantity receipt"));
            }
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
    if matches!(operation, Arithmetic::HalfUp) {
        if !rest.is_empty() {
            return Err(invalid("half-up requires one value"));
        }
        return first
            .div_euclid(2)
            .checked_add(first.rem_euclid(2))
            .ok_or_else(|| invalid("half-up overflow"));
    }
    rest.iter().try_fold(first, |left, &right| {
        match operation {
            Arithmetic::Add => left.checked_add(right),
            Arithmetic::Subtract => left.checked_sub(right),
            Arithmetic::Multiply => left.checked_mul(right),
            Arithmetic::Minimum => Some(left.min(right)),
            Arithmetic::Maximum => Some(left.max(right)),
            // 「切り捨て」: Euclidean division equals floor once the divisor is positive.
            Arithmetic::FloorDivide if right > 0 => Some(left.div_euclid(right)),
            Arithmetic::FloorDivide => return Err(invalid("non-positive divisor")),
            Arithmetic::HalfUp => None,
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
