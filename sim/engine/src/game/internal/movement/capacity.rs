//! Capacity choices use the same planned arrivals and movement receipts as the commit.
use super::super::{
    Execution, Game, InputDomain, OrderCard, Result, SearchPartitionDomain, Wait, invalid,
    seat_name, unsupported,
};
use super::{EX_CAPACITY, FIELD_CAPACITY, MovementPlan};
use crate::ir::{
    Cardinality, DecisionId, Instruction, MoveResult, Nat, Node, ObjectRef, PlayerId,
    RuntimeZoneRef, ZoneKind,
};
use alloc::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct CapacityWait {
    instruction: Box<Instruction>,
    destination: PlayerId,
    zone: ZoneKind,
    members: Vec<ObjectRef>,
    domain: SearchPartitionDomain,
    available: usize,
}

impl CapacityWait {
    pub(in crate::game::internal) fn validate(&self, handles: &[u64]) -> Result<Vec<ObjectRef>> {
        if handles.len() != self.available
            || handles.iter().collect::<BTreeSet<_>>().len() != handles.len()
        {
            return Err(invalid("capacity selection must fill the remaining spaces"));
        }
        handles
            .iter()
            .map(|handle| {
                self.domain
                    .cards
                    .iter()
                    .position(|card| card.handle == *handle)
                    .and_then(|index| self.members.get(index).copied())
                    .ok_or_else(|| invalid("unknown movement capacity handle"))
            })
            .collect()
    }
}

const fn reference_key(member: ObjectRef) -> (u64, u64) {
    (member.id.0, member.generation.0)
}

pub(super) fn complete_receipt(
    original: Vec<MoveResult>,
    moved: Vec<MoveResult>,
) -> Vec<MoveResult> {
    let mut outcomes: BTreeMap<_, _> = moved
        .into_iter()
        .map(|item| (reference_key(item.before), item))
        .collect();
    original
        .into_iter()
        .map(|item| outcomes.remove(&reference_key(item.before)).unwrap_or(item))
        .collect()
}

pub(super) fn failed_field_entry(members: &[ObjectRef], player: PlayerId) -> Vec<MoveResult> {
    members
        .iter()
        .map(|&before| MoveResult {
            before,
            after: None,
            requested_to: RuntimeZoneRef {
                player,
                kind: ZoneKind::Field,
            },
            requested_position: None,
            actual_to: None,
            actual_position: None,
            replacements: Vec::new(),
        })
        .collect()
}

pub(super) fn finish(execution: &mut Execution, instruction: &Instruction) {
    execution
        .movement_capacity
        .retain(|(receipt, _), _| *receipt != instruction.receipt);
}

fn capacity_zone(zone: ZoneKind) -> Result<(&'static str, i64)> {
    match zone {
        ZoneKind::Field => Ok(("field", FIELD_CAPACITY)),
        ZoneKind::Ex => Ok(("ex", EX_CAPACITY)),
        _ => Err(unsupported("movement capacity destination")),
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Capacity questions suspend the existing atomic movement transaction."
)]
impl Game {
    fn movement_free_spaces(&self, player: PlayerId, zone: ZoneKind) -> Result<usize> {
        let (zone, limit) = capacity_zone(zone)?;
        usize::try_from(
            limit
                .saturating_sub(self.zone_count(seat_name(player)?, zone))
                .max(0),
        )
        .map_err(invalid)
    }

    fn movement_capacity_domain(
        &self,
        execution: &Execution,
        members: &[ObjectRef],
        available: usize,
    ) -> Result<SearchPartitionDomain> {
        // A private unobserved deck slice cannot lend its faces to an allocation question.
        let actor = seat_name(execution.context.controller)?;
        for &member in members {
            let id = execution.object_id(member)?;
            let object = self.object(id)?;
            if object.zone == "deck"
                && !self
                    .state
                    .knowledge
                    .get(actor)
                    .is_some_and(|knowledge| knowledge.located.contains(id))
            {
                return Err(unsupported("unobserved deck capacity selection"));
            }
        }
        let cards = members
            .iter()
            .enumerate()
            .map(|(index, &member)| {
                let id = execution.object_id(member)?;
                Ok(OrderCard {
                    handle: u64::try_from(index).map_err(invalid)?,
                    card: self.object(id)?.card.clone(),
                    name: self.card_name(id)?,
                    object: matches!(self.object(id)?.zone.as_str(), "field" | "cemetery" | "ex")
                        .then(|| id.to_owned()),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let count = Nat(u64::try_from(available).map_err(invalid)?);
        Ok(SearchPartitionDomain {
            cards,
            cardinality: Cardinality {
                minimum: count,
                maximum: Some(count),
            },
        })
    }

    pub(super) fn internal_movement_capacity(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        plan: &MovementPlan,
    ) -> Result<Option<Vec<ObjectRef>>> {
        let mut incoming = BTreeMap::<u32, Vec<ObjectRef>>::new();
        for movement in &plan.plans {
            if !movement.stack_replaced {
                let result = plan
                    .moves
                    .get(movement.result_index)
                    .ok_or_else(|| invalid("missing planned movement result"))?;
                incoming
                    .entry(result.requested_to.player.seat)
                    .or_default()
                    .push(result.before);
            }
        }
        let mut kept: BTreeSet<_> = plan
            .moves
            .iter()
            .map(|item| reference_key(item.before))
            .collect();
        for (seat, members) in incoming {
            let destination = PlayerId { seat };
            let zone = plan
                .moves
                .iter()
                .find(|item| item.requested_to.player == destination)
                .ok_or_else(|| invalid("missing movement capacity destination"))?
                .requested_to
                .kind;
            let available = self.movement_free_spaces(destination, zone)?;
            if members.len() <= available {
                continue;
            }
            if available == 0 {
                for member in members {
                    kept.remove(&reference_key(member));
                }
                continue;
            }
            let key = (instruction.receipt, destination.seat);
            if let Some(chosen) = execution.movement_capacity.get(&key) {
                if chosen.len() != available
                    || chosen.iter().any(|member| !members.contains(member))
                {
                    return Err(invalid("movement capacity changed during its choices"));
                }
                for member in members {
                    if !chosen.contains(&member) {
                        kept.remove(&reference_key(member));
                    }
                }
                continue;
            }
            let domain = self.movement_capacity_domain(execution, &members, available)?;
            let wait = CapacityWait {
                instruction: Box::new(instruction.clone()),
                destination,
                zone,
                members,
                domain: domain.clone(),
                available,
            };
            self.internal_wait(
                execution,
                DecisionId(instruction.id.0),
                execution.context.controller,
                InputDomain::SearchPartition(domain),
                Wait::MovementCapacity(Box::new(wait)),
            )?;
            return Ok(None);
        }
        Ok(Some(
            plan.moves
                .iter()
                .filter(|item| kept.contains(&reference_key(item.before)))
                .map(|item| item.before)
                .collect(),
        ))
    }

    pub(in crate::game::internal) fn internal_movement_capacity_answer(
        &self,
        execution: &mut Execution,
        wait: &CapacityWait,
        chosen: Vec<ObjectRef>,
    ) -> Result<()> {
        if self.movement_free_spaces(wait.destination, wait.zone)? != wait.available {
            return Err(invalid("movement capacity changed before its answer"));
        }
        for &member in &wait.members {
            let object = self.object(execution.object_id(member)?)?;
            if object.generation != member.generation.0 {
                return Err(invalid("movement capacity candidate changed generation"));
            }
        }
        execution
            .movement_capacity
            .insert((wait.instruction.receipt, wait.destination.seat), chosen);
        execution.todo.push_front(Node::Instruction {
            instruction: wait.instruction.clone(),
        });
        Ok(())
    }
}
