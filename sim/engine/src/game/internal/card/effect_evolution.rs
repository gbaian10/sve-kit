use super::super::Slot;
use super::{Execution, Game, Result, invalid, seat_name, unsupported};
use crate::game::internal::{Fact, InstructionOutcome};
use crate::ir::effect_evolution;
use crate::ir::{
    Action, Control, EvolutionMode, EvolutionState, EvolveEvent, Instruction, Node, ObjectOperand,
    ObjectRef, Selector,
};
use core::slice::from_ref;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Effect evolution delegates to the shared evolution rule procedure."
)]
impl Game {
    pub(super) fn effect_evolution_members(
        &self,
        execution: &Execution,
        specification: &effect_evolution::Specification,
    ) -> Result<Vec<ObjectRef>> {
        let Some(reference) = self.effect_evolution_subject(execution, &specification.subject)?
        else {
            return Ok(vec![]);
        };
        let source = self.object(execution.object_id(reference)?)?;
        let mut members = Vec::new();
        let specification = super::evolution::destination_spec(&specification.destination);
        for id in self.zone_ids(&source.controller, "evolve_deck") {
            if self.evolution_resource_matches(&source.id, &id, 0, &specification)?
                && self.catalog.face(&self.object(&id)?.card, 0)?["card_type"]
                    == "フォロワー・エボルヴ"
            {
                members.push(execution.reference(self, &id)?);
            }
        }
        Ok(members)
    }

    fn effect_evolution_subject(
        &self,
        execution: &Execution,
        subject: &ObjectOperand,
    ) -> Result<Option<ObjectRef>> {
        let reference = if let ObjectOperand::Selected { selection } = subject {
            let objects = execution.objects(
                self,
                &Selector::Bound {
                    reference: *selection,
                },
            )?;
            let [reference] = objects.as_slice() else {
                if objects.is_empty() {
                    return Ok(None);
                }
                return Err(invalid("one effect evolution subject"));
            };
            *reference
        } else {
            execution.object(self, subject)?
        };
        let source = self.object(execution.object_id(reference)?)?;
        Ok((source.generation == reference.generation.0
            && source.zone == "field"
            && source.controller == seat_name(execution.context.controller)?
            && self.can_receive_evolution(&source.id)?
            && self.object_type(&source.id)? == "フォロワー")
            .then_some(reference))
    }

    fn effect_evolution_entry(&self, execution: &mut Execution, control: &Control) -> Result<bool> {
        let Some(projection) = effect_evolution::projection(control) else {
            return Ok(false);
        };
        let Some(entry) = projection.entry else {
            return Ok(false);
        };
        let members = execution.objects(self, entry.input)?;
        if members.len() > 1 {
            return Err(invalid("one actual field entry per effect evolution"));
        }
        if !members.is_empty() {
            execution.slots.insert(
                entry.binding.key(),
                Slot::Objects {
                    members,
                    on_field_at_capture: true,
                },
            );
            execution.prepend(entry.body.clone());
        }
        Ok(true)
    }

    pub(in crate::game::internal) fn internal_effect_evolution_iteration(
        &self,
        execution: &mut Execution,
        control: Control,
    ) -> Result<()> {
        if self.effect_evolution_entry(execution, &control)? {
            return Ok(());
        }
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
            Action::Evolve { form, mode: EvolutionMode::Normal, .. }
                if form.object == (ObjectOperand::Bound { binding }) && form.face.0 == "base"))
        {
            return Err(unsupported("effect evolution projection"));
        }
        let Selector::Bound { reference } = &input else {
            return Err(unsupported("effect evolution choice binding"));
        };
        let specification = execution
            .targets
            .get(&reference.key())
            .and_then(|wait| effect_evolution::specification(&wait.decision))
            .ok_or_else(|| invalid("missing effect evolution choice authority"))?;
        let forms = execution.objects(self, &input)?;
        let permitted = self.effect_evolution_members(execution, &specification)?;
        if forms.iter().any(|form| !permitted.contains(form)) {
            return Err(invalid("effect evolution form left its declared domain"));
        }
        if forms.len() > 1 {
            return Err(invalid("one evolution form per instruction"));
        }
        if !matches!(&step.node, Node::Instruction { instruction } if matches!(&instruction.action,
            Action::Evolve { subject, .. } if *subject == specification.subject))
        {
            return Err(invalid("foreign effect evolution subject"));
        }
        if !forms.is_empty() {
            let subject = self
                .effect_evolution_subject(execution, &specification.subject)?
                .ok_or_else(|| invalid("effect evolution subject left its domain"))?;
            // Both the subject and form remain tied to their original authority producers.
            if execution
                .effect_evolution_destinations
                .contains_key(&binding.key())
            {
                return Err(invalid("unconsumed effect evolution authority"));
            }
            execution
                .effect_evolution_destinations
                .insert(binding.key(), (specification, subject));
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
            subject,
            form,
            mode: EvolutionMode::Normal,
            ..
        } = &instruction.action
        else {
            return Err(unsupported("ordinary effect evolution"));
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
        let (specification, original) = execution
            .effect_evolution_destinations
            .remove(&binding.key())
            .ok_or_else(|| invalid("missing effect evolution destination authority"))?;
        if *subject != specification.subject
            || self.effect_evolution_subject(execution, subject)? != Some(original)
            || !self
                .effect_evolution_members(execution, &specification)?
                .contains(&selected)
        {
            return Err(invalid(
                "effect evolution form no longer belongs to its domain",
            ));
        }
        let source = execution.object_id(original)?.to_owned();
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
