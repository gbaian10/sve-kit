//! Hand cost choices are collected before any cost is committed.
use super::play::PlayState;
use super::{Execution, Game, Result, invalid, seat_name, unsupported};
use crate::game::internal::{
    DecisionId, DistributionMember, InputDomain, InstructionOutcome, Wait, contains,
    discard::HandWait, search,
};
use crate::ir::{
    Action, Bool, Cardinality, CostSpec, DiscardMode, Disjointness, Expr, Instruction, Int, Nat,
    ObjectOperand, ObjectRef, PlayerRef, PlayerRelation, Quantity, QuantityUnit,
    SelectionConstraint, SelectionView, Selector, ZoneKind,
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
    if group.cardinality != count
        || constraint.disjoint != Disjointness::Disjoint
        || constraint.distinct_by.is_some()
        || !constraint.aggregate_bounds.is_empty()
    {
        return Err(unsupported("hand cost cardinality"));
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

pub(super) fn source_discard(action: &Action) -> bool {
    let Action::Discard {
        player: PlayerRef::Controller {},
        mode: DiscardMode::All {
            eligible: Selector::Intersection { sets },
        },
    } = action
    else {
        return false;
    };
    matches!(sets.as_slice(), [
        Selector::Zone { players, zone: ZoneKind::Hand, view: SelectionView::Current {}, predicate, .. },
        Selector::Objects { objects },
    ] if players.relation == PlayerRelation::SelfPlayer
        && players.relative_to == (PlayerRef::Controller {})
        && players.cardinality == (Cardinality { minimum: Nat(1), maximum: Some(Nat(1)) })
        && **predicate == Expr::Literal(Bool(true))
        && objects == &[ObjectOperand::SourceHolder {}])
}

pub(super) fn check_cost(action: &Action) -> bool {
    source_discard(action) || selection(action).is_ok()
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Hand cost choice and commit share the play transaction."
)]
impl Game {
    pub(super) fn internal_play_hand_materials(
        &mut self,
        execution: &mut Execution,
        play: &PlayState,
    ) -> Result<bool> {
        if self.internal_hand_cost_materials(execution, &play.costs)? {
            return Ok(true);
        }
        if let Some(payment) = &play.optional
            && payment.accepted == Some(true)
        {
            return self.internal_hand_cost_materials(execution, &payment.costs);
        }
        Ok(false)
    }

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
                || source_discard(&instruction.action)
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
            let source_reserved = steps
                .iter()
                .any(|cost| source_discard(&cost.instruction.action));
            let seat = seat_name(execution.context.controller)?;
            let mut members = vec![];
            let mut candidates = vec![];
            for id in self.zone_ids(seat, "hand") {
                if source_reserved && id == execution.context.source {
                    continue;
                }
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
                    distinct_names: false,
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
        let members = if source_discard(&instruction.action) {
            vec![execution.object(self, &ObjectOperand::SourceHolder {})?]
        } else {
            let constraint = selection(&instruction.action)?;
            let members = execution
                .cost_discards
                .remove(&instruction.receipt)
                .ok_or_else(|| invalid("missing hand cost selection"))?;
            if !contains(
                constraint.cardinality,
                Nat(u64::try_from(members.len()).map_err(invalid)?),
            ) {
                return Err(invalid("incomplete hand cost selection"));
            }
            members
        };
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
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Discard(outcome));
        Ok((
            Quantity {
                amount: Int(i64::try_from(members.len()).map_err(invalid)?),
                unit: QuantityUnit::Cards,
            },
            members,
        ))
    }
}
