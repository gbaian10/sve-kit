//! Whole-zone returns and exact draws share the ordinary movement and draw commits.
use super::{
    Execution, Fact, Game, InstructionOutcome, Result, Slot, invalid, player_id, seat_name,
    unsupported,
};
use crate::ir::{
    Action, CardQuantityOutcome, Control, Expr, Instruction, Int, Located, MoveOutcome, Nat, Node,
    ObjectRef, PlayerId, Selector, ZoneKind,
};

#[cfg(test)]
mod tests;
use crate::ir::player_stages::{procedure, returned_share};
use crate::ir::{RuntimeZoneRef, shuffle};
use core::slice::from_ref;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Whole-zone movement requires a closed selector without exposing hand selection."
)]
impl Execution {
    pub(super) fn whole_return_members(
        &self,
        game: &Game,
        instruction: &Instruction,
    ) -> Result<Vec<ObjectRef>> {
        let player = shuffle::whole_return(&instruction.action)
            .ok_or_else(|| unsupported("whole-zone return selector"))?;
        let seat = seat_name(self.player(game, player)?)?;
        let Action::Move { objects, .. } = &instruction.action else {
            return Err(invalid("checked whole-zone move"));
        };
        let selectors = if let Selector::Union { sets } = objects {
            sets.as_slice()
        } else {
            from_ref(objects)
        };
        let mut members = Vec::new();
        for selector in selectors {
            let zone = match selector {
                Selector::Zone {
                    zone: ZoneKind::Hand,
                    ..
                } => "hand",
                Selector::Zone {
                    zone: ZoneKind::Cemetery,
                    ..
                } => "cemetery",
                _ => return Err(invalid("checked whole-zone selector")),
            };
            for id in game.zone_ids(seat, zone) {
                members.push(self.reference(game, &id)?);
            }
        }
        Ok(members)
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "A player's own draw count reads the same stage receipts."
)]
impl Execution {
    fn player_draw_count(&self, count: &Expr<Int>, player: PlayerId) -> Result<u64> {
        if let Expr::Literal(Int(count)) = count {
            return u64::try_from(*count).map_err(invalid);
        }
        let receipt = returned_share(count).ok_or_else(|| unsupported("player draw count"))?;
        let Some(InstructionOutcome::Move(MoveOutcome { moved })) = self.receipts.get(&receipt)
        else {
            return Err(invalid("missing whole return receipt"));
        };
        let own = moved
            .iter()
            .filter(|result| {
                result.actual_to
                    == Some(RuntimeZoneRef {
                        player,
                        kind: ZoneKind::Deck,
                    })
            })
            .count();
        u64::try_from(own).map_err(invalid)
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Player stages use the shared authority transaction."
)]
impl Game {
    pub(super) fn internal_nonchoice_player_stages(
        &mut self,
        execution: &mut Execution,
        control: &Control,
    ) -> Result<()> {
        let (players, binding, stages) =
            procedure(control).ok_or_else(|| unsupported("nonchoice player procedure"))?;
        let mut players = execution.players(self, players)?;
        let active = player_id(self.active())?;
        players.sort_by_key(|player| *player != active);
        for stage in stages {
            for step in &stage.body.steps {
                let Located {
                    node: Node::Instruction { instruction },
                    ..
                } = step
                else {
                    return Err(invalid("checked player instruction"));
                };
                self.internal_instruction_boundary(execution, |game, execution| {
                    match &instruction.action {
                        Action::Draw { count, .. } => {
                            let counts = players
                                .iter()
                                .map(|player| {
                                    Ok((*player, execution.player_draw_count(count, *player)?))
                                })
                                .collect::<Result<Vec<_>>>()?;
                            game.internal_player_draws(execution, instruction, &counts)?;
                        }
                        Action::Move { .. } => {
                            let mut members = Vec::new();
                            for player in &players {
                                execution.slots.insert(binding.key(), Slot::Player(*player));
                                members.extend(execution.whole_return_members(game, instruction)?);
                            }
                            game.internal_whole_return_receipt(execution, instruction, &members)?;
                        }
                        Action::Shuffle { .. } => {
                            for player in &players {
                                execution.slots.insert(binding.key(), Slot::Player(*player));
                                game.internal_shuffle(execution, instruction)?;
                            }
                        }
                        _ => return Err(invalid("checked player stage action")),
                    }
                    Ok(())
                })?;
            }
        }
        execution.slots.remove(&binding.key());
        Ok(())
    }

    fn internal_player_draws(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        counts: &[(PlayerId, u64)],
    ) -> Result<()> {
        let mut actual = 0_u64;
        let longest = counts.iter().map(|(_, count)| *count).max().unwrap_or(0);
        for ordinal in 1..=longest {
            let mut events = Vec::new();
            for (player, count) in counts {
                if ordinal > *count {
                    continue;
                }
                if let Some(event) =
                    self.internal_draw_card(execution, instruction, *player, Nat(ordinal))?
                {
                    events.push(event);
                }
            }
            // Both hands have changed before either player's draw observers capture their event.
            for event in events {
                self.internal_draw_event(event, execution)?;
                execution.record_fact(self, Fact::Draw(event));
                actual = actual
                    .checked_add(1)
                    .ok_or_else(|| invalid("player draw count overflow"))?;
            }
        }
        let requested = counts
            .iter()
            .try_fold(0_u64, |total, (_, count)| total.checked_add(*count))
            .ok_or_else(|| invalid("player draw request overflow"))?;
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Cards(CardQuantityOutcome {
                requested: Int(i64::try_from(requested).map_err(invalid)?),
                actual: Nat(actual),
            }),
        );
        Ok(())
    }

    pub(super) fn internal_whole_return_receipt(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        members: &[ObjectRef],
    ) -> Result<()> {
        let moved = self.internal_whole_deck_return(execution, instruction, members)?;
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Move(MoveOutcome { moved }),
        );
        Ok(())
    }
}
