//! Physical movement and successful receipt projections; full replacement chains remain closed.

use super::{Execution, Fact, Game, InstructionOutcome, Result, invalid, player_id, unsupported};
use crate::ir::{
    Action, ActionPattern, Bool, Cause, CausePattern, DestroyOutcome, DurationEnd, Effect,
    EffectSource, Expr, Generation, Instruction, MoveDestination, MoveMethod, MoveOutcome,
    MoveReason, MoveResult, MoveSpec, MovementReceipt, ObjectReceipt, ObjectRef, OwnerZone,
    PlayerId, PlayerRef, Polarity, ReceiptId, ReceiptLocation, ResultMember, ResultMemberId,
    RuntimeObjectView, RuntimeZoneRef, Selector, SnapshotId, TimeAnchor, ZoneKind,
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
                    zone: OwnerZone::Cemetery | OwnerZone::Banish
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
                    _ => return Err(unsupported("private movement destination")),
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
        let members = execution.objects(self, objects)?;
        let seat = super::seat_name(execution.context.controller)?;
        let mut incoming = 0_i64;
        for &member in &members {
            let object = self.object(execution.object_id(member)?)?;
            if object.generation != member.generation.0 {
                continue;
            }
            if !matches!(object.zone.as_str(), "cemetery" | "ex")
                || object.owner != seat
                || object.controller != seat
                || self.object_type(&object.id)? != "フォロワー"
                || object.state["face"] != 0_i64
            {
                return Err(unsupported("ordinary public controller field entry"));
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
        let (moved, _) = self.internal_move_members(
            execution,
            &members,
            "field",
            ZoneKind::Field,
            MoveMethod::Put,
        )?;
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
            let from_kind = if previous.zone == "deck" && method == MoveMethod::Put {
                ZoneKind::Deck
            } else {
                public_zone(&previous.zone)?
            };
            if previous.zone == zone
                || (method == MoveMethod::Destroy && execution.destroy_guards.contains(&before))
            {
                continue;
            }
            // Tokens, advance cards and links entail additional rule moves not yet represented here.
            let card_type = self.object_type(&id)?;
            if card_type.contains("トークン")
                || card_type.contains("アドバンス")
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
            previous
                .generation
                .checked_add(1)
                .ok_or_else(|| invalid("movement generation exhausted"))?;
            plans.push((moves.len().saturating_sub(1), id, previous, from_kind));
        }
        let ids = plans
            .iter()
            .map(|(_, id, _, _)| id.clone())
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
        self.state.ir_epoch = self
            .state
            .ir_epoch
            .checked_add(1)
            .ok_or_else(|| invalid("IR epoch exhausted"))?;
        let after_snapshot = SnapshotId(self.state.ir_epoch);
        let group = self.group();
        for (index, id, previous, from_kind) in plans {
            let after = execution.reference(self, &id)?;
            if after.generation != Generation(previous.generation.saturating_add(1))
                || self.object(&id)?.zone != zone
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
                    after: Some(RuntimeObjectView {
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
