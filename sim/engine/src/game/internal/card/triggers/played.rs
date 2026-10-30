//! Play admission captures cemetery observers before the Spell body can move them.
use super::{
    Ability, AbilityKind, Cause, EventPayload, EventSnapshotPolicy, Game, Result, SnapshotId,
    TriggerAggregation, ZoneKind, invalid, player_id, unsupported,
};
use crate::game::internal::Execution;
use crate::ir::events::played::filter;
use crate::ir::optional_payment;
use crate::ir::{
    Action, Control, CostSpec, Located, MoveDestination, MoveReason, Node, ObjectOperand,
    PlayEvent, PlaySubject, PlayerRef, Program, Requirements, RuntimeReceiptId, RuntimeZoneRef,
    Selector,
};

use alloc::collections::BTreeSet;

pub(super) fn check_header(ability: &Ability) -> Result<bool> {
    if filter(ability).is_none() {
        return Ok(false);
    }
    let trigger = ability
        .trigger
        .as_ref()
        .ok_or_else(|| invalid("missing Play trigger"))?;
    if ability.kind != AbilityKind::Triggered
        || !optional_payment::trigger_play(ability)
        || trigger.view != EventSnapshotPolicy::AtReveal
        || !cemetery_body(ability)
        || trigger.limit.is_some()
        || trigger.aggregation != (TriggerAggregation::PerFact {})
    {
        return Err(unsupported("printed cemetery per-fact card Play header"));
    }
    Ok(true)
}

fn cemetery_body(ability: &Ability) -> bool {
    let Some(body) = &ability.body else {
        return false;
    };
    let Some(first) = body.steps.first() else {
        return true;
    };
    let mut returns = BTreeSet::new();
    field_return_paths(body, "", &mut returns);
    Requirements::derive_program(&Located {
        node: body.clone(),
        source: first.source.clone(),
    })
    .required
    .iter()
    .filter(|requirement| requirement.capability.code == "shape.ObjectOperand.SourceHolder")
    .all(|requirement| {
        requirement.locations.iter().all(|location| {
            location.ir_path.ends_with("/instruction/source/holder")
                || returns.contains(&location.ir_path)
        })
    })
}

fn returns_holder(action: &Action) -> bool {
    matches!(action,
        Action::Move { objects: Selector::Objects { objects }, movement, reason: MoveReason::Put }
            if matches!(objects.as_slice(), [ObjectOperand::SourceHolder {}])
                && matches!(&movement.to, MoveDestination::Shared { zone } if zone.player == (PlayerRef::Controller {}) && zone.kind == ZoneKind::Field)
                && movement.initial_posture.is_none() && movement.entry_effects.is_empty()
                && movement.position.is_none() && movement.order.is_none())
}

fn field_return_paths(program: &Program, prefix: &str, paths: &mut BTreeSet<String>) {
    for (index, step) in program.steps.iter().enumerate() {
        let path = format!("{prefix}/steps/{index}");
        match &step.node {
            Node::Instruction { instruction } if returns_holder(&instruction.action) => {
                paths.insert(format!("{path}/instruction/action/objects/objects/0"));
            }
            Node::Control {
                control: Control::Sequence { body },
            } => {
                field_return_paths(body, &format!("{path}/control/body"), paths);
            }
            Node::Control {
                control:
                    Control::If {
                        then_body,
                        else_body,
                        ..
                    },
            } => {
                field_return_paths(then_body, &format!("{path}/control/then_body"), paths);
                if let Some(body) = else_body {
                    field_return_paths(body, &format!("{path}/control/else_body"), paths);
                }
            }
            Node::Control {
                control: Control::Mode { modes, .. },
            } => {
                for (mode_index, mode) in modes.iter().enumerate() {
                    field_return_paths(
                        &mode.body,
                        &format!("{path}/control/modes/{mode_index}/body"),
                        paths,
                    );
                }
            }
            Node::Control {
                control: Control::EachPlayer { stages, .. },
            } => {
                for (stage_index, stage) in stages.iter().enumerate() {
                    field_return_paths(
                        &stage.body,
                        &format!("{path}/control/stages/{stage_index}/body"),
                        paths,
                    );
                }
            }
            _ => {}
        }
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "The card play commit boundary owns cemetery trigger admission."
)]
impl Game {
    pub(in crate::game::internal) fn internal_played_card_events(
        &mut self,
        execution: &Execution,
        costs: &CostSpec,
    ) -> Result<()> {
        if !self.card_runtime.definitions.cards.values().any(|program| {
            program
                .card()
                .abilities
                .iter()
                .any(|ability| filter(&ability.node).is_some())
        }) {
            return Ok(());
        }
        let actor = execution.context.controller;
        if actor != player_id(self.active())? {
            return Ok(());
        }
        if !matches!(
            self.object_type(&execution.context.source)?,
            "スペル" | "スペル・トークン" | "スペル・アドバンス"
        ) {
            return Ok(());
        }
        // This subscription reads only class; a full snapshot would require unrelated title bindings.
        let class = self.face(&execution.context.source)?["card_class"]
            .as_str()
            .ok_or_else(|| invalid("missing played card class"))?
            .to_owned();
        let mut matched = Vec::new();
        for holder in self.zone_ids(super::super::seat_name(actor)?, "cemetery") {
            let (card, _) = self.internal_effective_print(&holder)?;
            if !self.card_runtime.is_loaded(&card)
                || player_id(&self.object(&holder)?.controller)? != actor
            {
                continue;
            }
            for ability in self.internal_printed_abilities(&holder)? {
                if let Some(plan) = filter(&ability.node)
                    && class == plan.class.0
                {
                    matched.push((holder.clone(), ability));
                }
            }
        }
        if matched.is_empty() {
            return Ok(());
        }
        let CostSpec::Ordered { steps } = costs else {
            return Err(unsupported("card play costs"));
        };
        let intrinsic = steps
            .first()
            .ok_or_else(|| invalid("missing intrinsic Spell payment"))?;
        let from = match self.object(&execution.context.source)?.state["entered_from"].as_str() {
            Some("hand") => ZoneKind::Hand,
            Some("ex") => ZoneKind::Ex,
            _ => return Err(unsupported("card Play origin")),
        };
        let event = PlayEvent {
            subject: PlaySubject::Card {
                object: self.internal_current_reference(&execution.context.source)?,
            },
            controller: actor,
            origin_zone: RuntimeZoneRef {
                player: actor,
                kind: from,
            },
            cost: RuntimeReceiptId {
                execution: execution.id,
                slot: intrinsic.instruction.receipt,
            },
        };
        let header = self.pending_header(
            actor,
            Cause::Effect {
                execution: execution.id,
            },
            execution.ability,
            SnapshotId(self.state.ir_epoch),
        )?;
        let mut candidates = Vec::new();
        for (holder, ability) in matched {
            candidates.push(self.timing_occurrence(
                &holder,
                ability,
                &header,
                EventPayload::Play { event },
            )?);
        }
        self.internal_admit_pending(candidates)
    }
}
