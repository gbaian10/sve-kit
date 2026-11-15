//! Declared numbers use the ordinary public integer question and a dedicated public fact.
use super::{Execution, Game, InputDomain, Result, Slot, Wait, invalid, unsupported};
use crate::game::player::QuestionPurpose;
use crate::ir::{Action, Binding, Cardinality, DecisionId, Declaration, Instruction, Int, Nat};

#[derive(Clone, Copy, Debug)]
pub(super) enum NumberBinding {
    Natural(Binding<Nat>),
    Integer(Binding<Int>),
}
impl NumberBinding {
    pub(super) fn assign(self, execution: &mut Execution, number: Nat) -> Result<()> {
        match self {
            Self::Natural(binding) => {
                execution.slots.insert(binding.key(), Slot::Natural(number));
            }
            Self::Integer(binding) => {
                execution.slots.insert(
                    binding.key(),
                    Slot::Integer(Int(i64::try_from(number.0).map_err(invalid)?)),
                );
            }
        }
        Ok(())
    }
}

pub(super) fn check(action: &Action) -> Result<()> {
    if let Action::Declare { actor, declaration } = action {
        if !super::alternatives::participant(actor) {
            return Err(unsupported("number declarer"));
        }
        match declaration {
            Declaration::NonnegativeInteger { .. } => {}
            Declaration::BoundedInteger {
                minimum, maximum, ..
            } if minimum.0 >= 0 && minimum.0 <= maximum.0 => {}
            Declaration::BoundedInteger { .. } | Declaration::CardNames { .. } => {
                return Err(unsupported("nonnegative number declaration"));
            }
        }
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Declarations share the existing public input transaction."
)]
impl Game {
    pub(super) fn internal_declare(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check(&instruction.action)?;
        let Action::Declare { actor, declaration } = &instruction.action else {
            return Err(unsupported("number declaration"));
        };
        let (binding, bounds) = match declaration {
            Declaration::NonnegativeInteger { binding } => (
                NumberBinding::Natural(*binding),
                Cardinality {
                    minimum: Nat(0),
                    maximum: None,
                },
            ),
            Declaration::BoundedInteger {
                minimum,
                maximum,
                binding,
            } => (
                NumberBinding::Integer(*binding),
                Cardinality {
                    minimum: Nat(u64::try_from(minimum.0).map_err(invalid)?),
                    maximum: Some(Nat(u64::try_from(maximum.0).map_err(invalid)?)),
                },
            ),
            Declaration::CardNames { .. } => return Err(unsupported("name declaration")),
        };
        let actor = execution.player(self, actor)?;
        self.internal_wait(
            execution,
            DecisionId(instruction.receipt.0),
            actor,
            InputDomain::Integer(bounds),
            Wait::Integer {
                binding,
                bounds,
                purpose: QuestionPurpose::ResolutionChoice,
                declaration: Some(instruction.receipt),
            },
        )
    }
}
