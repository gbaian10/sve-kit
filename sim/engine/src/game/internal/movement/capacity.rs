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
    resume: Node,
    destination: PlayerId,
    zone: ZoneKind,
    members: Vec<ArrivalMember>,
    domain: SearchPartitionDomain,
    available: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::game::internal) enum ArrivalMember {
    Object(ObjectRef),
    Token(String),
}

impl CapacityWait {
    pub(in crate::game::internal) fn validate(
        &self,
        handles: &[u64],
    ) -> Result<Vec<ArrivalMember>> {
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
                    .and_then(|index| self.members.get(index).cloned())
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
        members: &[ArrivalMember],
        available: usize,
    ) -> Result<SearchPartitionDomain> {
        let actor = seat_name(execution.context.controller)?;
        let cards = members
            .iter()
            .enumerate()
            .map(|(index, member)| {
                let (card, name, object) = match member {
                    ArrivalMember::Object(reference) => {
                        let id = execution.object_id(*reference)?;
                        let object = self.object(id)?;
                        if object.zone == "deck"
                            && !self
                                .state
                                .knowledge
                                .get(actor)
                                .is_some_and(|knowledge| knowledge.located().contains(id))
                        {
                            return Err(unsupported("unobserved deck capacity selection"));
                        }
                        (
                            object.card.clone(),
                            self.card_name(id)?,
                            matches!(object.zone.as_str(), "field" | "cemetery" | "ex" | "banish")
                                .then(|| id.to_owned()),
                        )
                    }
                    ArrivalMember::Token(card) => (
                        card.clone(),
                        self.catalog.face(card, 0)?["name"]
                            .as_str()
                            .ok_or_else(|| invalid("token name"))?
                            .to_owned(),
                        None,
                    ),
                };
                Ok(OrderCard {
                    handle: u64::try_from(index).map_err(invalid)?,
                    card,
                    name,
                    object,
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
        let arrivals = plan
            .plans
            .iter()
            .filter(|movement| !movement.stack_replaced)
            .map(|movement| {
                let result = plan
                    .moves
                    .get(movement.result_index)
                    .ok_or_else(|| invalid("missing planned movement result"))?;
                Ok((ArrivalMember::Object(result.before), result.requested_to))
            })
            .collect::<Result<Vec<_>>>()?;
        let resume = Node::Instruction {
            instruction: Box::new(instruction.clone()),
        };
        let Some(chosen) =
            self.internal_arrival_capacity(execution, instruction, resume, &arrivals)?
        else {
            return Ok(None);
        };
        Ok(Some(
            plan.moves
                .iter()
                .filter(|item| {
                    !arrivals
                        .iter()
                        .any(|(member, _)| *member == ArrivalMember::Object(item.before))
                        || chosen.contains(&ArrivalMember::Object(item.before))
                })
                .map(|item| item.before)
                .collect(),
        ))
    }

    pub(in crate::game::internal) fn internal_arrival_capacity(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        resume: Node,
        arrivals: &[(ArrivalMember, RuntimeZoneRef)],
    ) -> Result<Option<Vec<ArrivalMember>>> {
        let mut incoming = BTreeMap::<u32, Vec<ArrivalMember>>::new();
        for (member, destination) in arrivals {
            incoming
                .entry(destination.player.seat)
                .or_default()
                .push(member.clone());
        }
        let mut kept = arrivals
            .iter()
            .map(|(member, _)| member.clone())
            .collect::<Vec<_>>();
        for (seat, members) in incoming {
            let destination = PlayerId { seat };
            let zone = arrivals
                .iter()
                .find(|(_, zone)| zone.player == destination)
                .ok_or_else(|| invalid("missing movement capacity destination"))?
                .1
                .kind;
            if arrivals
                .iter()
                .any(|(_, other)| other.player == destination && other.kind != zone)
            {
                return Err(unsupported("mixed capacity destination kinds"));
            }
            let available = self.movement_free_spaces(destination, zone)?;
            if members.len() <= available {
                continue;
            }
            if available == 0 {
                for member in members {
                    kept.retain(|kept_member| *kept_member != member);
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
                        kept.retain(|kept_member| *kept_member != member);
                    }
                }
                continue;
            }
            let domain = self.movement_capacity_domain(execution, &members, available)?;
            let wait = CapacityWait {
                instruction: Box::new(instruction.clone()),
                resume,
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
        Ok(Some(kept))
    }

    pub(in crate::game::internal) fn internal_movement_capacity_answer(
        &self,
        execution: &mut Execution,
        wait: &CapacityWait,
        chosen: Vec<ArrivalMember>,
    ) -> Result<()> {
        if self.movement_free_spaces(wait.destination, wait.zone)? != wait.available {
            return Err(invalid("movement capacity changed before its answer"));
        }
        for member in &wait.members {
            let ArrivalMember::Object(member) = member else {
                continue;
            };
            let object = self.object(execution.object_id(*member)?)?;
            if object.generation != member.generation.0 {
                return Err(invalid("movement capacity candidate changed generation"));
            }
        }
        execution
            .movement_capacity
            .insert((wait.instruction.receipt, wait.destination.seat), chosen);
        execution.todo.push_front(wait.resume.clone());
        Ok(())
    }
}
