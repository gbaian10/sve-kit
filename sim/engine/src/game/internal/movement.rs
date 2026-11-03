//! Physical movement and successful receipt projections; full replacement chains remain closed.

use super::{
    Execution, Fact, Game, InstructionOutcome, Result, int, invalid, player_id, unsupported,
};
use crate::game::Object;
use crate::ir::{
    Action, ActionPattern, BanishOutcome, Bool, Cause, CausePattern, DestroyOutcome, DurationEnd,
    Effect, EffectSource, Expr, Generation, Instruction, MemberProjection, MoveDestination,
    MoveMethod, MoveOutcome, MoveReason, MoveResult, MoveSpec, MovementReceipt, Nat, Node,
    ObjectOperand, ObjectReceipt, ObjectRef, OwnerZone, PlayerId, PlayerRef, PlayerRelation,
    Polarity, Position, PositiveNat, ReceiptId, ReceiptLocation, ResultMember, ResultMemberId,
    RuntimeObjectView, RuntimeZoneRef, SelectionView, Selector, SliceDirection, SnapshotId,
    TimeAnchor, ZoneKind,
};
use alloc::collections::{BTreeMap, BTreeSet};
use core::num::NonZeroU64;
use serde_json::json;

/// One authority-only fact. No hidden destination reference is delivered to a player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MovementFact {
    /// Original and actual destinations, positions and physical generations.
    pub movement: MoveResult,
    /// Zone occupied before the commit.
    pub from: RuntimeZoneRef,
    /// The performed action; Put(Cemetery) does not mean Destroy.
    pub method: MoveMethod,
    /// Cause of this commit, independent of its destination.
    pub cause: Cause,
    /// Controller immediately before leaving the zone.
    pub controller_before: PlayerId,
}

pub(super) fn check_action(action: &Action) -> Result<()> {
    match action {
        Action::Move {
            objects: Selector::Ordered { .. },
            ..
        }
        | Action::Banish {
            objects: Selector::Ordered { .. },
        } if deck_top(action).is_none() => Err(unsupported("fixed deck top movement")),
        Action::Move {
            objects,
            movement,
            reason,
        } if observed_move(action) => {
            if !observation_spine(objects, false)
                || *reason != MoveReason::Put
                || movement.position.is_some()
                || movement.order.is_some()
                || movement.initial_posture.is_some()
                || !movement.entry_effects.is_empty()
            {
                return Err(unsupported("observed move shape"));
            }
            Ok(())
        }
        Action::Move {
            movement,
            reason: MoveReason::Put,
            ..
        } if field_move(movement)
            && movement.position.is_none()
            && movement.order.is_none()
            && movement.initial_posture.is_none()
            && movement.entry_effects.is_empty() =>
        {
            Ok(())
        }
        Action::Move {
            movement,
            reason: MoveReason::Put,
            ..
        } if fixed_deck_destination(movement) => Ok(()),
        Action::Move { movement, .. }
            if !matches!(
                movement.to,
                MoveDestination::Owners {
                    zone: OwnerZone::Cemetery | OwnerZone::Banish | OwnerZone::Hand | OwnerZone::Ex
                }
            ) || movement.position.is_some()
                || movement.order.is_some()
                || movement.initial_posture.is_some()
                || !movement.entry_effects.is_empty() =>
        {
            Err(unsupported(
                "movement destination, placement or entry effects",
            ))
        }
        Action::InstallEffect {
            effect, duration, ..
        } => {
            if matches!(
                effect.as_ref(),
                Effect::Permission {
                    action: ActionPattern::Stand { .. },
                    ..
                }
            ) {
                return super::posture::check_install(action);
            }
            if matches!(effect.as_ref(), Effect::Continuous { .. }) {
                return Ok(());
            }
            if matches!(effect.as_ref(), Effect::CostModifier { .. }) {
                return super::cost::check_action(action);
            }
            if !matches!(duration.end, DurationEnd::EndResolution {})
                || !matches!(duration.anchor, TimeAnchor::Installation {})
                || duration.usage.is_some()
                || !duration.intervals.is_empty()
                || !matches!(
                    effect.as_ref(),
                    Effect::Permission {
                        action: ActionPattern::Destroy {
                            cause: CausePattern::Effect {
                                source: EffectSource::Any {}
                            },
                            ..
                        },
                        polarity: Polarity::Forbid,
                        condition: Expr::Literal(Bool(true)),
                        ..
                    }
                )
            {
                return Err(unsupported("permission predicate or lifetime"));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// A checked fixed top slice: another player's deck may only be put into its owner's cemetery.
pub(super) fn deck_top(action: &Action) -> Option<(&PlayerRef, u64)> {
    let (objects, own_only) = match action {
        Action::Banish { objects } => (objects, true),
        Action::Move {
            objects,
            movement,
            reason: MoveReason::Put,
        } if matches!(
            movement.to,
            MoveDestination::Owners {
                zone: OwnerZone::Cemetery | OwnerZone::Ex
            }
        ) && movement.position.is_none()
            && movement.order.is_none()
            && movement.initial_posture.is_none()
            && movement.entry_effects.is_empty() =>
        {
            (
                objects,
                movement.to
                    != (MoveDestination::Owners {
                        zone: OwnerZone::Cemetery,
                    }),
            )
        }
        _ => return None,
    };
    match objects {
        Selector::Ordered {
            zone,
            start: Position::Top {},
            count: Expr::Literal(Nat(count)),
            direction: SliceDirection::Forward,
        } if *count > 0
            && zone.kind == ZoneKind::Deck
            && (!own_only || zone.player == (PlayerRef::Controller {})) =>
        {
            Some((&zone.player, *count))
        }
        _ => None,
    }
}

pub(super) fn advance_entry(action: &Action) -> bool {
    matches!(action, Action::Move { objects: Selector::Zone {
        zone: ZoneKind::Evolve, players, view: SelectionView::Current {}, predicate, ..
    }, movement, reason: MoveReason::Put }
        if field_move(movement)
            && players.relation == PlayerRelation::SelfPlayer
            && players.relative_to == (PlayerRef::Controller {})
            && players.cardinality.minimum.0 == 1
            && players.cardinality.maximum == Some(Nat(1))
            && **predicate == Expr::Literal(Bool(true)))
}

pub(super) fn field_move(movement: &MoveSpec) -> bool {
    matches!(&movement.to, MoveDestination::Shared { zone } if zone.player == (PlayerRef::Controller {}) && zone.kind == ZoneKind::Field)
}

pub(super) fn observed_move(action: &Action) -> bool {
    matches!(action, Action::Move { objects, movement, .. }
        if matches!(&movement.to, MoveDestination::Shared { zone }
            if zone.player == (PlayerRef::Controller {}) && zone.kind == ZoneKind::Ex)
            || (movement.to == (MoveDestination::Owners { zone: OwnerZone::Cemetery })
                && observation_spine(objects, false)))
}

fn observation_spine(selector: &Selector, remainder: bool) -> bool {
    match selector {
        Selector::ReceiptMembers {
            receipt: ObjectReceipt::Look { .. },
            ..
        } => true,
        Selector::ReceiptMembers {
            receipt:
                ObjectReceipt::Movement {
                    receipt: MovementReceipt::Move { .. } | MovementReceipt::Order { .. },
                },
            projection: MemberProjection::Before,
        } => remainder,
        Selector::Difference { input, except } => {
            observation_spine(input, remainder) && observation_spine(except, true)
        }
        Selector::Union { sets } => sets.iter().all(|set| observation_spine(set, remainder)),
        _ => false,
    }
}

impl Execution {
    pub(super) fn successful_moves(
        &self,
        receipt: &MovementReceipt,
    ) -> Result<impl Iterator<Item = &MoveResult>> {
        let moves = match receipt {
            MovementReceipt::Move { receipt } => {
                match self.receipts.get(local_id(receipt.location())?) {
                    Some(InstructionOutcome::Move(outcome)) => &outcome.moved,
                    _ => return Err(invalid("missing move receipt")),
                }
            }
            MovementReceipt::Search { receipt } => {
                match self.receipts.get(local_id(receipt.location())?) {
                    Some(InstructionOutcome::Search(outcome)) => &outcome.moved,
                    _ => return Err(invalid("missing search receipt")),
                }
            }
            MovementReceipt::Discard { receipt } => {
                match self.receipts.get(local_id(receipt.location())?) {
                    Some(InstructionOutcome::Discard(outcome)) => &outcome.moves,
                    _ => return Err(invalid("missing discard receipt")),
                }
            }
            MovementReceipt::Destroy { receipt } => {
                match self.receipts.get(local_id(receipt.location())?) {
                    Some(InstructionOutcome::Destroy(outcome)) => &outcome.moves,
                    _ => return Err(invalid("missing destroy receipt")),
                }
            }
            MovementReceipt::Banish { receipt } => {
                match self.receipts.get(local_id(receipt.location())?) {
                    Some(InstructionOutcome::Banish(outcome)) => &outcome.moves,
                    _ => return Err(invalid("missing banish receipt")),
                }
            }
            _ => return Err(unsupported("movement receipt projection")),
        };
        Ok(moves.iter().filter(|movement| {
            movement.after.is_some_and(|after| {
                after.id == movement.before.id && after.generation != movement.before.generation
            }) && movement.actual_to.is_some()
        }))
    }

    /// Successful before members; a same-deck reorder keeps generations, so all ordered cards count.
    pub(super) fn movement_before(&self, receipt: &MovementReceipt) -> Result<Vec<ObjectRef>> {
        let MovementReceipt::Order { receipt } = receipt else {
            return Ok(self
                .successful_moves(receipt)?
                .map(|movement| movement.before)
                .collect());
        };
        match self.receipts.get(local_id(receipt.location())?) {
            Some(InstructionOutcome::Order(outcome)) => Ok(outcome
                .successful
                .iter()
                .filter_map(|member| member.before.map(|view| view.object))
                .collect()),
            _ => Err(invalid("missing order receipt")),
        }
    }
}

fn local_id<T>(location: &ReceiptLocation<T>) -> Result<&ReceiptId> {
    match location {
        ReceiptLocation::Local { id } => Ok(id),
        ReceiptLocation::Iteration { .. } => Err(unsupported("iteration movement receipt")),
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Movement shares the authoritative Game and its mutation primitive."
)]
impl Game {
    fn departing_evolution(&self, object: &Object) -> Result<Option<(String, u64)>> {
        let Some(link) = object
            .state
            .get("evolved_with")
            .filter(|value| !value.is_null())
        else {
            return Ok(None);
        };
        let form = self.object(
            link.as_str()
                .ok_or_else(|| invalid("invalid evolution link"))?,
        )?;
        let base_print = self.catalog.face(&object.card, 0)?;
        let form_print = self.catalog.face(&form.card, 0)?;
        if object.zone != "field"
            || form.zone != "evolution"
            || form.owner != object.owner
            || base_print["card_type"] != "フォロワー"
            || form_print["card_type"] != "フォロワー・エボルヴ"
            || base_print["name"] != form_print["name"]
            || form.state["face"] != 0_u8
            || form
                .state
                .get("evolved_with")
                .is_some_and(|value| !value.is_null())
            || form
                .state
                .get("links")
                .is_some_and(|value| !value.is_null())
            || self
                .state
                .objects
                .values()
                .filter(|base| base.state["evolved_with"] == form.id)
                .count()
                != 1
        {
            return Err(unsupported(
                "ordinary single evolution departure association",
            ));
        }
        form.generation
            .checked_add(1)
            .ok_or_else(|| invalid("evolution generation exhausted"))?;
        Ok(Some((form.id.clone(), form.generation)))
    }

    fn check_evolution_return(&self, departure: Option<&(String, u64)>) -> Result<()> {
        if let Some((form, generation)) = departure {
            let returned = self.object(form)?;
            if returned.zone != "evolve_deck"
                || returned.generation != generation.saturating_add(1)
                || returned.state["face_up"] != true
            {
                return Err(invalid(
                    "evolution rule return diverged from departure plan",
                ));
            }
        }
        Ok(())
    }

    fn owner_ex_has_room(&self, owner: &str, count: usize) -> Result<bool> {
        let available = usize::try_from(5_i64.saturating_sub(self.zone_count(owner, "ex")).max(0))
            .map_err(invalid)?;
        if available != 0 && count > available {
            return Err(unsupported("public EX movement needs capacity selection"));
        }
        Ok(available != 0)
    }

    fn departure_followup(
        &self,
        execution: &Execution,
        previous: &Object,
        zone: &str,
        has_evolution: bool,
    ) -> Result<Option<&'static str>> {
        let card_type = self.object_type(&previous.id)?;
        let resource_links = previous.state["links"].as_object().is_some_and(|links| {
            links.len() == 1 && (links.contains_key("出走") || links.contains_key("憑依"))
        }) && self.internal_resource_links_valid(&previous.id)?;
        let special = card_type.contains("トークン")
            || card_type.contains("アドバンス")
            || has_evolution
            || resource_links;
        if special {
            if execution.ability.is_none() {
                return Err(unsupported(
                    "special-card movement requires whole-card authority",
                ));
            }
            self.check_internal_resources()?;
        }
        if ((card_type.contains("トークン") || card_type.contains("アドバンス"))
            && !matches!(
                card_type,
                "フォロワー・アドバンス"
                    | "フォロワー・トークン"
                    | "アミュレット・トークン"
                    | "スペル・トークン"
            ))
            || (previous
                .state
                .get("links")
                .is_some_and(|value| !value.is_null())
                && !resource_links)
            || self
                .state
                .objects
                .values()
                .any(|item| item.state["equipped_to"] == previous.id)
        {
            return Err(unsupported(
                "movement requires linked or special-card rules",
            ));
        }
        let followup = if card_type == "フォロワー・アドバンス"
            && !matches!(zone, "field" | "ex" | "resolution" | "evolve_deck")
        {
            Some("evolve_deck")
        } else if matches!(
            card_type,
            "フォロワー・トークン" | "アミュレット・トークン" | "スペル・トークン"
        ) && !matches!(zone, "field" | "ex" | "resolution")
        {
            Some("void")
        } else {
            None
        };
        previous
            .generation
            .checked_add(if followup.is_some() { 2 } else { 1 })
            .ok_or_else(|| invalid("movement generation exhausted"))?;
        Ok(followup)
    }

    pub(super) fn internal_banish(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        objects: &Selector,
    ) -> Result<()> {
        let members = if let Some((player, count)) = deck_top(&instruction.action) {
            self.deck_top_members(execution, player, count)?
        } else {
            let members = execution.objects(self, objects)?;
            for member in &members {
                let object = self.object(execution.object_id(*member)?)?;
                if object.generation != member.generation.0 {
                    continue;
                }
                if !(object.zone == "hand"
                    && object.owner == super::seat_name(execution.context.controller)?
                    && object.controller == object.owner
                    && execution.is_private_hand_target(objects, *member))
                {
                    public_zone(&object.zone)?;
                }
            }
            members
        };
        let (moves, successful) = self.internal_move_members(
            execution,
            &members,
            "banish",
            ZoneKind::Banish,
            MoveMethod::Banish,
            false,
            None,
        )?;
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Banish(BanishOutcome { successful, moves }),
        );
        Ok(())
    }

    pub(super) fn internal_move(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        objects: &Selector,
        movement: Option<(&MoveSpec, MoveReason)>,
    ) -> Result<()> {
        if observed_move(&instruction.action) {
            return self.internal_observed_move(execution, instruction, objects);
        }
        let (zone, kind, method) = match movement {
            None => ("cemetery", ZoneKind::Cemetery, MoveMethod::Destroy),
            Some((spec, reason)) => {
                if field_move(spec) && reason == MoveReason::Put {
                    return self.internal_field_entry(execution, instruction, objects);
                }
                let MoveDestination::Owners { zone } = spec.to else {
                    return Err(unsupported("shared movement destination"));
                };
                let (zone, kind) = match zone {
                    OwnerZone::Cemetery => ("cemetery", ZoneKind::Cemetery),
                    OwnerZone::Banish => ("banish", ZoneKind::Banish),
                    OwnerZone::Hand => ("hand", ZoneKind::Hand),
                    OwnerZone::Ex => ("ex", ZoneKind::Ex),
                    OwnerZone::Deck => ("deck", ZoneKind::Deck),
                };
                if reason != MoveReason::Put {
                    return Err(unsupported("movement reason"));
                }
                (zone, kind, MoveMethod::Put)
            }
        };
        let top = deck_top(&instruction.action);
        let members = if let Some((player, count)) = top {
            self.deck_top_members(execution, player, count)?
        } else {
            execution.objects(self, objects)?
        };
        for &member in &members {
            let object = self.object(execution.object_id(member)?)?;
            if object.generation == member.generation.0 {
                let actor = super::seat_name(execution.context.controller)?;
                let private_hand_move = method == MoveMethod::Put
                    && matches!(kind, ZoneKind::Ex | ZoneKind::Deck)
                    && object.zone == "hand"
                    && object.owner == actor
                    && object.controller == actor
                    && execution.is_private_hand_target(objects, member);
                let private_observed_move = method == MoveMethod::Put
                    && matches!(kind, ZoneKind::Hand | ZoneKind::Ex | ZoneKind::Cemetery)
                    && object.zone == "deck"
                    && object.owner == actor
                    && object.controller == actor
                    && self
                        .state
                        .knowledge
                        .get(actor)
                        .is_some_and(|knowledge| knowledge.located.contains(&object.id))
                    && execution.is_private_look_target(objects, member);
                if kind == ZoneKind::Hand
                    && !matches!(object.zone.as_str(), "field" | "cemetery")
                    && !private_observed_move
                {
                    return Err(unsupported(
                        "only public field or cemetery departures to hand",
                    ));
                }
                let resolving_source = kind == ZoneKind::Ex
                    && object.zone == "resolution"
                    && object.id == execution.context.source
                    && member.generation == execution.context.generation
                    && execution.ability.is_some()
                    && self.object_type(&object.id)? == "スペル";
                if kind == ZoneKind::Ex
                    && top.is_none()
                    && !private_observed_move
                    && !private_hand_move
                    && !resolving_source
                    && !matches!(object.zone.as_str(), "field" | "cemetery" | "ex")
                {
                    return Err(unsupported("public field or cemetery movement to EX"));
                }
                if top.is_none()
                    && !resolving_source
                    && !private_observed_move
                    && !private_hand_move
                {
                    public_zone(&object.zone)?;
                }
            }
        }
        let (moves, successful) = self.internal_move_members(
            execution,
            &members,
            zone,
            kind,
            method,
            kind == ZoneKind::Ex,
            movement.and_then(|(spec, _)| spec.position),
        )?;
        let outcome = if method == MoveMethod::Destroy {
            InstructionOutcome::Destroy(DestroyOutcome { successful, moves })
        } else {
            InstructionOutcome::Move(MoveOutcome { moved: moves })
        };
        execution.receipts.insert(instruction.receipt, outcome);
        Ok(())
    }

    fn deck_top_members(
        &self,
        execution: &Execution,
        player: &PlayerRef,
        count: u64,
    ) -> Result<Vec<ObjectRef>> {
        let seat = super::seat_name(execution.player(self, player)?)?;
        self.zone_ids(seat, "deck")
            .iter()
            .take(usize::try_from(count).map_err(invalid)?)
            .map(|id| {
                let object = self.object(id)?;
                if object.zone != "deck" || object.owner != seat || object.controller != seat {
                    return Err(unsupported("deck top ownership"));
                }
                execution.reference(self, id)
            })
            .collect()
    }

    fn field_entry_has_room(
        &self,
        execution: &Execution,
        members: &[ObjectRef],
        objects: &Selector,
        seat: &str,
    ) -> Result<bool> {
        let mut incoming = 0_i64;
        for &member in members {
            let object = self.object(execution.object_id(member)?)?;
            if object.generation != member.generation.0 {
                continue;
            }
            let ordinary = matches!(self.object_type(&object.id)?, "フォロワー" | "アミュレット")
                && matches!(object.zone.as_str(), "cemetery" | "ex");
            let private_hand = object.zone == "hand"
                && object.owner == seat
                && object.controller == seat
                && execution.is_private_hand_target(objects, member)
                && matches!(self.object_type(&object.id)?, "フォロワー" | "アミュレット");
            let private_look = object.zone == "deck"
                && object.owner == seat
                && object.controller == seat
                && self
                    .state
                    .knowledge
                    .get(seat)
                    .is_some_and(|knowledge| knowledge.located.contains(&object.id))
                && execution.is_private_look_target(objects, member)
                && matches!(self.object_type(&object.id)?, "フォロワー" | "アミュレット");
            let advance = self.object_type(&object.id)? == "フォロワー・アドバンス"
                && matches!(object.zone.as_str(), "evolve_deck" | "ex");
            if !(ordinary || advance || private_look || private_hand)
                || object.owner != seat
                || object.controller != seat
                || object.state["face"] != 0_i64
            {
                return Err(unsupported("admitted controller field entry"));
            }
            incoming = incoming
                .checked_add(1)
                .ok_or_else(|| invalid("entry count overflow"))?;
        }
        if self
            .zone_count(seat, "field")
            .checked_add(incoming)
            .is_none_or(|count| count > 5)
        {
            if members.len() == 1
                && incoming == 1
                && (matches!(objects, Selector::Bound { .. })
                    || matches!(objects,
                Selector::Objects { objects } if matches!(objects.as_slice(),
                    [ObjectOperand::FollowEventMove { .. } | ObjectOperand::SourceHolder {}]))
                    || members
                        .first()
                        .is_some_and(|member| execution.is_private_look_target(objects, *member)))
            {
                return Ok(false);
            }
            return Err(unsupported("field entry needs capacity selection"));
        }
        Ok(true)
    }

    fn internal_field_entry(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        objects: &Selector,
    ) -> Result<()> {
        let members = if let Selector::Zone {
            zone: ZoneKind::Evolve,
            players,
            predicate,
            ..
        } = objects
        {
            // CR 9.2 grants no player action: only this admitted effect may put Advance cards.
            if players.relation != PlayerRelation::SelfPlayer
                || players.relative_to != (PlayerRef::Controller {})
                || **predicate != Expr::Literal(Bool(true))
            {
                return Err(unsupported("own evolve-deck Advance entry selector"));
            }
            self.zone_ids(
                super::seat_name(execution.context.controller)?,
                "evolve_deck",
            )
            .iter()
            .map(|id| execution.reference(self, id))
            .collect::<Result<Vec<_>>>()?
        } else {
            execution.objects(self, objects)?
        };
        let seat = super::seat_name(execution.context.controller)?;
        if matches!(objects, Selector::Objects { objects }
            if objects == &[ObjectOperand::SourceHolder {}])
        {
            for &member in &members {
                let object = self.object(execution.object_id(member)?)?;
                if object.zone == "ex"
                    && object.state["counters"]
                        .as_object()
                        .is_some_and(|counters| {
                            counters.values().any(|count| count.as_i64() != Some(0))
                        })
                {
                    return Err(unsupported("EX source counter carry into Field"));
                }
            }
        }
        if !self.field_entry_has_room(execution, &members, objects, seat)? {
            let before = *members
                .first()
                .ok_or_else(|| invalid("missing returning object"))?;
            execution.receipts.insert(
                instruction.receipt,
                InstructionOutcome::Move(MoveOutcome {
                    moved: vec![MoveResult {
                        before,
                        after: None,
                        requested_to: RuntimeZoneRef {
                            player: execution.context.controller,
                            kind: ZoneKind::Field,
                        },
                        requested_position: None,
                        actual_to: None,
                        actual_position: None,
                        replacements: vec![],
                    }],
                }),
            );
            return Ok(());
        }
        for &member in &members {
            let id = execution.object_id(member)?.to_owned();
            if self.object(&id)?.generation == member.generation.0
                && self.internal_ward_entry_choice(execution, &id)?
            {
                execution.todo.push_front(Node::Instruction {
                    instruction: Box::new(instruction.clone()),
                });
                return Ok(());
            }
        }
        let (moved, _) = self.internal_move_members(
            execution,
            &members,
            "field",
            ZoneKind::Field,
            MoveMethod::Put,
            false,
            None,
        )?;
        for member in &moved {
            if member.after.is_some() {
                self.internal_ward_entry_posture(execution, member.before)?;
            }
        }
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Move(MoveOutcome { moved }),
        );
        Ok(())
    }

    fn internal_observed_move(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        objects: &Selector,
    ) -> Result<()> {
        let player = execution.context.controller;
        let members = execution.observed_objects(self, objects, player)?;
        let seat = super::seat_name(player)?;
        for &member in &members {
            let id = execution.object_id(member)?;
            let current = self.object(id)?;
            if current.generation != member.generation.0
                || current.zone != "deck"
                || current.owner != seat
                || current.controller != seat
                || !self
                    .state
                    .knowledge
                    .get(seat)
                    .is_some_and(|knowledge| knowledge.located.contains(id))
            {
                return Err(unsupported(
                    "observed move requires current own-deck evidence",
                ));
            }
        }
        let Action::Move { movement: spec, .. } = &instruction.action else {
            return Err(unsupported("observed move action"));
        };
        if matches!(&spec.to, MoveDestination::Shared { zone } if zone.kind == ZoneKind::Ex) {
            let available =
                usize::try_from(5_i64.saturating_sub(self.zone_count(seat, "ex")).max(0))
                    .map_err(invalid)?;
            if available > 0 && members.len() > available {
                return Err(unsupported("observed move needs a capacity selection"));
            }
        }
        let moved = self.internal_search_move(execution, &members, &spec.to, player)?;
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Move(MoveOutcome { moved }),
        );
        Ok(())
    }

    #[expect(
        clippy::too_many_lines,
        clippy::too_many_arguments,
        clippy::cognitive_complexity,
        reason = "Planning, replacements, generations and receipts commit at one atomic boundary."
    )]
    pub(super) fn internal_move_members(
        &mut self,
        execution: &mut Execution,
        members: &[ObjectRef],
        zone: &str,
        kind: ZoneKind,
        method: MoveMethod,
        owner_ex: bool,
        position: Option<Position>,
    ) -> Result<(Vec<MoveResult>, Vec<ResultMember>)> {
        let old_deck = if kind == ZoneKind::Deck {
            self.state
                .objects
                .values()
                .filter(|object| object.zone == "deck")
                .map(|object| (object.id.clone(), object.owner.clone()))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let snapshot = SnapshotId(self.state.ir_epoch);
        let mut moves = Vec::new();
        let mut successful = Vec::new();
        let mut plans = Vec::new();
        for &before in members {
            let id = execution.object_id(before)?.to_owned();
            let previous = self.object(&id)?.clone();
            let requested_to = RuntimeZoneRef {
                player: if shared_destination(zone) && !owner_ex {
                    execution.context.controller
                } else {
                    player_id(&previous.owner)?
                },
                kind,
            };
            let result = MoveResult {
                before,
                after: None,
                requested_to,
                requested_position: position,
                actual_to: None,
                actual_position: None,
                replacements: Vec::new(),
            };
            moves.push(result);
            if previous.generation != before.generation.0 {
                continue;
            }
            if method == MoveMethod::Destroy && previous.zone != "field" {
                return Err(unsupported("off-field destruction"));
            }
            let from_kind = movement_origin(&previous.zone, method, zone)?;
            if previous.zone == zone
                || (method == MoveMethod::Destroy && execution.destroy_guards.contains(&before))
            {
                continue;
            }
            let evolution = self.departing_evolution(&previous)?;
            let followup =
                self.departure_followup(execution, &previous, zone, evolution.is_some())?;

            let stack_replaced = self.stack_replaces_departure(&id, zone)?;
            plans.push((
                moves.len().saturating_sub(1),
                id,
                previous,
                from_kind,
                followup,
                evolution,
                stack_replaced,
            ));
        }
        if owner_ex {
            let mut arrivals = BTreeMap::<String, usize>::new();
            for (_, _, previous, ..) in plans.iter().filter(|plan| !plan.6) {
                let count = arrivals.entry(previous.owner.clone()).or_default();
                *count = count.saturating_add(1);
            }
            for (owner, count) in arrivals {
                if !self.owner_ex_has_room(&owner, count)? {
                    plans.retain(|(_, _, previous, _, _, _, stack_replaced)| {
                        *stack_replaced || previous.owner != owner
                    });
                }
            }
        }
        if kind == ZoneKind::Deck {
            let mut owners = BTreeSet::new();
            if plans
                .iter()
                .filter(|plan| !plan.6)
                .any(|(_, _, previous, ..)| !owners.insert(&previous.owner))
            {
                return Err(unsupported(
                    "simultaneous deck placement needs an order choice",
                ));
            }
        }
        let ids = plans
            .iter()
            .map(|(_, id, _, _, _, _, _)| id.clone())
            .collect::<Vec<_>>();
        let departing_source = execution.source_before_move(self, &ids)?;
        let mut frame = execution.frame();
        if method == MoveMethod::Destroy {
            frame.values.insert("destroyed_subjects".into(), json!(ids));
        }
        if owner_ex {
            frame.values.insert("owner_destination".into(), json!(true));
        }
        if method == MoveMethod::Banish {
            // The ordinary banish event is emitted only for a banish instruction frame.
            frame
                .values
                .insert("banish_instruction".into(), json!(true));
        }
        let placement = position.map(|position| match position {
            Position::Top {} => json!(1_u8),
            Position::Bottom {} => json!("bottom"),
            Position::Index { index } => json!(index.0.get()),
        });
        if method == MoveMethod::Discard {
            self.discard(&ids, &frame)?;
        } else {
            self.move_objects(
                &ids,
                zone,
                if shared_destination(zone) && !owner_ex {
                    Some(super::seat_name(execution.context.controller)?)
                } else {
                    None
                },
                placement.as_ref(),
                &frame,
            )?;
        }
        if kind == ZoneKind::Hand {
            // As with draws, the public card stays known but its new hand slot is not located.
            for (_, id, previous, ..) in plans.iter().filter(|plan| !plan.6) {
                for (viewer, knowledge) in &mut self.state.knowledge {
                    if *viewer != previous.owner {
                        knowledge.located.remove(id);
                    }
                }
            }
        }
        self.state.ir_epoch = self
            .state
            .ir_epoch
            .checked_add(1)
            .ok_or_else(|| invalid("IR epoch exhausted"))?;
        let after_snapshot = SnapshotId(self.state.ir_epoch);
        let group = self.group();
        let mut source_departed = false;
        for (index, id, previous, from_kind, followup, evolution, stack_replaced) in plans {
            self.check_evolution_return(evolution.as_ref())?;
            let current = self.object(&id)?;
            if stack_replaced {
                check_stack_replacement(&previous, current)?;
                continue;
            }
            let mut after = execution.reference(self, &id)?;
            after.generation = Generation(previous.generation.saturating_add(1));
            if current.generation
                != previous
                    .generation
                    .saturating_add(if followup.is_some() { 2 } else { 1 })
                || current.zone != followup.unwrap_or(zone)
                || player_id(&self.object(&id)?.controller)?
                    != moves
                        .get(index)
                        .ok_or_else(|| invalid("missing movement proposal"))?
                        .requested_to
                        .player
            {
                return Err(invalid(
                    "movement commit diverged from the admitted proposal",
                ));
            }
            let result = moves
                .get_mut(index)
                .ok_or_else(|| invalid("missing movement result"))?;
            source_departed |= id == execution.context.source;
            result.after = Some(after);
            result.actual_to = Some(result.requested_to);
            if kind == ZoneKind::Deck && followup.is_none() {
                result.actual_position = self
                    .object_position(&id)?
                    .and_then(|offset| offset.checked_add(1).and_then(NonZeroU64::new))
                    .map(|deck_index| Position::Index {
                        index: PositiveNat(deck_index),
                    });
            }
            let controller_before = player_id(&previous.controller)?;
            execution.facts.push(Fact::Movement(Box::new(MovementFact {
                movement: result.clone(),
                from: RuntimeZoneRef {
                    player: controller_before,
                    kind: from_kind,
                },
                method,
                cause: Cause::Effect {
                    execution: execution.id,
                },
                controller_before,
            })));
            if matches!(
                method,
                MoveMethod::Destroy | MoveMethod::Banish | MoveMethod::Discard
            ) {
                successful.push(ResultMember {
                    id: ResultMemberId(u64::try_from(index).map_err(invalid)?),
                    before: Some(RuntimeObjectView {
                        object: result.before,
                        snapshot,
                    }),
                    // A rule return or erasure already replaced this endpoint.
                    after: followup.is_none().then_some(RuntimeObjectView {
                        object: after,
                        snapshot: after_snapshot,
                    }),
                });
            }
            if method == MoveMethod::Destroy {
                self.emit(
                    json!({"kind":"破壊","object":id,"source":frame.source}),
                    &frame.cause,
                    group,
                );
            }
        }
        execution.seal_source_departure(departing_source.filter(|_| source_departed));
        if kind == ZoneKind::Deck {
            self.invalidate_deck_observations(execution, &moves, &old_deck)?;
        }
        Ok((moves, successful))
    }

    fn invalidate_deck_observations(
        &mut self,
        execution: &mut Execution,
        moves: &[MoveResult],
        old_deck: &[(String, String)],
    ) -> Result<()> {
        let changed_players = moves
            .iter()
            .filter_map(|movement| movement.actual_to.map(|to| to.player))
            .collect::<Vec<_>>();
        if changed_players.is_empty() {
            return Ok(());
        }
        execution
            .look_viewers
            .retain(|_, viewer| !changed_players.contains(viewer));
        let changed_owners = changed_players
            .iter()
            .map(|player| super::seat_name(*player))
            .collect::<Result<Vec<_>>>()?;
        for knowledge in self.state.knowledge.values_mut() {
            for (id, owner) in old_deck {
                if changed_owners.contains(&owner.as_str()) {
                    knowledge.located.remove(id);
                }
            }
        }
        Ok(())
    }
}

fn fixed_deck_destination(movement: &MoveSpec) -> bool {
    movement.to
        == (MoveDestination::Owners {
            zone: OwnerZone::Deck,
        })
        && movement.position.is_some()
        && movement.order.is_none()
        && movement.initial_posture.is_none()
        && movement.entry_effects.is_empty()
}

fn public_zone(zone: &str) -> Result<ZoneKind> {
    match zone {
        "field" => Ok(ZoneKind::Field),
        "ex" => Ok(ZoneKind::Ex),
        "cemetery" => Ok(ZoneKind::Cemetery),
        "banish" => Ok(ZoneKind::Banish),
        _ => Err(unsupported("non-public movement origin")),
    }
}

fn check_stack_replacement(previous: &Object, current: &Object) -> Result<()> {
    if current.generation != previous.generation
        || current.zone != previous.zone
        || int(&current.state["counters"]["stack_counter"])
            != int(&previous.state["counters"]["stack_counter"]).saturating_sub(1)
    {
        return Err(invalid(
            "Stack replacement diverged from the admitted proposal",
        ));
    }
    Ok(())
}

fn movement_origin(zone: &str, method: MoveMethod, destination: &str) -> Result<ZoneKind> {
    match (zone, method, destination) {
        ("evolve_deck", MoveMethod::Put, "field") => Ok(ZoneKind::Evolve),
        ("resolution", MoveMethod::Put, "ex") => Ok(ZoneKind::Resolution),
        ("hand", MoveMethod::Discard, "cemetery")
        | ("hand", MoveMethod::Banish, "banish")
        | ("hand", MoveMethod::Put, "deck" | "ex" | "field") => Ok(ZoneKind::Hand),
        ("deck", MoveMethod::Put | MoveMethod::Banish, _) => Ok(ZoneKind::Deck),
        (zone, _, _) => public_zone(zone),
    }
}

fn shared_destination(zone: &str) -> bool {
    matches!(zone, "field" | "ex")
}
