//! Mixed arrivals use the ordinary capacity question and one physical movement commit.
use super::super::{
    Execution, Game, InstructionOutcome, Result, Slot, invalid, seat_name, unsupported,
};
use super::{ArrivalMember, FieldArrival, MovementPlan};
use crate::ir::mixed_ex;
use crate::ir::{
    Action, BatchOutcome, Control, Located, MoveMethod, MoveOutcome, Node, Program, ResultMember,
    ResultMemberId, RuntimeObjectView, RuntimeReceiptId, RuntimeZoneRef, ScopeId, SnapshotId,
    TokenOutcome, ZoneKind,
};

pub(in crate::game::internal) fn mask(program: &mut Program) -> Result<()> {
    for step in &mut program.steps {
        match &mut step.node {
            Node::Control { control } if matches!(control, Control::Simultaneous { .. }) => {
                *control = Control::Sequence {
                    body: inspection(control, program.scope)?,
                };
            }
            Node::Control {
                control:
                    Control::Sequence { body }
                    | Control::May { body, .. }
                    | Control::EachObject { body, .. },
            } => mask(body)?,
            Node::Control {
                control:
                    Control::If {
                        then_body,
                        else_body,
                        ..
                    },
            } => {
                mask(then_body)?;
                if let Some(body) = else_body {
                    mask(body)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub(in crate::game::internal) fn inspection(control: &Control, scope: ScopeId) -> Result<Program> {
    if mixed_ex::projection(control).is_none() {
        return Err(unsupported("mixed EX batch"));
    }
    let Control::Simultaneous { batch, .. } = control else {
        return Err(invalid("batch"));
    };
    Ok(Program {
        scope,
        steps: batch
            .members
            .iter()
            .map(|member| Located {
                source: member.source.clone(),
                node: Node::Instruction {
                    instruction: Box::new(member.node.clone()),
                },
            })
            .collect(),
    })
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "The batch reuses the shared planner, capacity selection and atomic commit."
)]
impl Game {
    pub(in crate::game::internal) fn internal_mixed_ex(
        &mut self,
        execution: &mut Execution,
        control: &Control,
    ) -> Result<()> {
        let shape = mixed_ex::projection(control).ok_or_else(|| unsupported("mixed EX batch"))?;
        super::super::tokens::check_template(self, shape.template, ZoneKind::Ex)?;
        let Action::Move { objects, .. } = &shape.movement.action else {
            return Err(invalid("batch move"));
        };
        let members = execution.objects(self, objects)?;
        self.check_move_members(
            execution,
            shape.movement,
            &members,
            ZoneKind::Ex,
            MoveMethod::Put,
            false,
        )?;
        let plan = self.plan_movements(
            execution,
            &members,
            "ex",
            ZoneKind::Ex,
            MoveMethod::Put,
            true,
            None,
        )?;
        let card = self.catalog.token_print(shape.template)?;
        let token = ArrivalMember::Token(card.clone());
        let mut arrivals = plan
            .plans
            .iter()
            .map(|movement| {
                let outcome = plan
                    .moves
                    .get(movement.result_index)
                    .ok_or_else(|| invalid("batch movement result"))?;
                Ok((ArrivalMember::Object(outcome.before), outcome.requested_to))
            })
            .collect::<Result<Vec<_>>>()?;
        arrivals.push((
            token.clone(),
            RuntimeZoneRef {
                player: execution.context.controller,
                kind: ZoneKind::Ex,
            },
        ));
        let Some(chosen) = self.internal_arrival_capacity(
            execution,
            shape.movement,
            Node::Control {
                control: control.clone(),
            },
            &arrivals,
        )?
        else {
            return Ok(());
        };
        let MovementPlan { moves, mut plans } = plan;
        plans.retain(|movement| {
            moves
                .get(movement.result_index)
                .is_some_and(|outcome| chosen.contains(&ArrivalMember::Object(outcome.before)))
        });
        let ids = if chosen.contains(&token) {
            vec![self.new_token_object(&card, 0, seat_name(execution.context.controller)?)?]
        } else {
            vec![]
        };
        execution.ids = self.internal_identities()?;
        let before_snapshot = SnapshotId(self.state.ir_epoch);
        let (moved, _) = self.commit_movement_plan(
            execution,
            MovementPlan { moves, plans },
            &ids,
            "ex",
            ZoneKind::Ex,
            MoveMethod::Put,
            true,
            None,
            FieldArrival {
                posture: None,
                without_fanfare: false,
            },
            false,
        )?;
        let created = ids
            .iter()
            .map(|id| execution.reference(self, id))
            .collect::<Result<Vec<_>>>()?;
        publish(
            execution,
            &shape,
            MoveOutcome { moved },
            TokenOutcome { created },
            before_snapshot,
            SnapshotId(self.state.ir_epoch),
        )?;
        super::capacity::finish(execution, shape.movement);
        Ok(())
    }
}

fn publish(
    execution: &mut Execution,
    shape: &mixed_ex::Projection<'_>,
    movement: MoveOutcome,
    creation: TokenOutcome,
    before_snapshot: SnapshotId,
    after_snapshot: SnapshotId,
) -> Result<()> {
    let successful = movement
        .moved
        .iter()
        .filter_map(|movement| {
            movement.after.map(|after| {
                (
                    Some(RuntimeObjectView {
                        object: movement.before,
                        snapshot: before_snapshot,
                    }),
                    RuntimeObjectView {
                        object: after,
                        snapshot: after_snapshot,
                    },
                )
            })
        })
        .chain(creation.created.iter().map(|&object| {
            (
                None,
                RuntimeObjectView {
                    object,
                    snapshot: after_snapshot,
                },
            )
        }))
        .enumerate()
        .map(|(index, (before, after))| {
            Ok(ResultMember {
                id: ResultMemberId(u64::try_from(index).map_err(invalid)?),
                before,
                after: Some(after),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let Action::Token { output, .. } = &shape.creation.action else {
        return Err(invalid("batch token output"));
    };
    execution
        .slots
        .insert(output.key(), Slot::Created(creation.created.clone()));
    execution
        .receipts
        .insert(shape.movement.receipt, InstructionOutcome::Move(movement));
    execution
        .receipts
        .insert(shape.creation.receipt, InstructionOutcome::Token(creation));
    execution.receipts.insert(
        shape.receipt,
        InstructionOutcome::Batch(BatchOutcome {
            receipts: [shape.movement.receipt, shape.creation.receipt]
                .into_iter()
                .map(|receipt| RuntimeReceiptId {
                    execution: execution.id,
                    slot: receipt,
                })
                .collect(),
            successful,
        }),
    );
    Ok(())
}
