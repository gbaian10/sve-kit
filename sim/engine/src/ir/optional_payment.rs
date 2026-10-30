//! One optional payment declared before targets and committed before play.
use super::{
    Ability, Bool, Capture, CaptureTime, Comparison, Control, CostSpec, Expr, Int, IntRead, Node,
    PaymentOptionality, PaymentTime, PlaySpec, PlayerRef, PlayerResource, Predicate, Program,
    ReceiptId, ScopeId,
};

pub(crate) fn declaration(body: &Program) -> Option<(&CostSpec, ReceiptId)> {
    let Node::Control {
        control:
            Control::Pay {
                at: PaymentTime::PlayParameter,
                costs,
                on_paid,
                receipt,
                ..
            },
    } = &body.steps.first()?.node
    else {
        return None;
    };
    on_paid.steps.is_empty().then_some((costs, *receipt))
}

pub(crate) fn required(body: &Program) -> bool {
    matches!(
        body.steps.first().map(|step| &step.node),
        Some(Node::Control {
            control: Control::Pay {
                optional: PaymentOptionality::Required,
                ..
            }
        })
    )
}

pub(crate) fn trigger_play(ability: &Ability) -> bool {
    let Some(play) = &ability.play else {
        return true;
    };
    let PlaySpec {
        limit: None,
        captures,
        parameters,
        costs: CostSpec::Ordered { steps },
    } = play
    else {
        return false;
    };
    let Some(body) = &ability.body else {
        return false;
    };
    captures.is_empty()
        && steps.is_empty()
        && (!required(body) || declaration(body).is_some())
        && (declaration(body).is_some() || !parameters.is_empty() || declared_choice(body))
}

fn declared_choice(body: &Program) -> bool {
    // Read-only resolution captures precede the sole declared choice, never extra effects.
    let mut steps = body
        .steps
        .iter()
        .skip_while(|step| resolution_capture(&step.node, body.scope));
    let Some(step) = steps.next() else {
        return false;
    };
    if steps.next().is_some() {
        return false;
    }
    match &step.node {
        Node::Control {
            control: Control::Mode { .. },
        } => true,
        Node::Control {
            control:
                Control::If {
                    condition,
                    then_body,
                    else_body: None,
                },
        } => public_mode_guard(condition) && declared_choice(then_body),
        Node::Instruction { .. }
        | Node::Control { .. }
        | Node::Capture { .. }
        | Node::Decision { .. } => false,
    }
}

fn resolution_capture(node: &Node, scope: ScopeId) -> bool {
    let Node::Capture { capture } = node else {
        return false;
    };
    let (Capture::Integer { at, .. }
    | Capture::Natural { at, .. }
    | Capture::Boolean { at, .. }
    | Capture::Objects { at, .. }
    | Capture::Player { at, .. }) = capture
    else {
        return false;
    };
    *at == CaptureTime::AtResolution { scope }
}

pub(crate) fn public_mode_guard(condition: &Expr<Bool>) -> bool {
    matches!(condition, Expr::Compute(predicate)
    if matches!(predicate.as_ref(), Predicate::CompareInt {
        left: Expr::Live(read), comparison: Comparison::GreaterEqual,
        right: Expr::Literal(Int(minimum)),
    } if *minimum >= 0 && matches!(read.as_ref(), IntRead::PlayerResource {
        player: PlayerRef::Controller {}, property: PlayerResource::PpMaximum,
    })))
}
