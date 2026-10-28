//! Hand cost choices are collected before any cost is committed.
use super::{Execution, Game, Result, invalid, seat_name, unsupported};
use crate::game::internal::{
    DecisionId, DistributionMember, InputDomain, Wait, discard::HandWait, search,
};
use crate::ir::{
    Action, Cardinality, CostSpec, DiscardMode, Disjointness, Instruction, Int, Nat, ObjectRef,
    PlayerRef, PlayerRelation, Quantity, QuantityUnit, SelectionConstraint, SelectionView,
    Selector, ZoneKind,
};

fn selection(action: &Action) -> Result<&SelectionConstraint> {
    let Action::Discard {
        player: PlayerRef::Controller {},
        mode: DiscardMode::Chosen { constraint },
    } = action
    else {
        return Err(unsupported("chosen own hand cost"));
    };
    let [group] = constraint.groups.as_slice() else {
        return Err(unsupported("one hand cost group"));
    };
    let count = constraint.cardinality;
    if count.minimum.0 == 0
        || count.maximum != Some(count.minimum)
        || group.cardinality != count
        || constraint.disjoint != Disjointness::Disjoint
        || constraint.distinct_by.is_some()
        || !constraint.aggregate_bounds.is_empty()
    {
        return Err(unsupported("fixed hand cost count"));
    }
    let Selector::Zone {
        players,
        zone: ZoneKind::Hand,
        view: SelectionView::Current {},
        binding,
        predicate,
    } = &group.selector
    else {
        return Err(unsupported("current own hand cost selector"));
    };
    if players.relation != PlayerRelation::SelfPlayer
        || players.relative_to != (PlayerRef::Controller {})
        || players.cardinality
            != (Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            })
    {
        return Err(unsupported("own hand cost player"));
    }
    search::check_predicate(predicate, *binding)?;
    Ok(constraint)
}

pub(super) fn check_cost(action: &Action) -> bool {
    selection(action).is_ok()
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Hand cost choice and commit share the play transaction."
)]
impl Game {
    pub(super) fn internal_hand_cost_materials(
        &mut self,
        execution: &mut Execution,
        costs: &CostSpec,
    ) -> Result<bool> {
        let CostSpec::Ordered { steps } = costs else {
            return Err(unsupported("ordered hand costs"));
        };
        for step in steps {
            let instruction = &step.instruction;
            if !matches!(instruction.action, Action::Discard { .. })
                || execution.cost_discards.contains_key(&instruction.receipt)
            {
                continue;
            }
            let constraint = selection(&instruction.action)?;
            let group = constraint
                .groups
                .first()
                .ok_or_else(|| invalid("hand cost group"))?;
            let Selector::Zone { predicate, .. } = &group.selector else {
                return Err(unsupported("hand cost selector"));
            };
            let seat = seat_name(execution.context.controller)?;
            let mut members = vec![];
            let mut candidates = vec![];
            for id in self.zone_ids(seat, "hand") {
                let object = self.object(&id)?;
                if object.owner != seat || object.controller != seat {
                    return Err(unsupported("foreign hand owner"));
                }
                if search::matches_candidate(self, &id, predicate)? {
                    candidates.push(DistributionMember {
                        handle: u64::try_from(members.len()).map_err(invalid)?,
                        player: execution.context.controller,
                        card: Some(object.card.clone()),
                        name: Some(self.card_name(&id)?),
                    });
                    members.push(execution.reference(self, &id)?);
                }
            }
            if u64::try_from(members.len()).map_err(invalid)? < constraint.cardinality.minimum.0 {
                return Err(crate::EngineFailure::IllegalPlay(
                    "insufficient hand cost materials".into(),
                ));
            }
            self.internal_wait(
                execution,
                DecisionId(instruction.id.0),
                execution.context.controller,
                InputDomain::CostMaterials(super::TargetDomain {
                    members: candidates,
                    cardinality: constraint.cardinality,
                }),
                Wait::CostDiscard(HandWait::strict(
                    instruction.clone(),
                    members,
                    constraint.cardinality,
                )),
            )?;
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn internal_hand_cost_payment(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<(Quantity, Vec<ObjectRef>)> {
        let constraint = selection(&instruction.action)?;
        let members = execution
            .cost_discards
            .remove(&instruction.receipt)
            .ok_or_else(|| invalid("missing hand cost selection"))?;
        if u64::try_from(members.len()).map_err(invalid)? != constraint.cardinality.minimum.0 {
            return Err(invalid("incomplete hand cost selection"));
        }
        let seat = seat_name(execution.context.controller)?;
        for member in &members {
            let object = self.object(execution.object_id(*member)?)?;
            if object.generation != member.generation.0
                || object.zone != "hand"
                || object.owner != seat
                || object.controller != seat
            {
                return Err(crate::EngineFailure::IllegalPlay(
                    "hand cost materials changed".into(),
                ));
            }
        }
        let outcome = self.internal_discard_members(execution, &members)?;
        if outcome.moves.len() != members.len()
            || outcome
                .moves
                .iter()
                .any(|movement| movement.after.is_none())
        {
            return Err(crate::EngineFailure::IllegalPlay(
                "incomplete hand cost payment".into(),
            ));
        }
        Ok((
            Quantity {
                amount: Int(i64::try_from(members.len()).map_err(invalid)?),
                unit: QuantityUnit::Cards,
            },
            members,
        ))
    }
}
