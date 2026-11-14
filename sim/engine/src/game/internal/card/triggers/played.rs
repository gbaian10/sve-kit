//! Committed card plays capture current observers before entry or Spell body effects.
use super::{
    Ability, AbilityKind, Cause, EventPayload, EventSnapshotPolicy, Game, Result, SnapshotId,
    TriggerAggregation, ZoneKind, invalid, player_id, unsupported,
};
use crate::game::internal::Execution;
use crate::game::internal::conditions::subject as subject_condition;
use crate::game::internal::grant::EffectiveAbility;
use crate::ir::events::played::filter;
use crate::ir::events::subscriptions;
use crate::ir::optional_payment;
use crate::ir::{
    Action, CharacteristicPatch, Control, Effect, GrantedAbility, LimitSpec, Located,
    MoveDestination, MoveReason, Node, ObjectOperand, OwnerZone, PlayEvent, PlaySubject, PlayerId,
    PlayerRef, Program, ReceiptId, Requirements, RuntimeReceiptId, RuntimeZoneRef, Selector,
};

use alloc::collections::BTreeSet;

fn observes_play(ability: &Ability) -> bool {
    filter(ability).is_some()
        || ability.declarations.iter().any(|effect| {
            matches!(effect, Effect::Continuous {
                patch: CharacteristicPatch::GrantAbility { abilities }, ..
            } if abilities.iter().any(|granted| matches!(granted,
                GrantedAbility::Definition { ability: definition } if filter(definition).is_some())))
        })
}

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
        || (ability.active_zones == [ZoneKind::Cemetery] && !cemetery_body(ability))
        || trigger
            .limit
            .is_some_and(|limit| !LimitSpec::is_triggered(limit))
        || trigger.aggregation != (TriggerAggregation::PerFact {})
    {
        return Err(unsupported("printed per-fact card Play header"));
    }
    Ok(true)
}

/// Cemetery holder reads stay closed to admitted movement and the entry trigger's self cost.
pub(super) fn cemetery_body(ability: &Ability) -> bool {
    let Some(body) = &ability.body else {
        return false;
    };
    let Some(first) = body.steps.first() else {
        return true;
    };
    let mut returns = BTreeSet::new();
    if super::super::optional::cemetery_self_banish(ability) {
        returns
            .insert("/steps/0/control/costs/steps/0/instruction/action/objects/objects/0".into());
    }
    holder_move_paths(body, "", &mut returns);
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
                && match &movement.to {
                    MoveDestination::Shared { zone } => zone.player == (PlayerRef::Controller {}) && zone.kind == ZoneKind::Field,
                    MoveDestination::Owners { zone } => *zone == OwnerZone::Ex,
                }
                && movement.initial_posture.is_none() && movement.entry_effects.is_empty()
                && movement.position.is_none() && movement.order.is_none())
}

fn holder_move_paths(program: &Program, prefix: &str, paths: &mut BTreeSet<String>) {
    for (index, step) in program.steps.iter().enumerate() {
        let path = format!("{prefix}/steps/{index}");
        match &step.node {
            Node::Instruction { instruction } if returns_holder(&instruction.action) => {
                paths.insert(format!("{path}/instruction/action/objects/objects/0"));
            }
            Node::Control {
                control: Control::Pay { on_paid, .. },
            } => {
                holder_move_paths(on_paid, &format!("{path}/control/on_paid"), paths);
            }
            // 「…てよい」 keeps the same self-return inside its May body.
            Node::Control {
                control: Control::Sequence { body } | Control::May { body, .. },
            } => {
                holder_move_paths(body, &format!("{path}/control/body"), paths);
            }
            Node::Control {
                control:
                    Control::If {
                        then_body,
                        else_body,
                        ..
                    },
            } => {
                holder_move_paths(then_body, &format!("{path}/control/then_body"), paths);
                if let Some(body) = else_body {
                    holder_move_paths(body, &format!("{path}/control/else_body"), paths);
                }
            }
            Node::Control {
                control: Control::Mode { modes, .. },
            } => {
                for (mode_index, mode) in modes.iter().enumerate() {
                    holder_move_paths(
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
                    holder_move_paths(
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

use super::subject::EventSubjectInformation;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Card play commits share observer admission."
)]
impl Game {
    fn played_observers(
        &self,
        source: &str,
        info: &mut EventSubjectInformation,
        actor: PlayerId,
    ) -> Result<Vec<(String, EffectiveAbility)>> {
        let mut cost_loaded = false;
        let active = player_id(self.active())?;
        let mut observers = self.field_ids();
        observers.extend(self.zone_ids("P1", "ex"));
        observers.extend(self.zone_ids("P2", "ex"));
        observers.extend(self.zone_ids("P1", "cemetery"));
        observers.extend(self.zone_ids("P2", "cemetery"));
        let mut matched = Vec::new();
        for holder in observers {
            let zone = if self.object(&holder)?.zone == "field" {
                ZoneKind::Field
            } else if self.object(&holder)?.zone == "ex" {
                ZoneKind::Ex
            } else {
                ZoneKind::Cemetery
            };
            let (card, _) = self.internal_effective_print(&holder)?;
            if !self.card_runtime.is_loaded(&card) {
                if zone == ZoneKind::Field {
                    self.require_internal_abilities(&holder, "field Play observer")?;
                }
                continue;
            }
            let controller = player_id(&self.object(&holder)?.controller)?;
            for ability in self.internal_effective_abilities(&holder)? {
                if ability.node.active_zones == [zone]
                    && let Some(plan) = filter(&ability.node)
                {
                    if plan.original.is_some() && !cost_loaded {
                        info.original_cost = self
                            .internal_cost_print(source)?
                            .get("cost")
                            .and_then(known_original_cost);
                        cost_loaded = true;
                    }
                    if subject_condition::played(
                        &plan,
                        &info.borrowed(),
                        holder == source,
                        controller,
                        actor,
                        active,
                        info.token,
                    )? {
                        matched.push((holder.clone(), ability));
                    }
                }
            }
        }
        Ok(matched)
    }

    pub(in crate::game::internal) fn internal_played_card_events(
        &mut self,
        execution: &Execution,
        cost: ReceiptId,
    ) -> Result<()> {
        if !self.card_runtime.definitions.cards.values().any(|program| {
            if program
                .card()
                .abilities
                .iter()
                .any(|ability| subscriptions::compound(&ability.node))
            {
                return true;
            }
            program
                .card()
                .abilities
                .iter()
                .any(|ability| observes_play(&ability.node))
        }) && !self
            .card_runtime
            .granted_definitions
            .values()
            .any(|ability| filter(&ability.node).is_some())
        {
            return Ok(());
        }
        if !execution.receipts.contains_key(&cost) {
            return Err(invalid("missing committed card payment"));
        }
        let actor = execution.context.controller;
        let source = &execution.context.source;
        let mut info = self.event_subject_information(source)?;
        let matched = self.played_observers(source, &mut info, actor)?;
        if matched.is_empty() {
            return Ok(());
        }
        let from = execution
            .play_origin
            .ok_or_else(|| unsupported("card Play origin"))?;
        let event = PlayEvent {
            subject: PlaySubject::Card {
                object: self.internal_current_reference(source)?,
            },
            controller: actor,
            origin_zone: RuntimeZoneRef {
                player: execution.play_origin_player.unwrap_or(actor),
                kind: from,
            },
            cost: RuntimeReceiptId {
                execution: execution.id,
                slot: cost,
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

fn known_original_cost(value: &serde_json::Value) -> Option<i64> {
    let number = match value {
        serde_json::Value::Number(n) => n.as_i64(),
        serde_json::Value::String(s) => s.parse::<i64>().ok(),
        _ => None,
    };
    number.filter(|n| *n >= 0)
}

#[cfg(test)]
mod tests;
