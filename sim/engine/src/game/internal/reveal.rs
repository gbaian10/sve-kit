//! Whole-hand public visibility is local to one resolution and one hidden location.

use super::{Execution, Game, InstructionOutcome, Result, invalid, seat_name, unsupported};
use crate::ir::{
    Bool, Expr, Instruction, Nat, ObjectRef, PlayerRef, PlayerRelation, SelectionView, Selector,
    ZoneKind,
};
use core::mem::take;

#[derive(Clone, Debug)]
pub(super) struct PublicHand {
    object: ObjectRef,
    id: String,
    controller: String,
    viewers: Vec<String>,
}

pub(super) fn check_objects(objects: &Selector) -> Result<()> {
    let Selector::Zone {
        players,
        zone: ZoneKind::Hand,
        view: SelectionView::Current {},
        predicate,
        ..
    } = objects
    else {
        return Err(unsupported("direct whole-hand reveal"));
    };
    if players.relative_to != (PlayerRef::Controller {})
        || !matches!(
            players.relation,
            PlayerRelation::SelfPlayer | PlayerRelation::Opponents
        )
        || players.cardinality.minimum != Nat(1)
        || players.cardinality.maximum != Some(Nat(1))
        || **predicate != Expr::Literal(Bool(true))
    {
        return Err(unsupported("one unfiltered hand reveal"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Scoped revelation shares the authoritative Game visibility state."
)]
impl Game {
    pub(super) fn internal_reveal(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        objects: &Selector,
    ) -> Result<()> {
        check_objects(objects)?;
        let Selector::Zone { players, .. } = objects else {
            return Err(unsupported("whole hand reveal"));
        };
        let mut members = Vec::new();
        for player in execution.players(self, players)? {
            for id in self.zone_ids(seat_name(player)?, "hand") {
                members.push(execution.reference(self, &id)?);
            }
        }
        let mut ids = Vec::new();
        for member in &members {
            let id = execution.object_id(*member)?.to_owned();
            let object = self.object(&id)?;
            if object.zone != "hand" || object.controller != object.owner {
                return Err(invalid("foreign hand reveal member"));
            }
            let controller = object.controller.clone();
            let viewers = ["P1", "P2"]
                .into_iter()
                .filter(|viewer| {
                    self.state
                        .knowledge
                        .get(*viewer)
                        .is_none_or(|known| !known.located.contains(&id))
                })
                .map(str::to_owned)
                .collect();
            execution.reveals.push(PublicHand {
                object: *member,
                id: id.clone(),
                controller,
                viewers,
            });
            ids.push(id);
        }
        self.reveal(&ids, "all", &execution.frame(), true);
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Revealed(members));
        Ok(())
    }

    pub(super) fn internal_reveal_expiry(&mut self, execution: &mut Execution, end: bool) {
        let mut active = Vec::new();
        for grant in take(&mut execution.reveals) {
            let Some(current) = self.state.objects.get(&grant.id) else {
                for viewer in grant.viewers {
                    if let Some(known) = self.state.knowledge.get_mut(&viewer) {
                        known.located.remove(&grant.id);
                    }
                }
                continue;
            };
            let same_location = current.generation == grant.object.generation.0
                && current.controller == grant.controller
                && current.zone == "hand";
            if !end && same_location {
                active.push(grant);
                continue;
            }
            if matches!(current.zone.as_str(), "hand" | "deck" | "evolve_deck") {
                let owner = current.owner.clone();
                let own_hand = current.zone == "hand";
                for viewer in grant.viewers {
                    if !(own_hand && viewer == owner)
                        && let Some(known) = self.state.knowledge.get_mut(&viewer)
                    {
                        // Remember the printed card, but never locate its later private slot.
                        known.located.remove(&grant.id);
                    }
                }
            }
        }
        execution.reveals = active;
    }
}
