//! PP-backed X reuses the existing typed integer decision and payment expression.
use super::{
    Action, Binding, Cardinality, ChoiceDomain, CostSpec, Decision, DecisionKind, ExecutionRef,
    Expr, InputMode, InstructionContext, Int, IntOperation, Nat, PlaySlot, PlayerRef, Resource,
    ResourceOperation, SelectionSnapshot, Visibility,
};

pub(crate) fn parameter(decision: &Decision) -> Option<Binding<Nat>> {
    let ChoiceDomain::Integer {
        cardinality,
        binding,
    } = &decision.domain
    else {
        return None;
    };
    (*cardinality
        == (Cardinality {
            minimum: Nat(0),
            maximum: None,
        })
        && decision.captures.is_empty()
        && decision.actor == (PlayerRef::Controller {})
        && decision.visibility == (Visibility::Public {})
        && decision.kind
            == (DecisionKind::PlayParameter {
                slot: PlaySlot::X,
                selection_snapshot: SelectionSnapshot::AtDecision,
                play_scope: ExecutionRef::Current {},
                input: InputMode::Inline,
            }))
    .then_some(*binding)
}

pub(crate) fn amount(value: &Expr<Int>) -> Option<Binding<Nat>> {
    let Expr::Compute(operation) = value else {
        return None;
    };
    let IntOperation::FromNat {
        value: Expr::Bound(binding),
    } = operation.as_ref()
    else {
        return None;
    };
    Some(*binding)
}

pub(crate) fn cost_amount(value: &Expr<Int>) -> bool {
    matches!(value, Expr::Literal(Int(n)) if *n >= 0) || amount(value).is_some()
}

pub(crate) fn payment(costs: &CostSpec) -> bool {
    matches!(costs, CostSpec::Ordered { steps } if !steps.is_empty() && steps.iter().all(|step| {
        step.keyword.is_none() && step.materials.is_none()
            && step.instruction.precondition.is_none()
            && step.instruction.context == InstructionContext::Cost
            && step.instruction.actor == (PlayerRef::Controller {})
            && matches!(&step.instruction.action, Action::Resource {
                player: PlayerRef::Controller {}, resource: Resource::PpCurrent,
                operation: ResourceOperation::Pay, amount: value,
            } if amount(value).is_some())
    }))
}
