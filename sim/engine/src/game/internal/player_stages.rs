//! Whole-zone returns and exact draws share the ordinary movement and draw commits.
use super::{
    Execution, Fact, Game, InstructionOutcome, Result, Slot, invalid, player_id, seat_name,
    unsupported,
};
use crate::ir::walk::{self, NodeRef, Traversal, Visitor};
use crate::ir::{
    Action, CardQuantityOutcome, Control, Expr, Instruction, Int, Located, MoveOutcome, Nat, Node,
    ObjectRef, PlayerId, PlayerStage, ReceiptId, ScopeId, Selector, ZoneKind,
};
use alloc::collections::{BTreeMap, BTreeSet};

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

/// Each player's own results inside a choice procedure: a later stage reads the choices,
/// captures and receipts that player made, never those the previous player left in the same
/// bindings. Nested procedures stack inside the outer player's environment.
#[derive(Clone, Debug)]
pub(super) struct ProcedureLocals {
    player: (ScopeId, u64),
    scopes: BTreeSet<ScopeId>,
    receipts: BTreeSet<ReceiptId>,
    saved: BTreeMap<u32, Locals>,
}

#[derive(Clone, Debug, Default)]
struct Locals {
    slots: Vec<((ScopeId, u64), Slot)>,
    targets: Vec<((ScopeId, u64), super::card::selection::TargetWait)>,
    receipts: Vec<(ReceiptId, InstructionOutcome)>,
    // A Look receipt keeps its viewer and the candidate identities captured when it looked;
    // knowledge still revokes the observation after a shuffle or other deck change.
    look_viewers: Vec<(ReceiptId, PlayerId)>,
    receipt_links: Vec<(ReceiptId, super::player::CandidateLinks)>,
}

/// Every lexical scope and receipt slot a procedure's stage bodies can produce.
#[derive(Default)]
struct LocalNames {
    scopes: BTreeSet<ScopeId>,
    receipts: BTreeSet<ReceiptId>,
}

impl Visitor for LocalNames {
    fn enter(&mut self, node: NodeRef<'_>, _path: &str) -> Traversal {
        match node {
            NodeRef::Program(program) => {
                self.scopes.insert(program.scope);
            }
            NodeRef::Instruction(instruction) => {
                self.receipts.insert(instruction.receipt);
            }
            NodeRef::Control(
                Control::May { receipt, .. }
                | Control::Pay { receipt, .. }
                | Control::Simultaneous { receipt, .. },
            ) => {
                self.receipts.insert(*receipt);
            }
            NodeRef::Control(_)
            | NodeRef::Source(_)
            | NodeRef::Ability(_)
            | NodeRef::Action(_)
            | NodeRef::Effect(_)
            | NodeRef::CostSpec(_)
            | NodeRef::ObjectOperand(_) => {}
        }
        Traversal::Descend
    }
}

impl ProcedureLocals {
    pub(super) fn new(player: (ScopeId, u64), stages: &[PlayerStage]) -> Self {
        let mut names = LocalNames::default();
        for stage in stages {
            walk::program(&stage.body, "", &mut names);
        }
        Self {
            player,
            scopes: names.scopes,
            receipts: names.receipts,
            saved: BTreeMap::new(),
        }
    }
}

fn take<K, V, Local>(map: &mut BTreeMap<K, V>, local: Local) -> Vec<(K, V)>
where
    K: Ord + Copy,
    Local: Fn(&K) -> bool,
{
    let keys = map
        .keys()
        .copied()
        .filter(|key| local(key))
        .collect::<Vec<_>>();
    keys.into_iter()
        .filter_map(|key| map.remove(&key).map(|value| (key, value)))
        .collect()
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Switching the procedure player swaps only that procedure's local results."
)]
impl Execution {
    pub(super) fn begin_procedure(&mut self, locals: ProcedureLocals) {
        self.procedures.push(locals);
    }

    /// Switching an outer procedure's player ends every procedure nested inside it.
    /// Revoking a look also reaches the looks saved for procedure players not acting now, so
    /// a later look cannot revive one that a shuffle or deck change ended.
    pub(super) fn retain_look_viewers(&mut self, keep: impl Fn(PlayerId) -> bool) {
        self.look_viewers.retain(|_, viewer| keep(*viewer));
        for procedure in &mut self.procedures {
            for locals in procedure.saved.values_mut() {
                locals.look_viewers.retain(|(_, viewer)| keep(*viewer));
            }
        }
    }

    pub(super) fn switch_procedure_player(&mut self, key: (ScopeId, u64), next: PlayerId) {
        let Some(index) = self
            .procedures
            .iter()
            .rposition(|procedure| procedure.player == key)
        else {
            return;
        };
        self.procedures.truncate(index.saturating_add(1));
        let Some(mut procedure) = self.procedures.pop() else {
            return;
        };
        let local = |entry: &(ScopeId, u64)| procedure.scopes.contains(&entry.0);
        let current = Locals {
            slots: take(&mut self.slots, local),
            targets: take(&mut self.targets, local),
            receipts: take(&mut self.receipts, |id| procedure.receipts.contains(id)),
            look_viewers: take(&mut self.look_viewers, |id| procedure.receipts.contains(id)),
            receipt_links: take(&mut self.player_receipt_links, |id| {
                procedure.receipts.contains(id)
            }),
        };
        if let Some(Slot::Player(previous)) = self.slots.get(&key) {
            procedure.saved.insert(previous.seat, current);
        }
        let own = procedure.saved.remove(&next.seat).unwrap_or_default();
        self.slots.extend(own.slots);
        self.targets.extend(own.targets);
        self.receipts.extend(own.receipts);
        self.look_viewers.extend(own.look_viewers);
        self.player_receipt_links.extend(own.receipt_links);
        self.procedures.push(procedure);
    }
}
