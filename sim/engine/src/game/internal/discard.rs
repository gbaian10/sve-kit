//! The chooser's hand is exposed only through token-local candidates for this discard.

use super::{
    Execution, Game, InputDomain, InstructionOutcome, Result, Wait, contains, invalid, seat_name,
    unsupported,
};
use crate::ir::{
    Action, Bool, Cardinality, DecisionId, DiscardEvent, DiscardMode, DiscardOutcome, Disjointness,
    Expr, Instruction, MoveMethod, Nat, ObjectRef, PlayerRef, PlayerRelation, SelectionView,
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
    if matches!(action, Action::Discard { .. }) {
        bounds(action)?;
    }
    Ok(())
}

fn bounds(action: &Action) -> Result<Cardinality> {
    let Action::Discard {
        player: PlayerRef::Controller {},
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
    if players.relative_to != (PlayerRef::Controller {})
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
        let seat = seat_name(execution.context.controller)?;
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
        let mut count = bounds(&instruction.action)?;
        let player = execution.context.controller;
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
        let mut snapshots = Vec::new();
        for &member in members {
            let id = execution.object_id(member)?.to_owned();
            snapshots.push((member, self.internal_hand_characteristics(&id)?));
        }
        let (moves, successful) = self.internal_move_members(
            execution,
            members,
            "cemetery",
            ZoneKind::Cemetery,
            MoveMethod::Discard,
        )?;
        for (before, characters) in snapshots {
            if let Some(actual_to) = moves
                .iter()
                .find(|movement| movement.before == before && movement.after.is_some())
                .and_then(|movement| movement.actual_to)
            {
                execution
                    .facts
                    .push(super::Fact::Discard(Box::new(DiscardEvent {
                        player: execution.context.controller,
                        card_before: before,
                        characters,
                        actual_to,
                    })));
            }
        }
        execution.receipts.insert(
            wait.instruction.receipt,
            InstructionOutcome::Discard(DiscardOutcome { successful, moves }),
        );
        Ok(())
    }
}
