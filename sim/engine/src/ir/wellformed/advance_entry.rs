use super::{BindingKind, Checked, Checker, WellFormedReason};
use crate::ir::{Action, Instruction, advance_entry};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "The closed private entry projection shares lexical scope and identity checks."
)]
impl Checker {
    pub(super) fn check_advance_spell_instruction(&self, instruction: &Instruction) -> Checked {
        if matches!(
            instruction.action,
            Action::PlayCard {
                cost_override: None,
                ..
            }
        ) && self.advance_spell_instruction != Some(instruction.id)
        {
            return self.reject();
        }
        Ok(())
    }

    pub(super) fn advance_entry(&mut self, projection: advance_entry::Projection<'_>) -> Checked {
        if !self.scopes.insert(projection.body.scope) {
            return Err(self.error(WellFormedReason::DuplicateScope));
        }
        self.env.scope = projection.body.scope;
        self.decision_header(projection.decision)?;
        self.isolated(|checker| {
            let scope = projection.candidate.key().0;
            if !checker.scopes.insert(scope) {
                return Err(checker.error(WellFormedReason::DuplicateScope));
            }
            checker.env.scope = scope;
            checker.define(projection.candidate, BindingKind::Value)
        })?;
        self.define(projection.selected, BindingKind::Value)?;
        self.save_access(
            projection.selected.key(),
            Some(projection.decision.visibility.clone()),
        );
        let movement = projection
            .body
            .steps
            .get(1)
            .ok_or_else(|| self.error(WellFormedReason::InvalidEntry))?;
        let previous = self
            .advance_spell_instruction
            .replace(projection.instruction.id);
        let result = self.located(movement, Self::node);
        self.advance_spell_instruction = previous;
        result
    }
}
