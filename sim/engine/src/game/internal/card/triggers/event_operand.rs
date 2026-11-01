//! Direct action subjects may consume only this pending occurrence's exact subject.
use crate::ir::events::attack::target_plan;
use crate::ir::{
    Action, Binding, Bool, Control, EnterFieldEvent, Expr, InstallationSubjects, Node,
    ObjectOperand, ObjectRef, Program, Selector,
};
use crate::ir::{Int, IntOperation, Predicate};
use alloc::collections::BTreeSet;

fn subjects(
    selector: &Selector,
    path: &str,
    event: &Expr<ObjectRef>,
    paths: &mut BTreeSet<String>,
) {
    let Selector::Objects { objects } = selector else {
        return;
    };
    for (index, object) in objects.iter().enumerate() {
        if matches!(object, ObjectOperand::FromExpression { value } if value.as_ref() == event) {
            paths.insert(format!("{path}/objects/{index}"));
            paths.insert(format!("{path}/objects/{index}/value/frozen"));
        }
    }
}

pub(in crate::game::internal) fn paths(
    program: &Program,
    path: &str,
    event: &Expr<ObjectRef>,
    allowed: &mut BTreeSet<String>,
) {
    for (index, step) in program.steps.iter().enumerate() {
        let path = format!("{path}/steps/{index}");
        match &step.node {
            Node::Instruction { instruction } => match &instruction.action {
                Action::Modify {
                    subjects: objects, ..
                }
                | Action::Grant {
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
                Action::InstallEffect {
                    subjects: InstallationSubjects::Objects { objects, .. },
                    ..
                } => subjects(
                    objects,
                    &format!("{path}/instruction/action/subjects/objects"),
                    event,
                    allowed,
                ),
                _ => {}
            },
            Node::Control {
                control: Control::Sequence { body } | Control::May { body, .. },
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
            Node::Control {
                control: Control::Mode { modes, .. },
            } => {
                for (mode_index, mode) in modes.iter().enumerate() {
                    paths(
                        &mode.body,
                        &format!("{path}/control/modes/{mode_index}/body"),
                        event,
                        allowed,
                    );
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
                Control::Sequence { body } | Control::May { body, .. } => mask_origin(body, event),
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
                left: Expr::Frozen(field), right: Expr::Literal(EntryOrigin::Zone { zone: ZoneKind::Hand | ZoneKind::Deck | ZoneKind::Cemetery }),
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

fn target_selector_path(selector: &Selector, path: &str, allowed: &mut BTreeSet<String>) {
    if target_plan(selector).is_some() {
        allowed.insert(path.into());
    }
}

fn target_number(value: &Expr<Int>, path: &str, allowed: &mut BTreeSet<String>) {
    let Expr::Compute(operation) = value else {
        return;
    };
    if let IntOperation::Sum { objects, .. } = operation.as_ref() {
        target_selector_path(objects, &format!("{path}/compute/objects"), allowed);
    }
}

fn target_condition(value: &Expr<Bool>, path: &str, allowed: &mut BTreeSet<String>) {
    let Expr::Compute(operation) = value else {
        return;
    };
    match operation.as_ref() {
        Predicate::Exists { objects } => {
            target_selector_path(objects, &format!("{path}/compute/objects"), allowed);
        }
        Predicate::CompareInt { left, right, .. } => {
            target_number(left, &format!("{path}/compute/left"), allowed);
            target_number(right, &format!("{path}/compute/right"), allowed);
        }
        Predicate::And { predicates } | Predicate::Or { predicates } => {
            for (index, predicate) in predicates.iter().enumerate() {
                target_condition(
                    predicate,
                    &format!("{path}/compute/predicates/{index}"),
                    allowed,
                );
            }
        }
        Predicate::Not { predicate } => {
            target_condition(predicate, &format!("{path}/compute/predicate"), allowed);
        }
        _ => {}
    }
}

pub(in crate::game::internal) fn target_paths(
    program: &Program,
    path: &str,
    allowed: &mut BTreeSet<String>,
) {
    for (index, step) in program.steps.iter().enumerate() {
        let path = format!("{path}/steps/{index}");
        match &step.node {
            Node::Instruction { instruction } => match &instruction.action {
                Action::Modify { subjects, .. } | Action::Grant { subjects, .. } => {
                    target_selector_path(
                        subjects,
                        &format!("{path}/instruction/action/subjects"),
                        allowed,
                    );
                }
                Action::Move { objects, .. } | Action::Destroy { objects } => target_selector_path(
                    objects,
                    &format!("{path}/instruction/action/objects"),
                    allowed,
                ),
                _ => {}
            },
            Node::Control { control } => match control {
                Control::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    target_condition(condition, &format!("{path}/control/condition"), allowed);
                    target_paths(then_body, &format!("{path}/control/then_body"), allowed);
                    if let Some(body) = else_body {
                        target_paths(body, &format!("{path}/control/else_body"), allowed);
                    }
                }
                Control::Sequence { body } | Control::May { body, .. } => {
                    target_paths(body, &format!("{path}/control/body"), allowed);
                }
                _ => {}
            },
            _ => {}
        }
    }
}
