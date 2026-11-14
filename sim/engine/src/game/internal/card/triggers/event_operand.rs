//! Direct action subjects may consume only this pending occurrence's exact subject.
use crate::ir::events::attack::target_plan;
use crate::ir::walk::{self, NodeRef, Traversal, Visitor};
use crate::ir::{
    Action, Binding, Bool, Control, EnterFieldEvent, Expr, InstallationSubjects, LkiReason,
    NameOperation, Node, ObjectOperand, ObjectRef, ObjectView, Program, RecipientSet, Selector,
    TokenTemplate,
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

struct DirectPaths<'paths> {
    event: &'paths Expr<ObjectRef>,
    allowed: &'paths mut BTreeSet<String>,
}

impl Visitor for DirectPaths<'_> {
    fn enter(&mut self, node: NodeRef<'_>, path: &str) -> Traversal {
        match node {
            NodeRef::Control(Control::If { condition, .. }) => {
                condition_subjects(
                    condition,
                    &format!("{path}/condition"),
                    self.event,
                    self.allowed,
                );
                Traversal::Descend
            }
            NodeRef::Control(
                Control::Sequence { .. }
                | Control::May { .. }
                | Control::Mode { .. }
                | Control::Pay { .. },
            ) => Traversal::Descend,
            NodeRef::Control(_) | NodeRef::CostSpec(_) => Traversal::Skip,
            NodeRef::Instruction(instruction) => {
                direct_action_paths(&instruction.action, path, self.event, self.allowed);
                // This policy grants only direct subjects, never nested definitions or delayed bodies.
                Traversal::Skip
            }
            _ => Traversal::Descend,
        }
    }
}

/// 「場に出たフォロワーが…なら」: a branch condition may filter the exact subject.
fn condition_subjects(
    condition: &Expr<Bool>,
    path: &str,
    event: &Expr<ObjectRef>,
    allowed: &mut BTreeSet<String>,
) {
    let Expr::Compute(operation) = condition else {
        return;
    };
    match operation.as_ref() {
        Predicate::Exists {
            objects: Selector::Filter { input, .. },
        } => subjects(
            input,
            &format!("{path}/compute/objects/input"),
            event,
            allowed,
        ),
        Predicate::Not { predicate } => {
            condition_subjects(
                predicate,
                &format!("{path}/compute/predicate"),
                event,
                allowed,
            );
        }
        Predicate::And { predicates } | Predicate::Or { predicates } => {
            for (index, branch) in predicates.iter().enumerate() {
                condition_subjects(
                    branch,
                    &format!("{path}/compute/predicates/{index}"),
                    event,
                    allowed,
                );
            }
        }
        _ => {}
    }
}

pub(in crate::game::internal) fn paths(
    program: &Program,
    path: &str,
    event: &Expr<ObjectRef>,
    allowed: &mut BTreeSet<String>,
) {
    walk::program(program, path, &mut DirectPaths { event, allowed });
}

fn direct_action_paths(
    action: &Action,
    path: &str,
    event: &Expr<ObjectRef>,
    allowed: &mut BTreeSet<String>,
) {
    match action {
        Action::Damage { recipients, .. } => recipient_subjects(
            recipients,
            &format!("{path}/action/recipients"),
            event,
            allowed,
        ),
        Action::Modify {
            subjects: objects, ..
        }
        | Action::Grant {
            subjects: objects, ..
        } => subjects(objects, &format!("{path}/action/subjects"), event, allowed),
        Action::Token { tokens, .. } => {
            for (token_index, token) in tokens.iter().enumerate() {
                if let TokenTemplate::Name {
                    name: Expr::Compute(operation),
                    ..
                } = &token.template
                    && let NameOperation::UniqueName {
                        object:
                            ObjectView::LastKnown {
                                object,
                                reason: LkiReason::ZoneExit,
                            },
                    } = operation.as_ref()
                    && matches!(object.as_ref(), ObjectOperand::FromExpression { value } if value.as_ref() == event)
                {
                    let prefix = format!(
                        "{path}/action/tokens/{token_index}/template/name/compute/object/object"
                    );
                    allowed.insert(prefix.clone());
                    allowed.insert(format!("{prefix}/value/frozen"));
                }
            }
        }
        Action::Move { objects, .. } => {
            subjects(objects, &format!("{path}/action/objects"), event, allowed);
        }
        Action::InstallEffect {
            subjects: InstallationSubjects::Objects { objects, .. },
            ..
        } => subjects(
            objects,
            &format!("{path}/action/subjects/objects"),
            event,
            allowed,
        ),
        _ => {}
    }
}

fn recipient_subjects(
    recipients: &RecipientSet,
    path: &str,
    event: &Expr<ObjectRef>,
    allowed: &mut BTreeSet<String>,
) {
    match recipients {
        RecipientSet::Objects { objects } => {
            subjects(objects, &format!("{path}/objects"), event, allowed);
        }
        RecipientSet::Union { sets } => {
            for (index, set) in sets.iter().enumerate() {
                recipient_subjects(set, &format!("{path}/sets/{index}"), event, allowed);
            }
        }
        _ => {}
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
                Control::Sequence { body }
                | Control::May { body, .. }
                | Control::Pay { on_paid: body, .. } => mask_origin(body, event),
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

struct TargetPaths<'paths> {
    allowed: &'paths mut BTreeSet<String>,
}

impl Visitor for TargetPaths<'_> {
    fn enter(&mut self, node: NodeRef<'_>, path: &str) -> Traversal {
        match node {
            NodeRef::Control(Control::If { condition, .. }) => {
                target_condition(condition, &format!("{path}/condition"), self.allowed);
                Traversal::Descend
            }
            NodeRef::Control(Control::Sequence { .. } | Control::May { .. }) => Traversal::Descend,
            NodeRef::Control(_) | NodeRef::CostSpec(_) => Traversal::Skip,
            NodeRef::Instruction(instruction) => {
                target_action_paths(&instruction.action, path, self.allowed);
                Traversal::Skip
            }
            _ => Traversal::Descend,
        }
    }
}

pub(in crate::game::internal) fn target_paths(
    program: &Program,
    path: &str,
    allowed: &mut BTreeSet<String>,
) {
    walk::program(program, path, &mut TargetPaths { allowed });
}

fn target_action_paths(action: &Action, path: &str, allowed: &mut BTreeSet<String>) {
    match action {
        Action::Damage { recipients, .. } => {
            target_recipient_paths(recipients, &format!("{path}/action/recipients"), allowed);
        }
        Action::Modify { subjects, .. } | Action::Grant { subjects, .. } => {
            target_selector_path(subjects, &format!("{path}/action/subjects"), allowed);
        }
        Action::Move { objects, .. } | Action::Destroy { objects } => {
            target_selector_path(objects, &format!("{path}/action/objects"), allowed);
        }
        _ => {}
    }
}

fn target_recipient_paths(recipients: &RecipientSet, path: &str, allowed: &mut BTreeSet<String>) {
    match recipients {
        RecipientSet::Objects { objects } => {
            target_selector_path(objects, &format!("{path}/objects"), allowed);
        }
        RecipientSet::Union { sets } => {
            for (index, set) in sets.iter().enumerate() {
                target_recipient_paths(set, &format!("{path}/sets/{index}"), allowed);
            }
        }
        _ => {}
    }
}
