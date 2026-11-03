//! The chooser's hand is exposed only through token-local candidates for this discard.

use super::{
    Execution, Game, InputDomain, InstructionOutcome, Result, Wait, contains, invalid, seat_name,
    unsupported,
};
use crate::ir::{
    Action, Bool, Cardinality, DecisionId, DiscardEvent, DiscardMode, DiscardOutcome, Disjointness,
    Expr, Instruction, Int, MoveMethod, Nat, ObjectRef, PlayerRelation, ReceiptId, SelectionView,
    Selector, ZoneKind,
};
use alloc::collections::BTreeSet;

#[derive(Clone, Debug)]
pub(super) struct HandWait {
    instruction: Box<Instruction>,
    members: Vec<ObjectRef>,
    bounds: Cardinality,
}

pub(super) fn check_action(action: &Action) -> Result<()> {
    if let Action::Discard { mode, .. } = action {
        match mode {
            DiscardMode::Chosen { .. } => {
                bounds(action)?;
            }
            DiscardMode::All { .. } => {
                eligible(action)?;
            }
            DiscardMode::Random { .. } => {
                random_count(action)?;
            }
            DiscardMode::DownTo { .. } => return Err(unsupported("self hand discard mode")),
        }
    }
    Ok(())
}

fn eligible(action: &Action) -> Result<&Selector> {
    let Action::Discard {
        player,
        mode: DiscardMode::All { eligible },
    } = action
    else {
        return Err(unsupported("self all-hand discard"));
    };
    let Selector::Zone {
        players,
        zone: ZoneKind::Hand,
        view: SelectionView::Current {},
        predicate,
        ..
    } = eligible
    else {
        return Err(unsupported("direct current hand discard"));
    };
    if players.relative_to != *player
        || players.relation != PlayerRelation::SelfPlayer
        || players.cardinality.minimum != Nat(1)
        || players.cardinality.maximum != Some(Nat(1))
        || **predicate != Expr::Literal(Bool(true))
    {
        return Err(unsupported("unfiltered own hand discard"));
    }
    Ok(eligible)
}

/// A random discard draws from its player's whole current hand by a fixed positive count.
fn random_count(action: &Action) -> Result<u64> {
    let Action::Discard {
        player,
        mode:
            DiscardMode::Random {
                population:
                    Selector::Zone {
                        players,
                        zone: ZoneKind::Hand,
                        view: SelectionView::Current {},
                        predicate,
                        ..
                    },
                count: Expr::Literal(Int(count)),
            },
    } = action
    else {
        return Err(unsupported("random whole-hand discard"));
    };
    if players.relative_to != *player
        || players.relation != PlayerRelation::SelfPlayer
        || players.cardinality.minimum != Nat(1)
        || players.cardinality.maximum != Some(Nat(1))
        || **predicate != Expr::Literal(Bool(true))
        || *count <= 0
    {
        return Err(unsupported("unfiltered random hand discard"));
    }
    u64::try_from(*count).map_err(invalid)
}

fn bounds(action: &Action) -> Result<Cardinality> {
    let Action::Discard {
        player,
        mode: DiscardMode::Chosen { constraint },
    } = action
    else {
        return Err(unsupported("self chosen hand discard"));
    };
    let [group] = constraint.groups.as_slice() else {
        return Err(unsupported("single hand discard group"));
    };
    let count = constraint.cardinality;
    if count.minimum.0 == 0
        || count.maximum != Some(count.minimum)
        || group.cardinality != count
        || constraint.disjoint != Disjointness::Disjoint
        || constraint.distinct_by.is_some()
        || !constraint.aggregate_bounds.is_empty()
    {
        return Err(unsupported("fixed positive discard cardinality"));
    }
    let Selector::Zone {
        players,
        zone: ZoneKind::Hand,
        view: SelectionView::Current {},
        predicate,
        ..
    } = &group.selector
    else {
        return Err(unsupported("direct current hand discard"));
    };
    if players.relative_to != *player
        || players.relation != PlayerRelation::SelfPlayer
        || players.cardinality.minimum != Nat(1)
        || players.cardinality.maximum != Some(Nat(1))
        || **predicate != Expr::Literal(Bool(true))
    {
        return Err(unsupported("unfiltered own hand discard"));
    }
    Ok(count)
}

impl HandWait {
    pub(super) const fn strict(
        instruction: Box<Instruction>,
        members: Vec<ObjectRef>,
        bounds: Cardinality,
    ) -> Self {
        Self {
            instruction,
            members,
            bounds,
        }
    }
    pub(super) const fn receipt(&self) -> ReceiptId {
        self.instruction.receipt
    }

    pub(super) fn inline_members(
        &self,
        game: &Game,
        execution: &Execution,
        ids: &[String],
    ) -> Result<Vec<ObjectRef>> {
        self.validate(
            game,
            execution,
            &execution.inline_material_handles(&self.members, ids)?,
        )
        .map_err(|_error| {
            crate::EngineFailure::IllegalPlay("invalid evolution discard selection".into())
        })
    }

    pub(super) fn validate(
        &self,
        game: &Game,
        execution: &Execution,
        handles: &[u64],
    ) -> Result<Vec<ObjectRef>> {
        if !contains(
            self.bounds,
            Nat(u64::try_from(handles.len()).map_err(invalid)?),
        ) {
            return Err(invalid("discard cardinality"));
        }
        let seat = seat_name(execution.player(game, &self.instruction.actor)?)?;
        let mut unique = BTreeSet::new();
        handles
            .iter()
            .map(|handle| {
                if !unique.insert(*handle) {
                    return Err(invalid("duplicate discard handle"));
                }
                let member = *self
                    .members
                    .get(usize::try_from(*handle).map_err(invalid)?)
                    .ok_or_else(|| invalid("foreign discard handle"))?;
                let object = game.object(execution.object_id(member)?)?;
                if object.generation != member.generation.0
                    || object.zone != "hand"
                    || object.owner != seat
                    || object.controller != seat
                {
                    return Err(invalid("stale own-hand discard member"));
                }
                Ok(member)
            })
            .collect()
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Private choice and movement use the same authoritative Game."
)]
impl Game {
    pub(super) fn internal_discard_wait(
        &mut self,
        execution: &mut Execution,
        instruction: Box<Instruction>,
    ) -> Result<()> {
        if matches!(
            &instruction.action,
            Action::Discard {
                mode: DiscardMode::All { .. },
                ..
            }
        ) {
            eligible(&instruction.action)?;
            let player = execution.player(self, &instruction.actor)?;
            let seat = seat_name(player)?;
            let members = self
                .zone_ids(seat, "hand")
                .into_iter()
                .map(|id| execution.reference(self, &id))
                .collect::<Result<Vec<_>>>()?;
            let outcome = self.internal_discard_members(execution, &members)?;
            execution
                .receipts
                .insert(instruction.receipt, InstructionOutcome::Discard(outcome));
            return Ok(());
        }
        if let Action::Discard {
            player,
            mode: DiscardMode::Random { .. },
        } = &instruction.action
        {
            let count = random_count(&instruction.action)?;
            let seat = seat_name(execution.player(self, player)?)?;
            let hand = self.zone_ids(seat, "hand");
            // Anonymous hand cards still belong to the population; never draw around them.
            if i64::try_from(hand.len()).map_err(invalid)? != self.zone_count(seat, "hand") {
                return Err(unsupported("random discard from anonymous hand cards"));
            }
            let chosen = self.random_members(hand, usize::try_from(count).map_err(invalid)?)?;
            let members = chosen
                .iter()
                .map(|id| execution.reference(self, id))
                .collect::<Result<Vec<_>>>()?;
            let outcome = self.internal_discard_members(execution, &members)?;
            execution
                .receipts
                .insert(instruction.receipt, InstructionOutcome::Discard(outcome));
            return Ok(());
        }
        let mut count = bounds(&instruction.action)?;
        let player = execution.player(self, &instruction.actor)?;
        let seat = seat_name(player)?;
        let mut members = Vec::new();
        let mut candidates = Vec::new();
        for id in self.zone_ids(seat, "hand") {
            let object = self.object(&id)?;
            if object.owner != seat || object.controller != seat {
                return Err(unsupported("foreign hand owner"));
            }
            candidates.push(super::DistributionMember {
                handle: u64::try_from(members.len()).map_err(invalid)?,
                player,
                card: Some(object.card.clone()),
                name: Some(self.card_name(&id)?),
            });
            members.push(execution.reference(self, &id)?);
        }
        count.minimum.0 = count
            .minimum
            .0
            .min(u64::try_from(members.len()).map_err(invalid)?);
        self.internal_wait(
            execution,
            DecisionId(instruction.id.0),
            player,
            InputDomain::Targets(super::TargetDomain {
                members: candidates,
                cardinality: count,
                distinct_names: false,
            }),
            Wait::Discard(HandWait {
                instruction,
                members,
                bounds: count,
            }),
        )
    }

    pub(super) fn internal_discard_answer(
        &mut self,
        execution: &mut Execution,
        wait: &HandWait,
        members: &[ObjectRef],
    ) -> Result<()> {
        let outcome = self.internal_discard_members(execution, members)?;
        execution.receipts.insert(
            wait.instruction.receipt,
            InstructionOutcome::Discard(outcome),
        );
        Ok(())
    }

    pub(super) fn internal_discard_members(
        &mut self,
        execution: &mut Execution,
        members: &[ObjectRef],
    ) -> Result<DiscardOutcome> {
        let mut snapshots = Vec::new();
        for &member in members {
            let id = execution.object_id(member)?.to_owned();
            snapshots.push((
                member,
                super::player_id(&self.object(&id)?.controller)?,
                self.internal_hand_characteristics(&id)?,
            ));
        }
        let (moves, successful) = self.internal_move_members(
            execution,
            members,
            "cemetery",
            ZoneKind::Cemetery,
            MoveMethod::Discard,
            false,
            None,
        )?;
        for (before, player, characters) in snapshots {
            if let Some(actual_to) = moves
                .iter()
                .find(|movement| movement.before == before && movement.after.is_some())
                .and_then(|movement| movement.actual_to)
            {
                execution
                    .facts
                    .push(super::Fact::Discard(Box::new(DiscardEvent {
                        player,
                        card_before: before,
                        characters,
                        actual_to,
                    })));
            }
        }
        Ok(DiscardOutcome { successful, moves })
    }
}
