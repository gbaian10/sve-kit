//! Physical movement and successful receipt projections; full replacement chains remain closed.

use super::{Execution, Fact, Game, InstructionOutcome, Result, invalid, player_id, unsupported};
use crate::ir::{
    Action, ActionPattern, BanishOutcome, Bool, Cause, CausePattern, DestroyOutcome, DurationEnd,
    Effect, EffectSource, Expr, Generation, Instruction, MoveDestination, MoveMethod, MoveOutcome,
    MoveReason, MoveResult, MoveSpec, MovementReceipt, Nat, Node, ObjectReceipt, ObjectRef,
    OwnerZone, PlayerId, PlayerRef, PlayerRelation, Polarity, ReceiptId, ReceiptLocation,
    ResultMember, ResultMemberId, RuntimeObjectView, RuntimeZoneRef, SelectionView, Selector,
    SnapshotId, TimeAnchor, ZoneKind,
};
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
            objects,
            movement,
            reason,
        } if observed_move(action) => {
            if !observation_spine(objects)
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
        Action::Move { movement, .. }
            if !matches!(
                movement.to,
                MoveDestination::Owners {
                    zone: OwnerZone::Cemetery | OwnerZone::Banish | OwnerZone::Hand
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
    matches!(action, Action::Move { movement, .. }
        if matches!(&movement.to, MoveDestination::Shared { zone }
            if zone.player == (PlayerRef::Controller {}) && zone.kind == ZoneKind::Ex))
}

fn observation_spine(selector: &Selector) -> bool {
    match selector {
        Selector::ReceiptMembers {
            receipt: ObjectReceipt::Look { .. },
            ..
        } => true,
        Selector::Difference { input, except } => {
            observation_spine(input) && observation_spine(except)
        }
        Selector::Union { sets } => sets.iter().all(observation_spine),
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
    pub(super) fn internal_banish(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        objects: &Selector,
    ) -> Result<()> {
        let members = execution.objects(self, objects)?;
        let (moves, successful) = self.internal_move_members(
            execution,
            &members,
            "banish",
            ZoneKind::Banish,
            MoveMethod::Banish,
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
                    OwnerZone::Deck => return Err(unsupported("private movement destination")),
                };
                if reason != MoveReason::Put {
                    return Err(unsupported("movement reason"));
                }
                (zone, kind, MoveMethod::Put)
            }
        };
        let members = execution.objects(self, objects)?;
        for &member in &members {
            let object = self.object(execution.object_id(member)?)?;
            if object.generation == member.generation.0 {
                if kind == ZoneKind::Hand && object.zone != "field" {
                    return Err(unsupported("only public field departures to hand"));
                }
                public_zone(&object.zone)?;
            }
        }
        let (moves, successful) =
            self.internal_move_members(execution, &members, zone, kind, method)?;
        let outcome = if method == MoveMethod::Destroy {
            InstructionOutcome::Destroy(DestroyOutcome { successful, moves })
        } else {
            InstructionOutcome::Move(MoveOutcome { moved: moves })
        };
        execution.receipts.insert(instruction.receipt, outcome);
        Ok(())
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
        let mut incoming = 0_i64;
        for &member in &members {
            let object = self.object(execution.object_id(member)?)?;
            if object.generation != member.generation.0 {
                continue;
            }
            let ordinary = self.object_type(&object.id)? == "フォロワー"
                && matches!(object.zone.as_str(), "cemetery" | "ex");
            let advance = self.object_type(&object.id)? == "フォロワー・アドバンス"
                && matches!(object.zone.as_str(), "evolve_deck" | "ex");
            if !(ordinary || advance)
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
            return Err(unsupported("field entry needs capacity selection"));
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
        let available = usize::try_from(5_i64.saturating_sub(self.zone_count(seat, "ex")).max(0))
            .map_err(invalid)?;
        if available > 0 && members.len() > available {
            return Err(unsupported("observed move needs a capacity selection"));
        }
        let Action::Move { movement: spec, .. } = &instruction.action else {
            return Err(unsupported("observed move action"));
        };
        let moved = self.internal_search_move(execution, &members, &spec.to, player)?;
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Move(MoveOutcome { moved }),
        );
        Ok(())
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Planning and receipt construction share the atomic movement boundary."
    )]
    pub(super) fn internal_move_members(
        &mut self,
        execution: &mut Execution,
        members: &[ObjectRef],
        zone: &str,
        kind: ZoneKind,
        method: MoveMethod,
    ) -> Result<(Vec<MoveResult>, Vec<ResultMember>)> {
        let snapshot = SnapshotId(self.state.ir_epoch);
        let mut moves = Vec::new();
        let mut successful = Vec::new();
        let mut plans = Vec::new();
        for &before in members {
            let id = execution.object_id(before)?.to_owned();
            let previous = self.object(&id)?.clone();
            let requested_to = RuntimeZoneRef {
                player: if zone == "field" {
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
                requested_position: None,
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
            let from_kind =
                if method == MoveMethod::Put && previous.zone == "evolve_deck" && zone == "field" {
                    ZoneKind::Evolve
                } else if previous.zone == "deck" && method == MoveMethod::Put {
                    ZoneKind::Deck
                } else {
                    public_zone(&previous.zone)?
                };
            if previous.zone == zone
                || (method == MoveMethod::Destroy && execution.destroy_guards.contains(&before))
            {
                continue;
            }
            // Linked departures still need their own typed rule receipts.
            let card_type = self.object_type(&id)?;
            let special = card_type.contains("トークン") || card_type.contains("アドバンス");
            if special {
                if execution.ability.is_none() {
                    return Err(unsupported(
                        "special-card movement requires whole-card authority",
                    ));
                }
                self.check_internal_resources()?;
            }
            if ((card_type.contains("トークン") || card_type.contains("アドバンス"))
                && !matches!(card_type, "フォロワー・アドバンス" | "フォロワー・トークン"))
                || previous
                    .state
                    .get("evolved_with")
                    .is_some_and(|value| !value.is_null())
                || previous
                    .state
                    .get("links")
                    .is_some_and(|value| !value.is_null())
                || self
                    .state
                    .objects
                    .values()
                    .any(|item| item.state["equipped_to"] == id)
            {
                return Err(unsupported(
                    "movement requires linked or special-card rules",
                ));
            }
            let followup = if card_type == "フォロワー・アドバンス"
                && !matches!(zone, "field" | "ex" | "resolution" | "evolve_deck")
            {
                Some("evolve_deck")
            } else if card_type == "フォロワー・トークン"
                && !matches!(zone, "field" | "ex" | "resolution")
            {
                Some("void")
            } else {
                None
            };
            previous
                .generation
                .checked_add(if followup.is_some() { 2 } else { 1 })
                .ok_or_else(|| invalid("movement generation exhausted"))?;
            plans.push((
                moves.len().saturating_sub(1),
                id,
                previous,
                from_kind,
                followup,
            ));
        }
        let ids = plans
            .iter()
            .map(|(_, id, _, _, _)| id.clone())
            .collect::<Vec<_>>();
        let departing_source = execution.source_before_move(self, &ids)?;
        let mut frame = execution.frame();
        if method == MoveMethod::Banish {
            // The ordinary banish event is emitted only for a banish instruction frame.
            frame
                .values
                .insert("banish_instruction".into(), json!(true));
        }
        self.move_objects(
            &ids,
            zone,
            if zone == "field" {
                Some(super::seat_name(execution.context.controller)?)
            } else {
                None
            },
            None,
            &frame,
        )?;
        if kind == ZoneKind::Hand {
            // As with draws, the public card stays known but its new hand slot is not located.
            for (_, id, previous, _, _) in &plans {
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
        for (index, id, previous, from_kind, followup) in plans {
            let current = self.object(&id)?;
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
            result.after = Some(after);
            result.actual_to = Some(result.requested_to);
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
            if matches!(method, MoveMethod::Destroy | MoveMethod::Banish) {
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
        execution.seal_source_departure(departing_source);
        Ok((moves, successful))
    }
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
