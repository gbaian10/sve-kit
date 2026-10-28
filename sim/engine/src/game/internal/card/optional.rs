//! Optional declaration is separate from its all-or-nothing payment.
use super::play::PlayState;
use super::{AbilitySource, Execution, Game, Result, invalid, unsupported};
use crate::game::internal::{InputDomain, InstructionOutcome, Wait};
use crate::ir::{
    Action, Bool, Control, CostSpec, DecisionId, Expr, InstructionContext, Int, Node,
    PaymentOptionality, PaymentResult, PaymentTime, PlayerRef, Program, ReceiptId, Resource,
    ResourceOperation, RuntimeReceiptId, optional_payment,
};

#[derive(Clone, Debug)]
pub(super) struct OptionalState {
    costs: CostSpec,
    receipt: ReceiptId,
    accepted: Option<bool>,
}

pub(super) fn check_costs(costs: &CostSpec, origin: &AbilitySource) -> Result<()> {
    let CostSpec::Ordered { steps } = costs else {
        return Err(unsupported("optional cost alternatives"));
    };
    let [step] = steps.as_slice() else {
        return Err(unsupported("one optional payment step"));
    };
    let instruction = &step.instruction;
    if step.materials.is_some()
        || instruction.precondition.is_some()
        || instruction.source != *origin
        || instruction.actor != (PlayerRef::Controller {})
        || instruction.context != InstructionContext::Cost
        || !matches!(&instruction.action, Action::Resource { player: PlayerRef::Controller {},
            resource: Resource::PpCurrent, operation: ResourceOperation::Pay,
            amount: Expr::Literal(Int(amount)) } if *amount >= 0)
    {
        return Err(unsupported("optional payment source or cost"));
    }
    Ok(())
}

pub(super) fn check_body(program: &Program, root: bool) -> Result<()> {
    for (index, step) in program.steps.iter().enumerate() {
        match &step.node {
            Node::Control {
                control: control @ Control::Pay { .. },
            } => {
                if !root || index != 0 {
                    return Err(unsupported("one root optional declaration"));
                }
                check_control(control)?;
            }
            Node::Control {
                control: Control::Sequence { body },
            } => check_body(body, false)?,
            Node::Control {
                control:
                    Control::If {
                        then_body,
                        else_body,
                        ..
                    },
            } => {
                check_body(then_body, false)?;
                if let Some(body) = else_body {
                    check_body(body, false)?;
                }
            }
            Node::Control {
                control: Control::Mode { modes, .. },
            } => {
                for mode in modes {
                    check_body(&mode.body, false)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn take(body: &mut Program) -> Option<OptionalState> {
    let (costs, receipt) = optional_payment::declaration(body)?;
    let state = OptionalState {
        costs: costs.clone(),
        receipt,
        accepted: None,
    };
    body.steps.remove(0);
    Some(state)
}

pub(in crate::game::internal) fn accept(execution: &mut Execution, accepted: bool) -> Result<bool> {
    let Some(payment) = execution
        .play
        .as_mut()
        .and_then(|play| play.optional.as_mut())
    else {
        return Ok(false);
    };
    if payment.accepted.replace(accepted).is_some() {
        return Err(invalid("repeated optional declaration"));
    }
    Ok(true)
}

pub(in crate::game::internal) fn check_control(control: &Control) -> Result<()> {
    if !matches!(control, Control::Pay { at: PaymentTime::PlayParameter,
        optional: PaymentOptionality::Optional, on_paid, .. } if on_paid.steps.is_empty())
    {
        return Err(unsupported("optional payment timing or continuation"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Declaration and payment share the play transaction."
)]
impl Game {
    pub(super) fn internal_optional_decision(
        &mut self,
        execution: &mut Execution,
        play: &PlayState,
    ) -> Result<bool> {
        let Some(payment) = &play.optional else {
            return Ok(false);
        };
        if payment.accepted.is_some() {
            return Ok(false);
        }
        self.internal_wait(
            execution,
            DecisionId(payment.receipt.0),
            execution.context.controller,
            InputDomain::OptionalCost,
            Wait::OptionalCost,
        )?;
        Ok(true)
    }

    pub(super) fn internal_optional_payment(
        &mut self,
        execution: &mut Execution,
        play: &PlayState,
    ) -> Result<()> {
        let Some(payment) = &play.optional else {
            return Ok(());
        };
        let complete = payment
            .accepted
            .ok_or_else(|| invalid("missing optional declaration"))?;
        let mut receipts = vec![];
        if complete {
            self.internal_card_payment(execution, &payment.costs, false)?;
            let CostSpec::Ordered { steps } = &payment.costs else {
                return Err(unsupported("optional costs"));
            };
            receipts.extend(steps.iter().map(|step| RuntimeReceiptId {
                execution: execution.id,
                slot: step.instruction.receipt,
            }));
        }
        execution.receipts.insert(
            payment.receipt,
            InstructionOutcome::ExchangePayment(PaymentResult {
                complete: Bool(complete),
                steps: receipts,
            }),
        );
        Ok(())
    }
}
