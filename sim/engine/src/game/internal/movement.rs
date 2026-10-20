//! Public-zone movement subset; replacements and cross-generation reads remain closed.

use super::{Execution, Fact, Game, InstructionOutcome, Result, invalid, player_id, unsupported};
use crate::ir::{
    Action, ActionPattern, Bool, Cause, CausePattern, DestroyOutcome, DurationEnd, Effect,
    EffectSource, Expr, Generation, Instruction, MoveDestination, MoveMethod, MoveOutcome,
    MoveReason, MoveResult, MoveSpec, OwnerZone, PlayerId, Polarity, ResultMember, ResultMemberId,
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

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Movement shares the authoritative Game and its mutation primitive."
)]
impl Game {
    #[expect(
        clippy::too_many_lines,
        reason = "Planning, commit and receipt construction form one atomic movement instruction."
    )]
    pub(super) fn internal_move(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        objects: &Selector,
        movement: Option<(&MoveSpec, MoveReason)>,
    ) -> Result<()> {
        let (zone, kind, method) = match movement {
            None => ("cemetery", ZoneKind::Cemetery, MoveMethod::Destroy),
            Some((spec, reason)) => {
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
        let snapshot = SnapshotId(self.state.ir_epoch);
        let mut moves = Vec::new();
        let mut successful = Vec::new();
        let mut plans = Vec::new();
        for before in execution.objects(self, objects)? {
            let id = execution.object_id(before)?.to_owned();
            let previous = self.object(&id)?.clone();
            let requested_to = RuntimeZoneRef {
                player: player_id(&previous.owner)?,
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
            let from_kind = public_zone(&previous.zone)?;
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
        let frame = execution.frame();
        self.move_objects(&ids, zone, None, None, &frame)?;
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
                || self.object(&id)?.controller != previous.owner
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
            if method == MoveMethod::Destroy {
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
                self.emit(
                    json!({"kind":"破壊","object":id,"source":frame.source}),
                    &frame.cause,
                    group,
                );
            }
        }
        let outcome = if method == MoveMethod::Destroy {
            InstructionOutcome::Destroy(DestroyOutcome { successful, moves })
        } else {
            InstructionOutcome::Move(MoveOutcome { moved: moves })
        };
        execution.receipts.insert(instruction.receipt, outcome);
        Ok(())
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
