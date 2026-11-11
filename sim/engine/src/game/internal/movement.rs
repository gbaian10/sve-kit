//! Physical movement and successful receipt projections; full replacement chains remain closed.

use super::{
    Execution, Fact, Game, InstructionOutcome, Result, int, invalid, player_id, unsupported,
};
use crate::game::Object;
use crate::game::effects::token_erasure_zone;
use crate::ir::observation::{self, Consumer};
use crate::ir::shuffle as shuffle_shape;
use crate::ir::{
    Action, ActionPattern, BanishOutcome, Binding, Bool, Cause, CausePattern, Control,
    DestroyOutcome, DurationEnd, Effect, EffectSource, Expr, Generation, Instruction,
    InstructionContext, MoveDestination, MoveMethod, MoveOutcome, MoveReason, MoveResult, MoveSpec,
    MovementEvent, Nat, Node, ObjectOperand, ObjectRef, OwnerZone, PlayerId, PlayerRef,
    PlayerRelation, Polarity, Position, PositiveNat, Posture, ResultMember, ResultMemberId,
    RuntimeObjectView, RuntimeZoneRef, SelectionView, Selector, SliceDirection, SnapshotId,
    TimeAnchor, ZoneKind,
};
use alloc::collections::{BTreeMap, BTreeSet};
use core::num::NonZeroU64;
use serde_json::json;

mod capacity;
pub(super) mod mixed;
pub(super) use capacity::{ArrivalMember, CapacityWait};

// Planning and committing must agree on each destination's limit.
const FIELD_CAPACITY: i64 = 5;
const EX_CAPACITY: i64 = 5;

struct PlannedMove {
    result_index: usize,
    id: String,
    previous: Object,
    from_kind: ZoneKind,
    followup: Option<&'static str>,
    evolution: Option<(String, u64)>,
    stack_replaced: bool,
}

struct MovementPlan {
    moves: Vec<MoveResult>,
    plans: Vec<PlannedMove>,
}

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
    /// Immutable base printing, retained even when a token is erased on departure.
    pub card_before: String,
}

pub(super) fn check_action(action: &Action) -> Result<()> {
    match action {
        Action::Move { .. } if shuffle_shape::placement(action).is_some() => Ok(()),
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
        } if observed_move(action) || cemetery_ex_move(action) || revealed_move(action) => {
            if (!observation::visit(objects, Consumer::ObservedMove, &mut |_| {})
                && !cemetery_ex_move(action))
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
            objects,
            movement,
            reason: MoveReason::Put,
        } if admitted_field_entry(objects, movement) => Ok(()),
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

/// The enclosing player stage supplies authority for this otherwise private bound deck.
pub(super) fn player_banish(control: &Control) -> Option<(&Instruction, Binding<PlayerRef>, u64)> {
    let Control::EachPlayer { player, stages, .. } = control else {
        return None;
    };
    let [stage] = stages.as_slice() else {
        return None;
    };
    let [step] = stage.body.steps.as_slice() else {
        return None;
    };
    let Node::Instruction { instruction } = &step.node else {
        return None;
    };
    let Action::Banish {
        objects:
            Selector::Ordered {
                zone,
                start: Position::Top {},
                count: Expr::Literal(Nat(count)),
                direction: SliceDirection::Forward,
            },
    } = &instruction.action
    else {
        return None;
    };
    (*count > 0
        && zone.kind == ZoneKind::Deck
        && zone.player == (PlayerRef::Bound { binding: *player })
        && instruction.actor == zone.player)
        .then_some((instruction, *player, *count))
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

fn admitted_field_entry(objects: &Selector, movement: &MoveSpec) -> bool {
    let source_allowed = !matches!(&movement.to, MoveDestination::Shared { zone } if zone.player == (PlayerRef::Owner {}))
        || matches!(objects, Selector::Objects { objects } if matches!(objects.as_slice(),
            [ObjectOperand::FollowEventMove { event: MovementEvent::LeaveField { .. } }]));
    field_move(movement)
        && movement.position.is_none()
        && movement.order.is_none()
        && matches!(movement.initial_posture, None | Some(Posture::Acted))
        && movement.entry_effects.is_empty()
        && source_allowed
}

pub(super) fn field_move(movement: &MoveSpec) -> bool {
    matches!(&movement.to, MoveDestination::Shared { zone } if matches!(zone.player, PlayerRef::Controller {} | PlayerRef::Owner {}) && zone.kind == ZoneKind::Field)
}

pub(super) fn cemetery_ex_move(action: &Action) -> bool {
    matches!(action, Action::Move { objects: Selector::Bound { .. }, movement, .. }
        if matches!(&movement.to, MoveDestination::Shared { zone }
            if zone.player == (PlayerRef::Controller {}) && zone.kind == ZoneKind::Ex))
}

/// A private hand card moves when it was chosen privately, or when it pays
/// 「手札のこれをEXエリアに置く」 for its own activation.
fn private_hand_member(
    execution: &Execution,
    instruction: &Instruction,
    objects: &Selector,
    object: &Object,
    member: ObjectRef,
) -> bool {
    execution.is_private_hand_target(objects, member)
        || (object.id == execution.context.source
            && member.generation == execution.context.generation
            && instruction.context == InstructionContext::Cost
            && super::card::hand_payment::source_to_ex(&instruction.action))
}

fn banished_hand_return(execution: &Execution, objects: &Selector, member: ObjectRef) -> bool {
    let Selector::Objects { objects } = objects else {
        return false;
    };
    let [
        ObjectOperand::FollowEventMove {
            event: event @ MovementEvent::Banish { .. },
        },
    ] = objects.as_slice()
    else {
        return false;
    };
    execution.event_move == Some((*event, Some(member)))
}

pub(super) fn revealed_move(action: &Action) -> bool {
    matches!(action, Action::Move { objects, movement, .. } if super::reveal::look_receipt(objects).is_some()
        && (movement.to == (MoveDestination::Owners { zone: OwnerZone::Hand }) || field_move(movement)))
}

pub(super) fn observed_move(action: &Action) -> bool {
    !cemetery_ex_move(action)
        && matches!(action, Action::Move { objects, movement, .. }
        if matches!(&movement.to, MoveDestination::Shared { zone }
            if zone.player == (PlayerRef::Controller {}) && zone.kind == ZoneKind::Ex)
            || (movement.to == (MoveDestination::Owners { zone: OwnerZone::Cemetery })
                && observation::visit(objects, Consumer::ObservedMove, &mut |_| {})))
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
        let form_print = self.face(&form.id)?;
        if object.zone != "field"
            || form.zone != "evolution"
            || base_print["card_type"] != "フォロワー"
            || form_print["card_type"] != "フォロワー・エボルヴ"
            || !super::card::resource_association(self, &object.id, &form.id)?
            || object.state["face"] != form.state["face"]
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
        let available = usize::try_from(
            EX_CAPACITY
                .saturating_sub(self.zone_count(owner, "ex"))
                .max(0),
        )
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
        let equipped = self.internal_equipment_links(&previous.id)?;
        let special = equipped
            || card_type.contains("トークン")
            || card_type.contains("アドバンス")
            || has_evolution
            || resource_links;
        if special {
            if execution.ability.is_none()
                && !self.internal_exchange_token_cost_authorized(execution, &previous.id, zone)?
            {
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
                    | "イクイップメント・トークン"
                    | "クレスト・トークン"
            ))
            || (previous
                .state
                .get("links")
                .is_some_and(|value| !value.is_null())
                && !resource_links)
        {
            return Err(unsupported(
                "movement requires linked or special-card rules",
            ));
        }
        let followup = if card_type == "フォロワー・アドバンス"
            && !matches!(zone, "field" | "ex" | "resolution" | "evolve_deck")
        {
            Some("evolve_deck")
        } else if token_erasure_zone(card_type, zone) {
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

    pub(super) fn internal_player_banish(
        &mut self,
        execution: &mut Execution,
        control: &Control,
    ) -> Result<()> {
        super::tokens::check_each(control)?;
        let (instruction, binding, count) =
            player_banish(control).ok_or_else(|| unsupported("all-player deck banish stage"))?;
        let Control::EachPlayer { players, .. } = control else {
            return Err(invalid("player banish stage"));
        };
        let mut players = execution.players(self, players)?;
        let active = player_id(self.active())?;
        players.sort_by_key(|member| *member != active);
        let mut members = Vec::new();
        for player in players {
            execution
                .slots
                .insert(binding.key(), super::Slot::Player(player));
            members.extend(self.deck_top_members(
                execution,
                &PlayerRef::Bound { binding },
                count,
            )?);
        }
        // Every player's slice is captured before the shared movement batch mutates either deck.
        let (moves, successful) = self.internal_move_members(
            execution,
            &members,
            "banish",
            ZoneKind::Banish,
            MoveMethod::Banish,
            false,
            None,
            None,
        )?;
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Banish(BanishOutcome { successful, moves }),
        );
        Ok(())
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
            let members =
                if let Some(materials) = execution.private_hand_banish_materials(instruction) {
                    materials.to_vec()
                } else {
                    execution.objects(self, objects)?
                };
            for member in &members {
                let object = self.object(execution.object_id(*member)?)?;
                if object.generation != member.generation.0 {
                    continue;
                }
                let actor = super::seat_name(execution.context.controller)?;
                let private_look = self.private_look_deck_member(execution, objects, *member)?;
                let private_hand = object.zone == "hand"
                    && object.owner == actor
                    && object.controller == object.owner
                    && (execution.is_private_hand_target(objects, *member)
                        || execution.is_private_hand_banish_cost(instruction, *member));
                let resolving_source = object.zone == "resolution"
                    && execution.spell
                    && execution.ability.is_some()
                    && matches!(objects, Selector::Objects { objects }
                        if objects.as_slice() == [ObjectOperand::SourceHolder {}])
                    && object.id == execution.context.source
                    && member.generation == execution.context.generation
                    && object.controller == actor
                    && self.object_type(&object.id)?.starts_with("スペル");
                if !(private_look || private_hand || resolving_source) {
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
            None,
        )?;
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Banish(BanishOutcome { successful, moves }),
        );
        Ok(())
    }

    fn check_move_members(
        &self,
        execution: &Execution,
        instruction: &Instruction,
        members: &[ObjectRef],
        kind: ZoneKind,
        method: MoveMethod,
        from_deck_top: bool,
    ) -> Result<()> {
        let (Action::Move { objects, .. } | Action::Destroy { objects }) = &instruction.action
        else {
            return Err(invalid(
                "movement member validation needs a movement instruction",
            ));
        };
        for &member in members {
            let object = self.object(execution.object_id(member)?)?;
            if object.generation == member.generation.0 {
                let actor = super::seat_name(execution.context.controller)?;
                let private_hand_move = method == MoveMethod::Put
                    && matches!(kind, ZoneKind::Ex | ZoneKind::Deck)
                    && object.zone == "hand"
                    && object.owner == actor
                    && object.controller == actor
                    && private_hand_member(execution, instruction, objects, object, member);
                let private_observed_move =
                    method == MoveMethod::Put
                        && matches!(kind, ZoneKind::Hand | ZoneKind::Ex | ZoneKind::Cemetery)
                        && (self.private_look_deck_member(execution, objects, member)?
                            || (object.zone == "deck"
                                && object.owner == actor
                                && object.controller == actor
                                && self.state.knowledge.get(actor).is_some_and(|knowledge| {
                                    knowledge.knows_location(&object.id)
                                })
                                && execution.public_look_member(self, objects, member)?));
                let resolving_source = method == MoveMethod::Put
                    && matches!(kind, ZoneKind::Hand | ZoneKind::Ex)
                    && object.zone == "resolution"
                    && object.id == execution.context.source
                    && member.generation == execution.context.generation
                    && execution.ability.is_some()
                    && self.object_type(&object.id)? == "スペル";
                if kind == ZoneKind::Hand
                    && !matches!(object.zone.as_str(), "field" | "cemetery" | "banish")
                    && !private_observed_move
                    && !resolving_source
                {
                    return Err(unsupported(
                        "only public departures or the resolving spell source to hand",
                    ));
                }
                let special_origin = private_observed_move
                    || private_hand_move
                    || resolving_source
                    || (method == MoveMethod::Put
                        && kind == ZoneKind::Ex
                        && object.zone == "banish"
                        && object.owner == actor
                        && object.controller == actor
                        && banished_hand_return(execution, objects, member))
                    || (kind == ZoneKind::Ex
                        && execution.is_private_advance_target(objects, member)
                        && self.own_evolve_advance(&object.id, actor)?);
                if kind == ZoneKind::Ex
                    && !from_deck_top
                    && !special_origin
                    && !matches!(object.zone.as_str(), "field" | "cemetery" | "ex")
                {
                    return Err(unsupported("public field or cemetery movement to EX"));
                }
                if !from_deck_top && !special_origin {
                    public_zone(&object.zone)?;
                }
            }
        }
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
        self.check_move_members(
            execution,
            instruction,
            &members,
            kind,
            method,
            top.is_some(),
        )?;
        let capacity_plan = if kind == ZoneKind::Ex {
            Some(self.plan_movements(execution, &members, zone, kind, method, true, None)?)
        } else {
            None
        };
        let members = if let Some(plan) = &capacity_plan {
            let Some(chosen) = self.internal_movement_capacity(execution, instruction, plan)?
            else {
                return Ok(());
            };
            chosen
        } else {
            members
        };
        let (mut moves, successful) = self.internal_move_members(
            execution,
            &members,
            zone,
            kind,
            method,
            kind == ZoneKind::Ex,
            movement.and_then(|(spec, _)| spec.position),
            None,
        )?;
        if let Some(plan) = capacity_plan {
            moves = capacity::complete_receipt(plan.moves, moves);
            capacity::finish(execution, instruction);
        }
        let outcome = if method == MoveMethod::Destroy {
            InstructionOutcome::Destroy(DestroyOutcome { successful, moves })
        } else {
            InstructionOutcome::Move(MoveOutcome { moved: moves })
        };
        execution.receipts.insert(instruction.receipt, outcome);
        Ok(())
    }

    /// The named Advance pick may put its own evolve-deck Advance card into EX (BP10-109).
    fn own_evolve_advance(&self, id: &str, actor: &str) -> Result<bool> {
        let object = self.object(id)?;
        Ok(object.zone == "evolve_deck"
            && object.owner == actor
            && object.controller == actor
            && object.state["face_up"] != true
            && self.object_type(id)? == "フォロワー・アドバンス")
    }

    pub(super) fn deck_top_members(
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

    fn check_field_entry(
        &self,
        execution: &Execution,
        members: &[ObjectRef],
        objects: &Selector,
        seat: &str,
    ) -> Result<()> {
        for &member in members {
            let object = self.object(execution.object_id(member)?)?;
            if object.generation != member.generation.0 {
                continue;
            }
            let ordinary = (matches!(self.object_type(&object.id)?, "フォロワー" | "アミュレット")
                && matches!(object.zone.as_str(), "cemetery" | "ex" | "banish"))
                // A follower token waiting in EX enters like any EX follower (BP01-078's ドラゴン).
                || (self.object_type(&object.id)? == "フォロワー・トークン" && object.zone == "ex");
            let private_hand = object.zone == "hand"
                && object.owner == seat
                && object.controller == seat
                && execution.is_private_hand_target(objects, member)
                && matches!(self.object_type(&object.id)?, "フォロワー" | "アミュレット");
            let private_look = ((seat == super::seat_name(execution.context.controller)?
                && self.private_look_deck_member(execution, objects, member)?)
                || (object.zone == "deck"
                    && object.owner == seat
                    && object.controller == seat
                    && self
                        .state
                        .knowledge
                        .get(seat)
                        .is_some_and(|knowledge| knowledge.knows_location(&object.id))
                    && execution.public_look_member(self, objects, member)?))
                && matches!(self.object_type(&object.id)?, "フォロワー" | "アミュレット");
            let revealed_match = object.zone == "deck"
                && execution.is_revealed_match(objects, member)
                && self.object_type(&object.id)? == "フォロワー"
                && ["P1", "P2"].iter().all(|viewer| {
                    self.state
                        .knowledge
                        .get(*viewer)
                        .is_some_and(|known| known.knows_location(&object.id))
                });
            let advance = self.object_type(&object.id)? == "フォロワー・アドバンス"
                && (object.zone == "ex"
                    || (object.zone == "evolve_deck"
                        && (matches!(
                            objects,
                            Selector::Zone {
                                zone: ZoneKind::Evolve,
                                ..
                            }
                        ) || execution.is_private_advance_target(objects, member))));
            if !(ordinary || advance || private_look || private_hand || revealed_match)
                || object.owner != seat
                || object.controller != seat
                || object.state["face"] != 0_i64
            {
                return Err(unsupported("admitted controller field entry"));
            }
        }
        Ok(())
    }

    fn field_entry_player(&self, execution: &Execution, movement: &MoveSpec) -> Result<PlayerId> {
        let MoveDestination::Shared { zone } = &movement.to else {
            return Err(invalid("field entry destination"));
        };
        execution.player(self, &zone.player)
    }

    fn field_entry_members(
        &self,
        execution: &Execution,
        objects: &Selector,
    ) -> Result<Vec<ObjectRef>> {
        if let Selector::Zone {
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
            .collect::<Result<Vec<_>>>()
        } else {
            execution.objects(self, objects)
        }
    }

    fn internal_field_entry(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        objects: &Selector,
    ) -> Result<()> {
        let members = self.field_entry_members(execution, objects)?;
        let Action::Move { movement, .. } = &instruction.action else {
            return Err(invalid("field entry movement"));
        };
        let destination = self.field_entry_player(execution, movement)?;
        let owner_destination = destination != execution.context.controller;
        let seat = super::seat_name(destination)?;
        self.check_field_entry(execution, &members, objects, seat)?;
        if self.zone_count(seat, "field") >= FIELD_CAPACITY {
            execution.receipts.insert(
                instruction.receipt,
                InstructionOutcome::Move(MoveOutcome {
                    moved: capacity::failed_field_entry(&members, destination),
                }),
            );
            capacity::finish(execution, instruction);
            return Ok(());
        }
        let plan = self.plan_movements(
            execution,
            &members,
            "field",
            ZoneKind::Field,
            MoveMethod::Put,
            owner_destination,
            None,
        )?;
        let Some(selected) = self.internal_movement_capacity(execution, instruction, &plan)? else {
            return Ok(());
        };
        for &member in &selected {
            let id = execution.object_id(member)?.to_owned();
            if movement.initial_posture.is_none()
                && self.object(&id)?.generation == member.generation.0
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
            &selected,
            "field",
            ZoneKind::Field,
            MoveMethod::Put,
            owner_destination,
            None,
            movement.initial_posture,
        )?;
        for member in &moved {
            if member.after.is_some() && movement.initial_posture.is_none() {
                self.internal_ward_entry_posture(execution, member.before)?;
            }
        }
        let moved = capacity::complete_receipt(plan.moves, moved);
        capacity::finish(execution, instruction);
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Move(MoveOutcome { moved }),
        );
        Ok(())
    }

    pub(super) fn internal_cemetery_ex_move(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        objects: &Selector,
    ) -> Result<()> {
        let members = execution.objects(self, objects)?;
        if members.len() > 1 {
            return Err(unsupported("single cemetery capture"));
        }
        for &member in &members {
            let current = self.object(execution.object_id(member)?)?;
            if current.generation == member.generation.0 && current.zone != "cemetery" {
                return Err(unsupported("cemetery capture origin"));
            }
        }
        let Action::Move { movement, .. } = &instruction.action else {
            return Err(invalid("cemetery capture action"));
        };
        let moved = self.internal_search_move(
            execution,
            &members,
            &movement.to,
            execution.context.controller,
            None,
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
        let members = execution.observed_objects(self, objects, player, Consumer::ObservedMove)?;
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
                    .is_some_and(|knowledge| knowledge.knows_location(id))
            {
                return Err(unsupported(
                    "observed move requires current own-deck evidence",
                ));
            }
        }
        let Action::Move { movement: spec, .. } = &instruction.action else {
            return Err(unsupported("observed move action"));
        };
        let capacity_plan = if matches!(&spec.to, MoveDestination::Shared { zone } if zone.kind == ZoneKind::Ex)
            && self.zone_count(seat, "ex") < EX_CAPACITY
        {
            Some(self.plan_movements(
                execution,
                &members,
                "ex",
                ZoneKind::Ex,
                MoveMethod::Put,
                false,
                None,
            )?)
        } else {
            None
        };
        let members = if let Some(plan) = &capacity_plan {
            let Some(chosen) = self.internal_movement_capacity(execution, instruction, plan)?
            else {
                return Ok(());
            };
            chosen
        } else {
            members
        };
        let mut moved = self.internal_search_move(execution, &members, &spec.to, player, None)?;
        if let Some(plan) = capacity_plan {
            moved = capacity::complete_receipt(plan.moves, moved);
            capacity::finish(execution, instruction);
        }
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Move(MoveOutcome { moved }),
        );
        Ok(())
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "The pure proposal uses exactly the atomic movement commit's inputs."
    )]
    fn plan_movements(
        &self,
        execution: &Execution,
        members: &[ObjectRef],
        zone: &str,
        kind: ZoneKind,
        method: MoveMethod,
        owner_destination: bool,
        position: Option<Position>,
    ) -> Result<MovementPlan> {
        let mut moves = Vec::new();
        let mut plans = Vec::new();
        for &before in members {
            let id = execution.object_id(before)?.to_owned();
            let previous = self.object(&id)?.clone();
            let requested_to = RuntimeZoneRef {
                player: if shared_destination(zone) && !owner_destination {
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
            let prohibited = match method {
                MoveMethod::Destroy => self.restricted(&id, "ability_destroy")?,
                MoveMethod::Banish => self.restricted(&id, "banish")?,
                MoveMethod::Put
                | MoveMethod::Return
                | MoveMethod::Mill
                | MoveMethod::Draw
                | MoveMethod::Discard
                | MoveMethod::Play => false,
            };
            if prohibited
                || previous.zone == zone
                || (method == MoveMethod::Destroy && execution.destroy_guards.contains(&before))
            {
                continue;
            }
            let evolution = self.departing_evolution(&previous)?;
            let followup =
                self.departure_followup(execution, &previous, zone, evolution.is_some())?;

            let stack_replaced = self.stack_replaces_departure(&id, zone)?;
            plans.push(PlannedMove {
                result_index: moves.len().saturating_sub(1),
                id,
                previous,
                from_kind,
                followup,
                evolution,
                stack_replaced,
            });
        }
        Ok(MovementPlan { moves, plans })
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Existing movement callers keep the same atomic input boundary."
    )]
    pub(super) fn internal_move_members(
        &mut self,
        execution: &mut Execution,
        members: &[ObjectRef],
        zone: &str,
        kind: ZoneKind,
        method: MoveMethod,
        owner_destination: bool,
        position: Option<Position>,
        initial_posture: Option<Posture>,
    ) -> Result<(Vec<MoveResult>, Vec<ResultMember>)> {
        self.commit_move_members(
            execution,
            members,
            zone,
            kind,
            method,
            owner_destination,
            position,
            initial_posture,
            false,
        )
    }

    pub(super) fn internal_unordered_deck_return(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        members: &[ObjectRef],
    ) -> Result<Vec<MoveResult>> {
        self.check_move_members(
            execution,
            instruction,
            members,
            ZoneKind::Deck,
            MoveMethod::Put,
            false,
        )?;
        let (moves, _) = self.commit_move_members(
            execution,
            members,
            "deck",
            ZoneKind::Deck,
            MoveMethod::Put,
            false,
            None,
            None,
            true,
        )?;
        Ok(moves)
    }

    pub(super) fn internal_whole_deck_return(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        members: &[ObjectRef],
    ) -> Result<Vec<MoveResult>> {
        if shuffle_shape::whole_return(&instruction.action).is_none() {
            return Err(unsupported("whole-zone deck return"));
        }
        for member in members {
            let object = self.object(execution.object_id(*member)?)?;
            if object.generation != member.generation.0
                || !matches!(object.zone.as_str(), "hand" | "cemetery")
                || object.owner != object.controller
            {
                return Err(unsupported("whole-zone deck return membership"));
            }
        }
        let (moves, _) = self.commit_move_members(
            execution,
            members,
            "deck",
            ZoneKind::Deck,
            MoveMethod::Put,
            false,
            None,
            None,
            true,
        )?;
        Ok(moves)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Replacements, generations and receipts commit at one atomic boundary."
    )]
    fn commit_move_members(
        &mut self,
        execution: &mut Execution,
        members: &[ObjectRef],
        zone: &str,
        kind: ZoneKind,
        method: MoveMethod,
        owner_destination: bool,
        position: Option<Position>,
        initial_posture: Option<Posture>,
        unordered_deck: bool,
    ) -> Result<(Vec<MoveResult>, Vec<ResultMember>)> {
        let plan = self.plan_movements(
            execution,
            members,
            zone,
            kind,
            method,
            owner_destination,
            position,
        )?;
        self.commit_movement_plan(
            execution,
            plan,
            &[],
            zone,
            kind,
            method,
            owner_destination,
            position,
            initial_posture,
            unordered_deck,
        )
    }

    #[expect(
        clippy::too_many_lines,
        clippy::too_many_arguments,
        reason = "Physical moves and selected creations share one atomic movement boundary."
    )]
    fn commit_movement_plan(
        &mut self,
        execution: &mut Execution,
        proposal: MovementPlan,
        created: &[String],
        zone: &str,
        kind: ZoneKind,
        method: MoveMethod,
        owner_destination: bool,
        position: Option<Position>,
        initial_posture: Option<Posture>,
        unordered_deck: bool,
    ) -> Result<(Vec<MoveResult>, Vec<ResultMember>)> {
        if !created.is_empty()
            && (kind != ZoneKind::Ex || !owner_destination || method != MoveMethod::Put)
        {
            return Err(unsupported("mixed creation destination"));
        }
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
        let MovementPlan {
            mut moves,
            mut plans,
        } = proposal;
        let mut successful = Vec::new();
        if owner_destination && kind == ZoneKind::Ex {
            self.filter_owner_ex_arrivals(&mut plans, created)?;
        }
        if kind == ZoneKind::Deck && !unordered_deck {
            let mut owners = BTreeSet::new();
            if plans
                .iter()
                .filter(|plan| !plan.stack_replaced)
                .any(|plan| !owners.insert(&plan.previous.owner))
            {
                return Err(unsupported(
                    "simultaneous deck placement needs an order choice",
                ));
            }
        }
        let ids = plans
            .iter()
            .map(|plan| plan.id.clone())
            .chain(created.iter().cloned())
            .collect::<Vec<_>>();
        let banished = self.internal_prepare_banish_events(&ids, execution, method)?;
        let departing_source = execution.source_before_move(self, &ids)?;
        let mut frame = execution.frame();
        if method == MoveMethod::Destroy {
            frame.values.insert("destroyed_subjects".into(), json!(ids));
        }
        if owner_destination {
            frame.values.insert("owner_destination".into(), json!(true));
        }
        match (initial_posture, zone) {
            (None, _) => {}
            (Some(Posture::Acted), "field") => {
                frame.values.insert("field_entry_acted".into(), json!(true));
            }
            (Some(Posture::Acted | Posture::Standing), _) => {
                return Err(unsupported("fixed acted field entry"));
            }
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
        let player_mark = self.player_event_mark();
        if method == MoveMethod::Discard {
            self.discard(&ids, &frame)?;
        } else {
            self.move_objects(
                &ids,
                zone,
                if shared_destination(zone) && !owner_destination {
                    Some(super::seat_name(execution.context.controller)?)
                } else {
                    None
                },
                placement.as_ref(),
                &frame,
            )?;
        }
        self.player_movement_method(
            player_mark,
            &ids,
            super::super::player::events::MovementMethod::Effect(method),
        );
        self.check_created_ex_arrivals(created)?;
        if kind == ZoneKind::Hand {
            // As with draws, the public card stays known but its new hand slot is not located.
            for PlannedMove { id, previous, .. } in plans.iter().filter(|plan| !plan.stack_replaced)
            {
                for (viewer, knowledge) in &mut self.state.knowledge {
                    if *viewer != previous.owner {
                        knowledge.forget_location(id);
                    }
                }
            }
        }
        if kind == ZoneKind::Hand {
            self.player_movement_private_hand(player_mark, &ids);
        }
        self.state.ir_epoch = self
            .state
            .ir_epoch
            .checked_add(1)
            .ok_or_else(|| invalid("IR epoch exhausted"))?;
        let after_snapshot = SnapshotId(self.state.ir_epoch);
        let group = self.group();
        for PlannedMove {
            result_index: index,
            id,
            previous,
            from_kind,
            followup,
            evolution,
            stack_replaced,
        } in plans
        {
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
            execution.record_fact(
                self,
                Fact::Movement(Box::new(MovementFact {
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
                    card_before: previous.card.clone(),
                })),
            );
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
                let player_step = self.emit(
                    json!({"kind":"破壊","object":id,"source":frame.source}),
                    &frame.cause,
                    group,
                );
                self.omit_player_step(&player_step);
            }
        }
        let departed = departing_source
            .into_iter()
            .filter(|(_, _, departing)| {
                moves
                    .iter()
                    .any(|movement| movement.before == *departing && movement.after.is_some())
            })
            .collect::<Vec<_>>();
        execution.seal_source_departure(&departed);
        if let Some(plan) = banished {
            self.internal_commit_banish_events(plan, &moves)?;
        }
        if kind == ZoneKind::Deck {
            self.invalidate_deck_observations(execution, &moves, &old_deck)?;
        }
        Ok((moves, successful))
    }

    fn filter_owner_ex_arrivals(
        &self,
        plans: &mut Vec<PlannedMove>,
        created: &[String],
    ) -> Result<()> {
        let mut arrivals = BTreeMap::<String, usize>::new();
        for PlannedMove { previous, .. } in plans.iter().filter(|plan| !plan.stack_replaced) {
            let count = arrivals.entry(previous.owner.clone()).or_default();
            *count = count.saturating_add(1);
        }
        for id in created {
            let count = arrivals.entry(self.object(id)?.owner.clone()).or_default();
            *count = count.saturating_add(1);
        }
        for (owner, count) in arrivals {
            if !self.owner_ex_has_room(&owner, count)? {
                plans.retain(|plan| plan.stack_replaced || plan.previous.owner != owner);
            }
        }
        Ok(())
    }

    fn check_created_ex_arrivals(&self, created: &[String]) -> Result<()> {
        for id in created {
            let object = self.object(id)?;
            if object.zone != "ex" || object.generation != 1 || object.controller != object.owner {
                return Err(invalid("token arrival diverged from the shared proposal"));
            }
        }
        Ok(())
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
                    knowledge.forget_location(id);
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
        ("evolve_deck", MoveMethod::Put, "field" | "ex") => Ok(ZoneKind::Evolve),
        // Source membership is proven before planning; this only labels the departure.
        ("resolution", MoveMethod::Put, "hand" | "ex")
        | ("resolution", MoveMethod::Banish, "banish") => Ok(ZoneKind::Resolution),
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
