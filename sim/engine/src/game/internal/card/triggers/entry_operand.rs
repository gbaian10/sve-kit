//! Only direct action subjects may consume this pending occurrence's exact entry object.
use crate::ir::{
    Action, Binding, Control, EnterFieldEvent, Expr, Node, ObjectFrozen, ObjectOperand, Program,
    Selector,
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
