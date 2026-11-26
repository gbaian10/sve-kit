//! Only the owner inspects face-down payment candidates; flipping publishes through Game.
use super::super::conditions::{ConditionPlan, Policy};
use super::super::search;
use super::{Execution, Game, Result, seat_name, unsupported};
use crate::ir::{Decision, ObjectRef, PlaySpec, Selector, flip_cost};

impl Game {
    pub(super) fn flip_payment_available(
        &self,
        execution: &Execution,
        payment: &PlaySpec,
        parameters: &[Decision],
    ) -> Result<bool> {
        let Some((first, rest)) = parameters.split_first() else {
            let mut trial = self.clone();
            let mut paid = execution.clone();
            trial.card_runtime.active_source = paid.ability;
            return trial.internal_payment_probe(&mut paid, &payment.costs, false);
        };
        for (key, slot) in self.internal_public_material_choices(execution, first, false)? {
            let mut selected = execution.clone();
            selected.slots.insert(key, slot);
            if self.flip_payment_available(&selected, payment, rest)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn flip_cost_candidates(
        &self,
        execution: &Execution,
        selector: &Selector,
    ) -> Result<Vec<ObjectRef>> {
        if flip_cost::selector(selector).is_none() {
            return Err(unsupported("flip material selector"));
        }
        let Selector::Zone {
            players,
            binding,
            predicate,
            ..
        } = selector
        else {
            return Err(unsupported("flip material zone"));
        };
        let condition =
            ConditionPlan::compile(predicate, *binding, Policy::Selection)?.prepare(None)?;
        let mut members = Vec::new();
        for player in execution.players(self, players)? {
            let seat = seat_name(player)?;
            for id in self.zone_ids(seat, "evolve_deck") {
                let object = self.object(&id)?;
                if object.owner == seat
                    && object.controller == seat
                    && search::matches_candidate(self, &id, &condition)?
                {
                    members.push(execution.reference(self, &id)?);
                }
            }
        }
        Ok(members)
    }
}
