//! Hand cost choices are collected before any cost is committed.
use super::play::PlayState;
use super::{Execution, Game, Result, invalid, seat_name, unsupported};
use crate::game::internal::{InstructionOutcome, contains, search};

mod choices;
use crate::ir::{
    Action, Bool, Cardinality, CostSpec, DiscardMode, Disjointness, Expr, Instruction, Int,
    MoveDestination, MoveReason, Nat, ObjectOperand, ObjectRef, OwnerZone, PlayerRef,
    PlayerRelation, Quantity, QuantityUnit, SelectionGroup, SelectionView, Selector, ZoneKind,
};

struct HandSelection<'action> {
    groups: &'action [SelectionGroup],
    cardinality: Cardinality,
    source: bool,
}

fn selection(action: &Action) -> Result<HandSelection<'_>> {
    let Action::Discard {
        player: PlayerRef::Controller {},
        mode: DiscardMode::Chosen { constraint },
    } = action
    else {
        return Err(unsupported("chosen own hand cost"));
    };
    let one = Cardinality {
        minimum: Nat(1),
        maximum: Some(Nat(1)),
    };
    let (groups, source, cardinality) = match constraint.groups.as_slice() {
        [group] if group.cardinality == constraint.cardinality => {
            (constraint.groups.as_slice(), false, constraint.cardinality)
        }
        [own, group]
            if own.cardinality == one
                && group.cardinality == one
                && constraint.cardinality
                    == (Cardinality {
                        minimum: Nat(2),
                        maximum: Some(Nat(2)),
                    })
                && source_selector(&own.selector) =>
        {
            (&constraint.groups[1..], true, one)
        }
        groups
            if matches!(groups.len(), 2 | 3)
                && groups.iter().all(|group| group.cardinality == one)
                && constraint.cardinality.minimum.0
                    == u64::try_from(groups.len()).map_err(invalid)?
                && constraint.cardinality.maximum == Some(constraint.cardinality.minimum) =>
        {
            (groups, false, constraint.cardinality)
        }
        _ => return Err(unsupported("hand cost groups")),
    };
    if constraint.disjoint != Disjointness::Disjoint
        || constraint.distinct_by.is_some()
        || !constraint.aggregate_bounds.is_empty()
    {
        return Err(unsupported("hand cost cardinality"));
    }
    for group in groups {
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
            || players.cardinality != one
        {
            return Err(unsupported("own hand cost player"));
        }
        search::check_predicate(predicate, *binding)?;
    }
    Ok(HandSelection {
        groups,
        cardinality,
        source,
    })
}

fn source_selector(selector: &Selector) -> bool {
    let Selector::Intersection { sets } = selector else {
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

pub(super) fn source_discard(action: &Action) -> bool {
    matches!(action, Action::Discard {
        player: PlayerRef::Controller {}, mode: DiscardMode::All { eligible },
    } if source_selector(eligible))
}

/// 「手札のこれをEXエリアに置く」 moves only the activating hand card to its controller's EX.
pub(in crate::game::internal) fn source_to_ex(action: &Action) -> bool {
    matches!(action, Action::Move { objects, movement, reason: MoveReason::Put }
        if source_selector(objects)
            && movement.to
                == (MoveDestination::Owners { zone: OwnerZone::Ex })
            && movement.position.is_none()
            && movement.order.is_none()
            && movement.initial_posture.is_none()
            && movement.entry_effects.is_empty())
}

pub(super) fn check_cost(action: &Action) -> bool {
    source_discard(action) || selection(action).is_ok()
}

pub(super) fn single_additional(costs: &CostSpec) -> bool {
    let CostSpec::Ordered { steps } = costs else {
        return false;
    };
    let [step] = steps.as_slice() else {
        return false;
    };
    // The Quick quote covers another hand card; its Spell source is already in Resolution.
    selection(&step.instruction.action).is_ok_and(|plan| {
        !plan.source
            && plan.groups.len() == 1
            && plan.cardinality
                == (Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                })
    })
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
        let mut costs = Vec::new();
        if let Some((_, additional)) = play.additional.as_deref() {
            costs.push(&additional.play.costs);
        }
        costs.push(&play.costs);
        let early_hand = costs.iter().any(|cost| matches!(cost, CostSpec::Ordered { steps }
            if steps.iter().any(|step| matches!(step.instruction.action, Action::Discard { mode: DiscardMode::Chosen { .. }, .. })
                && !execution.cost_materials.contains_key(&step.instruction.receipt))));
        costs.extend(
            play.optional
                .iter()
                .filter(|payment| payment.accepted == Some(true))
                .map(|payment| &payment.costs),
        );
        // Fusion materials keep their original place before accepted optional hand fees.
        if !early_hand && self.internal_fusion_materials(execution, &play.costs)? {
            return Ok(true);
        }
        self.internal_hand_payment_question(execution, &costs)
    }

    /// Finds a disjoint assignment before any of the ordered costs are committed.
    pub(super) fn internal_probe_hand_costs(
        &self,
        execution: &mut Execution,
        costs: &CostSpec,
    ) -> Result<()> {
        self.internal_probe_hand_payments(execution, &[costs])
    }

    pub(super) fn internal_hand_cost_materials(
        &mut self,
        execution: &mut Execution,
        costs: &CostSpec,
    ) -> Result<bool> {
        self.internal_hand_payment_question(execution, &[costs])
    }

    /// A hand-source cost is payable only while the activating card is still in its owner's hand.
    pub(super) fn internal_hand_source(&self, execution: &Execution) -> Result<()> {
        let seat = seat_name(execution.context.controller)?;
        let source = self.object(&execution.context.source)?;
        if source.zone != "hand" || source.owner != seat || source.controller != seat {
            return Err(crate::EngineFailure::IllegalPlay(
                "hand source unavailable".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn internal_hand_cost_payment(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<(Quantity, Vec<ObjectRef>)> {
        let members = if source_discard(&instruction.action) {
            vec![execution.object(self, &ObjectOperand::SourceHolder {})?]
        } else {
            let plan = selection(&instruction.action)?;
            let mut members = execution
                .cost_materials
                .remove(&instruction.receipt)
                .ok_or_else(|| invalid("missing hand cost selection"))?;
            if !contains(
                plan.cardinality,
                Nat(u64::try_from(members.len()).map_err(invalid)?),
            ) {
                return Err(invalid("incomplete hand cost selection"));
            }
            let proof = execution
                .hand_cost_groups
                .remove(&instruction.receipt)
                .ok_or_else(|| invalid("missing hand cost proof"))?;
            if !proof.accepts(&members) {
                return Err(invalid("hand cost group proof"));
            }
            if plan.source {
                let source = execution.object(self, &ObjectOperand::SourceHolder {})?;
                if members.contains(&source) {
                    return Err(invalid("grouped hand source selected twice"));
                }
                members.insert(0, source);
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
