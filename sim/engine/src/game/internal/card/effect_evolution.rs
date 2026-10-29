use super::super::Slot;
use super::{Execution, Game, Result, invalid, seat_name, unsupported};
use crate::game::internal::{Fact, InstructionOutcome};
use crate::ir::{
    Action, Control, EvolutionMode, EvolutionState, EvolveEvent, Instruction, Node, ObjectOperand,
    ObjectRef,
};
use core::slice::from_ref;
use serde_json::json;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Effect evolution delegates to the shared evolution rule procedure."
)]
impl Game {
    pub(super) fn effect_evolution_members(&self, execution: &Execution) -> Result<Vec<ObjectRef>> {
        let source = self.object(&execution.context.source)?;
        if source.generation != execution.context.generation.0
            || source.controller != seat_name(execution.context.controller)?
            || !self.can_receive_evolution(&source.id)?
            || self.object_type(&source.id)? != "フォロワー"
        {
            return Ok(vec![]);
        }
        let mut members = Vec::new();
        for id in self.zone_ids(&source.controller, "evolve_deck") {
            if self.evolution_resource_matches(&source.id, &id, 0, &json!({}))?
                && self.catalog.face(&self.object(&id)?.card, 0)?["card_type"]
                    == "フォロワー・エボルヴ"
            {
                members.push(execution.reference(self, &id)?);
            }
        }
        Ok(members)
    }

    pub(in crate::game::internal) fn internal_effect_evolution_iteration(
        &self,
        execution: &mut Execution,
        control: Control,
    ) -> Result<()> {
        let Control::EachObject {
            input,
            binding,
            body,
            order: None,
        } = control
        else {
            return Err(unsupported("effect evolution iteration"));
        };
        let [step] = body.steps.as_slice() else {
            return Err(unsupported("effect evolution instruction count"));
        };
        if !matches!(&step.node, Node::Instruction { instruction } if matches!(&instruction.action,
            Action::Evolve { subject: ObjectOperand::SourceHolder {}, form, mode: EvolutionMode::Normal, .. }
                if form.object == (ObjectOperand::Bound { binding }) && form.face.0 == "base"))
        {
            return Err(unsupported("effect evolution projection"));
        }
        let forms = execution.objects(self, &input)?;
        if forms.len() > 1 {
            return Err(invalid("one evolution form per instruction"));
        }
        if !forms.is_empty() {
            execution.slots.insert(
                binding.key(),
                Slot::Objects {
                    members: forms,
                    on_field_at_capture: false,
                },
            );
            execution.prepend(body);
        }
        Ok(())
    }

    pub(in crate::game::internal) fn internal_effect_evolve(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        let Action::Evolve {
            subject: ObjectOperand::SourceHolder {},
            form,
            mode: EvolutionMode::Normal,
            ..
        } = &instruction.action
        else {
            return Err(unsupported("ordinary effect self evolution"));
        };
        if form.face.0 != "base" || execution.ability.is_none() {
            return Err(unsupported("effect evolution authority or face"));
        }
        let ObjectOperand::Bound { binding } = &form.object else {
            return Err(unsupported("effect evolution selected form"));
        };
        let Some(Slot::Objects { members, .. }) = execution.slots.get(&binding.key()) else {
            return Err(invalid("missing evolution selection binding"));
        };
        let [selected] = members.as_slice() else {
            return Err(invalid("one selected evolution form"));
        };
        let selected = *selected;
        if !self
            .effect_evolution_members(execution)?
            .contains(&selected)
        {
            return Err(invalid(
                "effect evolution form no longer belongs to its domain",
            ));
        }
        let source = execution.context.source.clone();
        let resource = execution.object_id(selected)?.to_owned();
        self.numeric_evolution_allowed(&source, false)?;
        let before = self.internal_characteristics(&source)?;
        let mut frame = execution.frame();
        self.reveal(from_ref(&resource), "all", &frame, false);
        self.apply_evolution(&source, &resource, 0, 0, &mut frame)?;
        let event = EvolveEvent {
            subject: execution.reference(self, &source)?,
            with: execution.reference(self, &resource)?,
            kind: EvolutionState::Evolved,
            before,
            after: self.internal_characteristics(&source)?,
        };
        let associations = self.internal_evolution_link(&source)?;
        let [association] = associations.as_slice() else {
            return Err(invalid("effect evolution association"));
        };
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Evolution {
                event: Box::new(event.clone()),
                association: *association,
            },
        );
        execution.facts.push(Fact::Evolved(Box::new(event)));
        Ok(())
    }
}
