//! One optional payment declared before targets and committed before play.
use super::{
    Ability, Bool, BoolRead, Capture, CaptureTime, Cardinality, ChoiceDomain, Comparison, Control,
    CostSpec, ExecutionRef, Expr, HistoryCount, Int, IntOperation, IntRead, Located,
    ModeChoiceKind, ModeDecision, Nat, NatOperation, NatRead, Node, ObjectProperty,
    PaymentOptionality, PaymentTime, PlaySpec, PlayerRef, PlayerResource, Predicate, Program,
    ReceiptId, ScopeId, history_turn, public_numbers,
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
        } => public_mode_guard(condition, then_body) && declared_choice(then_body),
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

pub(crate) fn public_mode_guard(condition: &Expr<Bool>, body: &Program) -> bool {
    if entry_origin_guard(condition) {
        return true;
    }
    public_pp_guard(condition)
        || matches!(condition, Expr::Compute(predicate)
        if matches!(predicate.as_ref(), Predicate::CompareInt {
            left: Expr::Compute(operation), comparison: Comparison::GreaterEqual,
            right: Expr::Literal(Int(minimum)),
        } if *minimum > 0 && matches!(operation.as_ref(), IntOperation::FromNat {
            value: Expr::Live(read),
        } if matches!(read.as_ref(), NatRead::Combo {
            player: PlayerRef::Controller {}, execution: ExecutionRef::Current {},
        }))))
        || (single_mode_body(body) && public_target_guard(condition))
}

/// A conditional declaration may have a single required branch that needs no mode input.
pub(crate) fn required_single_mode(count: &Expr<Nat>, chosen: &ModeDecision) -> bool {
    *count == Expr::Literal(Nat(1))
        && matches!(&chosen.domain, ChoiceDomain::Modes {
            modes,
            cardinality: Cardinality { minimum: Nat(1), maximum: Some(Nat(1)) },
            exclude_used: None,
            ..
        } if modes.len() == 1)
}

fn single_mode_body(body: &Program) -> bool {
    matches!(body.steps.as_slice(), [Located {
        node: Node::Control { control: Control::Mode {
            count, chosen, choice_kind: ModeChoiceKind::ChoiceKeyword, ..
        }},
        ..
    }] if required_single_mode(count, chosen))
}

fn public_target_guard(condition: &Expr<Bool>) -> bool {
    if let Expr::Live(read) = condition {
        return matches!(
            read.as_ref(),
            BoolRead::Crimson {
                player: PlayerRef::Controller {}
            }
        );
    }
    let Expr::Compute(predicate) = condition else {
        return false;
    };
    matches!(predicate.as_ref(), Predicate::Exists { objects } if public_numbers::count(objects))
        || matches!(predicate.as_ref(), Predicate::CompareInt {
            left, comparison: Comparison::GreaterEqual, right: Expr::Literal(Int(0..)),
        } if public_target_threshold(left))
}

fn public_target_threshold(threshold: &Expr<Int>) -> bool {
    if let Expr::Live(read) = threshold {
        return matches!(
            read.as_ref(),
            IntRead::PlayerResource {
                player: PlayerRef::Controller {},
                property: PlayerResource::Life | PlayerResource::PpCurrent,
            }
        );
    }
    let Expr::Compute(operation) = threshold else {
        return false;
    };
    let IntOperation::FromNat { value } = operation.as_ref() else {
        return false;
    };
    match value {
        Expr::Live(read) => {
            matches!(read.as_ref(), NatRead::ZoneCount { selector } if public_numbers::count(selector))
                || matches!(read.as_ref(), NatRead::HistoryCount { query, count: HistoryCount::Facts }
                    if matches!(history_turn::HistoryPlan::compile(query), Some(history_turn::HistoryPlan::Discarded)))
        }
        Expr::Compute(distinct) => matches!(distinct.as_ref(), NatOperation::DistinctCount {
            objects, property: ObjectProperty::Names,
        } if public_numbers::count(objects)),
        Expr::Literal(_) | Expr::Frozen(_) | Expr::Captured(_) | Expr::Bound(_) => false,
    }
}

pub(crate) fn public_pp_guard(condition: &Expr<Bool>) -> bool {
    matches!(condition, Expr::Compute(predicate)
    if matches!(predicate.as_ref(), Predicate::CompareInt {
        left: Expr::Live(read), comparison: Comparison::GreaterEqual,
        right: Expr::Literal(Int(minimum)),
    } if *minimum >= 0 && matches!(read.as_ref(), IntRead::PlayerResource {
        player: PlayerRef::Controller {}, property: PlayerResource::PpMaximum,
    })))
}

fn entry_origin_guard(condition: &Expr<Bool>) -> bool {
    use crate::ir::{EntryOrigin, EntryOriginFields, ValueEquality, ZoneKind};
    let Expr::Compute(outer) = condition else {
        return false;
    };
    let candidate = if let Predicate::Not { predicate } = outer.as_ref() {
        predicate
    } else {
        condition
    };
    let Expr::Compute(inner) = candidate else {
        return false;
    };
    matches!(inner.as_ref(), Predicate::Equal { operands } if matches!(operands.as_ref(), ValueEquality::EntryOrigin {
        left: Expr::Frozen(field), right: Expr::Literal(EntryOrigin::Zone { zone: ZoneKind::Hand | ZoneKind::Deck | ZoneKind::Cemetery }),
    } if matches!(field.as_ref(), EntryOriginFields::EnterFieldFrom { .. })))
}

/// A selected mode owns one optional play-time cost and its guarded continuation.
pub(crate) const fn mode_declaration(body: &Program) -> Option<(&CostSpec, ReceiptId, &Program)> {
    let [step] = body.steps.as_slice() else {
        return None;
    };
    let Node::Control {
        control:
            Control::Pay {
                at: PaymentTime::PlayParameter,
                costs,
                on_paid,
                receipt,
                optional: PaymentOptionality::Optional,
            },
    } = &step.node
    else {
        return None;
    };
    Some((costs, *receipt, on_paid))
}
