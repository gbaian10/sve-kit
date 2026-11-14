//! A chosen leader-or-follower runs only the body for its kind, with the narrowed reference.

use super::{Execution, Game, Result, Slot, invalid, unsupported};
use crate::ir::{
    Binding, Control, DamageRecipient, Node, ObjectRef, PlayerRef, Program, Requirement,
    RuntimeDamageRecipient,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Kind branches read the same authoritative object state as other controls."
)]
impl Game {
    pub(super) fn internal_by_kind(
        &self,
        execution: &mut Execution,
        recipients: Binding<Vec<DamageRecipient>>,
        leader: (Binding<PlayerRef>, Program),
        follower: (Binding<ObjectRef>, Program),
    ) -> Result<()> {
        let Some(Slot::Recipients {
            members,
            characteristics,
        }) = execution.slots.get(&recipients.key())
        else {
            return Err(invalid("missing recipient choice"));
        };
        match members.as_slice() {
            [RuntimeDamageRecipient::Leader { player }] => {
                let player = *player;
                execution.slots.insert(leader.0.key(), Slot::Player(player));
                execution.prepend(leader.1);
            }
            [RuntimeDamageRecipient::Object { object }] => {
                let object = *object;
                let id = execution.object_id(object)?;
                let current = self.object(id)?;
                // A follower that left the field is a new object; the effect has nothing to reach.
                if current.generation != object.generation.0 || current.zone != "field" {
                    return Ok(());
                }
                let saved = characteristics
                    .iter()
                    .find(|(reference, _)| *reference == object)
                    .ok_or_else(|| invalid("missing recipient target producer"))?;
                if super::card::selection::characteristics(self, id)? != saved.1 {
                    return Err(unsupported(
                        "selected recipient target characteristics changed",
                    ));
                }
                execution.slots.insert(
                    follower.0.key(),
                    Slot::Objects {
                        members: vec![object],
                        on_field_at_capture: true,
                    },
                );
                execution.prepend(follower.1);
            }
            _ => return Err(invalid("by_kind requires exactly one recipient")),
        }
        Ok(())
    }
}

/// Control paths of every by_kind and the branch bodies that may read its narrowed bindings.
#[derive(Default)]
pub(super) struct Paths {
    controls: Vec<String>,
    bodies: Vec<String>,
}

pub(super) fn paths(program: &Program) -> Paths {
    let mut paths = Paths::default();
    collect(program, "", &mut paths);
    paths
}

fn collect(program: &Program, prefix: &str, paths: &mut Paths) {
    for (index, step) in program.steps.iter().enumerate() {
        let path = format!("{prefix}/steps/{index}");
        let Node::Control { control } = &step.node else {
            continue;
        };
        match control {
            Control::Match {
                leader_body,
                follower_body,
                ..
            } => {
                paths.controls.push(format!("{path}/control"));
                for (name, body) in [
                    ("leader_body", leader_body),
                    ("follower_body", follower_body),
                ] {
                    let body_path = format!("{path}/control/{name}");
                    collect(body, &body_path, paths);
                    paths.bodies.push(body_path);
                }
            }
            Control::Sequence { body }
            | Control::May { body, .. }
            | Control::EachObject { body, .. } => {
                collect(body, &format!("{path}/control/body"), paths);
            }
            Control::If {
                then_body,
                else_body,
                ..
            } => {
                collect(then_body, &format!("{path}/control/then_body"), paths);
                if let Some(body) = else_body {
                    collect(body, &format!("{path}/control/else_body"), paths);
                }
            }
            Control::Mode { modes, .. } => {
                for (mode_index, mode) in modes.iter().enumerate() {
                    collect(
                        &mode.body,
                        &format!("{path}/control/modes/{mode_index}/body"),
                        paths,
                    );
                }
            }
            Control::Simultaneous { .. }
            | Control::Repeat { .. }
            | Control::EachPlayer { .. }
            | Control::Pay { .. } => {}
        }
    }
}

/// The control itself, and the narrowed player or follower only inside its own branches.
pub(super) fn requirement(requirement: &Requirement, paths: &Paths) -> bool {
    let (allowed, nested) = match requirement.capability.code {
        "shape.Control.Match" | "control.match" => (&paths.controls, false),
        "shape.PlayerRef.Bound" | "shape.ObjectOperand.Bound" => (&paths.bodies, true),
        _ => return false,
    };
    requirement.locations.iter().all(|location| {
        allowed.iter().any(|prefix| {
            location.ir_path == *prefix
                || (nested
                    && location
                        .ir_path
                        .strip_prefix(prefix.as_str())
                        .is_some_and(|tail| tail.starts_with('/')))
        })
    })
}
