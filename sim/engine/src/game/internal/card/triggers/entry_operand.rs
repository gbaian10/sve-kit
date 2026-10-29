//! Only direct action subjects may consume this pending occurrence's exact entry object.
use crate::ir::{
    Action, Binding, Bool, Control, EnterFieldEvent, Expr, Node, ObjectFrozen, ObjectOperand,
    Program, Selector,
};
use alloc::collections::BTreeSet;

fn subjects(
    selector: &Selector,
    path: &str,
    event: Binding<EnterFieldEvent>,
    paths: &mut BTreeSet<String>,
) {
    let Selector::Objects { objects } = selector else {
        return;
    };
    for (index, object) in objects.iter().enumerate() {
        if matches!(object, ObjectOperand::FromExpression {value} if matches!(value.as_ref(), Expr::Frozen(field) if matches!(field.as_ref(), ObjectFrozen::EnterFieldObject {event: bound} if *bound == event)))
        {
            paths.insert(format!("{path}/objects/{index}"));
            paths.insert(format!("{path}/objects/{index}/value/frozen"));
        }
    }
}

pub(in crate::game::internal) fn paths(
    program: &Program,
    path: &str,
    event: Binding<EnterFieldEvent>,
    allowed: &mut BTreeSet<String>,
) {
    for (index, step) in program.steps.iter().enumerate() {
        let path = format!("{path}/steps/{index}");
        match &step.node {
            Node::Instruction { instruction } => match &instruction.action {
                Action::Modify {
                    subjects: objects, ..
                } => subjects(
                    objects,
                    &format!("{path}/instruction/action/subjects"),
                    event,
                    allowed,
                ),
                Action::Move { objects, .. } => subjects(
                    objects,
                    &format!("{path}/instruction/action/objects"),
                    event,
                    allowed,
                ),
                _ => {}
            },
            Node::Control {
                control: Control::Sequence { body },
            } => paths(body, &format!("{path}/control/body"), event, allowed),
            Node::Control {
                control:
                    Control::If {
                        then_body,
                        else_body,
                        ..
                    },
            } => {
                paths(
                    then_body,
                    &format!("{path}/control/then_body"),
                    event,
                    allowed,
                );
                if let Some(body) = else_body {
                    paths(body, &format!("{path}/control/else_body"), event, allowed);
                }
            }
            _ => {}
        }
    }
}

pub(in crate::game::internal) fn mask_origin(
    program: &mut Program,
    event: Binding<EnterFieldEvent>,
) {
    for step in &mut program.steps {
        if let Node::Control { control } = &mut step.node {
            match control {
                Control::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    mask_condition(condition, Some(event));
                    mask_origin(then_body, event);
                    if let Some(body) = else_body {
                        mask_origin(body, event);
                    }
                }
                Control::Sequence { body } => mask_origin(body, event),
                Control::Mode { modes, .. } => {
                    for mode in modes {
                        mask_origin(&mut mode.body, event);
                    }
                }
                _ => {}
            }
        }
    }
}

pub(in crate::game::internal) fn mask_condition(
    condition: &mut Expr<Bool>,
    event: Option<Binding<EnterFieldEvent>>,
) {
    use crate::ir::{EntryOrigin, EntryOriginFields, Predicate, ValueEquality, ZoneKind};
    if let Expr::Compute(operation) = condition {
        match operation.as_mut() {
            Predicate::Equal { operands }
                if matches!(operands.as_ref(), ValueEquality::EntryOrigin {
                left: Expr::Frozen(field), right: Expr::Literal(EntryOrigin::Zone { zone: ZoneKind::Hand }),
            } if matches!(field.as_ref(), EntryOriginFields::EnterFieldFrom { event: bound } if event.is_none_or(|event| event == *bound))) =>
            {
                *condition = Expr::Literal(Bool(false));
            }
            Predicate::Not { predicate } => mask_condition(predicate, event),
            Predicate::And { predicates } | Predicate::Or { predicates } => {
                for predicate in predicates {
                    mask_condition(predicate, event);
                }
            }
            _ => {}
        }
    }
}
