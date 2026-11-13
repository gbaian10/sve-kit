//! Closed placements share ordinary movement, deck ordering, and the existing RNG.
use super::{Execution, Fact, Game, InstructionOutcome, Result, invalid, seat_name, unsupported};
use crate::ir::observation::Consumer;
use crate::ir::shuffle::{self, Placement};
use crate::ir::{
    Action, Instruction, MoveOutcome, MoveResult, ObjectRef, PlayerId, Position, PositiveNat,
    RuntimeZoneRef, Selector, ShuffleScope, ZoneKind,
};
use core::num::NonZeroU64;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Shuffling uses the existing game transaction and random authority."
)]
impl Game {
    pub(super) fn internal_shuffle(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        if !shuffle::action(&instruction.action) {
            return Err(unsupported("shuffle shape"));
        }
        let Action::Shuffle { scope, deck, .. } = &instruction.action else {
            return Err(unsupported("shuffle action"));
        };
        let player = execution.player(self, deck)?;
        let seat = seat_name(player)?;
        let ids = match scope {
            ShuffleScope::FullZone {} => self.internal_known_deck(seat)?,
            ShuffleScope::Subset { .. } => {
                let receipt = shuffle::consumed(&instruction.action)
                    .ok_or_else(|| invalid("missing shuffle placement receipt"))?;
                let Some(InstructionOutcome::Move(outcome)) = execution.receipts.get(&receipt)
                else {
                    return Err(invalid("missing temporary deck placement"));
                };
                // Generic movement follow requires a new generation; this closed pair only repositions.
                let members = outcome
                    .moved
                    .iter()
                    .map(|movement| {
                        let zone = RuntimeZoneRef {
                            player,
                            kind: ZoneKind::Deck,
                        };
                        if movement.after != Some(movement.before)
                            || movement.actual_to != Some(zone)
                            || movement.requested_to != zone
                            || movement.requested_position != Some(Position::Bottom {})
                            || movement.actual_position.is_none()
                            || !movement.replacements.is_empty()
                        {
                            return Err(invalid(
                                "shuffle did not follow its exact same-deck placement",
                            ));
                        }
                        Ok(movement.before)
                    })
                    .collect::<Result<Vec<_>>>()?;
                self.shuffle_member_ids(execution, &members, player, false)?
            }
        };
        self.shuffle(seat, &ids)?;
        if matches!(scope, ShuffleScope::FullZone {}) {
            execution.retain_look_viewers(|_| false);
            execution.record_fact(self, Fact::Shuffled { player });
        }
        if let Some(receipt) = shuffle::consumed(&instruction.action) {
            execution.receipts.remove(&receipt);
        }
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Shuffled);
        Ok(())
    }

    fn shuffle_member_ids(
        &self,
        execution: &Execution,
        members: &[ObjectRef],
        player: PlayerId,
        observed: bool,
    ) -> Result<Vec<String>> {
        let seat = seat_name(player)?;
        members
            .iter()
            .map(|&member| {
                let id = execution.object_id(member)?;
                let current = self.object(id)?;
                if current.generation != member.generation.0
                    || current.zone != "deck"
                    || current.owner != seat
                    || current.controller != seat
                    || (observed
                        && !self
                            .state
                            .knowledge
                            .get(seat)
                            .is_some_and(|knowledge| knowledge.knows_location(id)))
                {
                    return Err(unsupported(
                        "shuffle placement requires current own-deck evidence",
                    ));
                }
                Ok(id.to_owned())
            })
            .collect()
    }

    pub(super) fn internal_shuffle_placement(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        let Action::Move { objects, .. } = &instruction.action else {
            return Err(unsupported("shuffle placement"));
        };
        let moved = match shuffle::placement(&instruction.action) {
            Some(Placement::Return) => {
                if shuffle::whole_return(&instruction.action).is_some() {
                    let members = execution.whole_return_members(self, instruction)?;
                    return self.internal_whole_return_receipt(execution, instruction, &members);
                }
                let members = execution.objects(self, objects)?;
                let seat = seat_name(execution.context.controller)?;
                for &member in &members {
                    let current = self.object(execution.object_id(member)?)?;
                    if current.generation != member.generation.0 {
                        continue;
                    }
                    if !matches!(current.zone.as_str(), "cemetery" | "banish")
                        || (current.zone == "banish" && current.state["face_up"] == false)
                        || current.owner != seat
                        || current.controller != seat
                    {
                        return Err(unsupported(
                            "shuffle return requires own public cemetery or banish cards",
                        ));
                    }
                }
                self.internal_unordered_deck_return(execution, instruction, &members)?
            }
            Some(Placement::Bottom) => self.internal_bottom_placement(execution, objects)?,
            None => return Err(unsupported("shuffle placement shape")),
        };
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Move(MoveOutcome { moved }),
        );
        Ok(())
    }

    fn internal_bottom_placement(
        &mut self,
        execution: &Execution,
        objects: &Selector,
    ) -> Result<Vec<MoveResult>> {
        let player = execution.context.controller;
        let seat = seat_name(player)?;
        let members = execution.observed_objects(self, objects, player, Consumer::BottomShuffle)?;
        let ids = self.shuffle_member_ids(execution, &members, player, true)?;
        let original = self.internal_known_deck(seat)?;
        self.internal_reorder_deck(seat, &original, &ids, Position::Bottom {})?;
        let deck = self.internal_known_deck(seat)?;
        // This is positioning within one deck, so references survive and no movement event occurs.
        members
            .into_iter()
            .zip(ids)
            .map(|(member, id)| {
                let index = deck
                    .iter()
                    .position(|current| *current == id)
                    .ok_or_else(|| invalid("missing positioned card"))?;
                let position = u64::try_from(index)
                    .map_err(invalid)?
                    .checked_add(1)
                    .and_then(NonZeroU64::new)
                    .ok_or_else(|| invalid("deck position overflow"))?;
                let zone = RuntimeZoneRef {
                    player,
                    kind: ZoneKind::Deck,
                };
                Ok(MoveResult {
                    before: member,
                    after: Some(member),
                    requested_to: zone,
                    requested_position: Some(Position::Bottom {}),
                    actual_to: Some(zone),
                    actual_position: Some(Position::Index {
                        index: PositiveNat(position),
                    }),
                    replacements: vec![],
                })
            })
            .collect()
    }
}
