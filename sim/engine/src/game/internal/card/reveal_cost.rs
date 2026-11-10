//! Selected hand costs revalidate hidden locations before publishing any identity.
use super::{Execution, Game, InstructionOutcome, Result, invalid, seat_name};
use crate::game::internal::Slot;
use crate::ir::reveal_cost as reveal_payment;
use crate::ir::{
    Action, Characteristics, CostSpec, Instruction, InstructionContext, Int, MoveMethod,
    MoveOutcome, ObjectRef, Position, Quantity, QuantityUnit, Selector, ZoneKind,
};

pub(super) fn selected(execution: &Execution, action: &Action) -> bool {
    matches!(reveal_payment::objects(action), Some(Selector::Bound { reference })
        if execution.material_selections.get(&reference.key()).is_some_and(super::material::MaterialWait::is_hand))
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Revelation uses the existing play payment transaction."
)]
impl Game {
    pub(super) fn check_hand_material_cost_entry(
        &self,
        execution: &Execution,
        costs: &CostSpec,
    ) -> Result<()> {
        if !matches!(costs, CostSpec::Ordered { steps } if steps.iter().any(|step| selected(execution, &step.instruction.action)))
        {
            return Ok(());
        }
        let authority = execution
            .ability
            .ok_or_else(|| invalid("missing reveal ability source"))?;
        // Pending triggers retain their original printing after the holder leaves or changes form.
        let printing = self
            .card_runtime
            .instances
            .iter()
            .find_map(|((_, _, printing, _, _), instance)| {
                (*instance == authority.instance).then_some(printing)
            })
            .ok_or_else(|| invalid("missing reveal source instance"))?;
        let card = self
            .card_runtime
            .definitions
            .cards
            .get(printing)
            .ok_or_else(|| invalid("missing reveal cost card"))?;
        let field = card.card().abilities.iter().any(|ability| {
            ability.node.id == authority.definition
                && reveal_payment::field_cost(&ability.node, costs)
        });
        if !field
            && !card.card().card_play_costs.iter().any(|face| {
                face.node.additional.as_ref().is_some_and(|payment| {
                    payment.node.play.costs == *costs && reveal_payment::payment(&payment.node.play)
                })
            })
        {
            return Err(invalid("hand materials require their declared cost entry"));
        }
        Ok(())
    }

    pub(super) fn internal_hand_material_cost_payment(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<(Quantity, Vec<ObjectRef>, Vec<Characteristics>)> {
        let objects = reveal_payment::objects(&instruction.action)
            .ok_or_else(|| invalid("revelation action"))?;
        let Selector::Bound { reference } = objects else {
            return Err(invalid("bound revelation materials"));
        };
        let Some(Slot::Objects { members, .. }) = execution.slots.get(&reference.key()) else {
            return Err(invalid("missing revelation material selection"));
        };
        let members = members.clone();
        execution
            .material_selections
            .get(&reference.key())
            .ok_or_else(|| invalid("missing reveal selection proof"))?
            .revalidate(self, execution, &members)?;
        if !matches!(members.len(), 1 | 2) {
            return Err(invalid("mandatory revelation cardinality"));
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
                    "revelation materials changed".into(),
                ));
            }
        }
        match &instruction.action {
            Action::Reveal { .. } => {
                self.internal_reveal_members(execution, instruction, &members, Some(objects))?;
                if !matches!(execution.receipts.get(&instruction.receipt),
                    Some(InstructionOutcome::Revealed(actual)) if *actual == members)
                {
                    return Err(crate::EngineFailure::IllegalPlay(
                        "incomplete mandatory revelation".into(),
                    ));
                }
            }
            Action::Move { movement, .. }
                if reveal_payment::transfer(&instruction.action).is_some() =>
            {
                self.internal_hand_transfer_payment(
                    execution,
                    instruction,
                    &members,
                    movement.position,
                )?;
            }
            Action::Banish { .. } => {
                self.internal_hand_banish_payment(execution, instruction, objects, &members)?;
            }
            _ => return Err(invalid("mandatory revelation cost")),
        }
        Ok((
            Quantity {
                amount: Int(i64::try_from(members.len()).map_err(invalid)?),
                unit: QuantityUnit::Cards,
            },
            members,
            vec![],
        ))
    }
    fn internal_hand_banish_payment(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        objects: &Selector,
        members: &[ObjectRef],
    ) -> Result<()> {
        let [member] = members else {
            return Err(invalid("single-card hand banish cost"));
        };
        self.internal_banish(execution, instruction, objects)?;
        if !matches!(execution.receipts.get(&instruction.receipt),
            Some(InstructionOutcome::Banish(outcome))
                if outcome.successful.len() == 1 && outcome.moves.len() == 1
                    && outcome.moves[0].before == *member
                    && outcome.moves[0].after.is_some()
                    && outcome.moves[0].actual_to.is_some_and(|to|
                        to.kind == ZoneKind::Banish && to.player == execution.context.controller))
        {
            return Err(crate::EngineFailure::IllegalPlay(
                "incomplete hand banish cost".into(),
            ));
        }
        Ok(())
    }

    fn internal_hand_transfer_payment(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        members: &[ObjectRef],
        position: Option<Position>,
    ) -> Result<()> {
        if members.len() != 1 {
            return Err(invalid("single-card hand transfer cost"));
        }
        let (destination, kind) = if reveal_payment::bottom(&instruction.action).is_some() {
            ("deck", ZoneKind::Deck)
        } else {
            ("ex", ZoneKind::Ex)
        };
        if kind == ZoneKind::Ex
            && self.zone_count(seat_name(execution.context.controller)?, "ex") >= 5
        {
            return Err(crate::EngineFailure::IllegalPlay(
                "insufficient EX capacity for cost".into(),
            ));
        }
        let (moved, _) = self.internal_move_members(
            execution,
            members,
            destination,
            kind,
            MoveMethod::Put,
            false,
            position,
            None,
        )?;
        if moved.len() != members.len()
            || !moved.iter().zip(members).all(|(arrival, selected)| {
                arrival.before == *selected
                    && arrival.after.is_some()
                    && arrival.actual_to.is_some_and(|to| {
                        to.kind == kind && to.player == execution.context.controller
                    })
            })
        {
            return Err(crate::EngineFailure::IllegalPlay(
                "incomplete hand transfer cost".into(),
            ));
        }
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Move(MoveOutcome { moved }),
        );
        Ok(())
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "A selected hand cost retains its private material proof through banishment."
)]
impl Execution {
    pub(in crate::game::internal) fn private_hand_banish_materials(
        &self,
        instruction: &Instruction,
    ) -> Option<&[ObjectRef]> {
        let Action::Banish {
            objects: Selector::Bound { reference },
        } = &instruction.action
        else {
            return None;
        };
        if instruction.context != InstructionContext::Cost {
            return None;
        }
        let proof = self.material_selections.get(&reference.key())?;
        if !proof.is_hand() {
            return None;
        }
        let Slot::Objects { members, .. } = self.slots.get(&reference.key())? else {
            return None;
        };
        members
            .iter()
            .all(|member| proof.has_hand_member(*member))
            .then_some(members.as_slice())
    }

    pub(in crate::game::internal) fn is_private_hand_banish_cost(
        &self,
        instruction: &Instruction,
        member: ObjectRef,
    ) -> bool {
        self.private_hand_banish_materials(instruction)
            .is_some_and(|members| members.contains(&member))
    }
}
