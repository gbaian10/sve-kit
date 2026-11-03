//! A committed UB captures observers before its body can change the field.
use super::{
    Ability, AbilityKind, Cause, EventPayload, EventSnapshotPolicy, Game, Result, SnapshotId,
    TriggerAggregation, ZoneKind, invalid, player_id, unsupported,
};
use crate::game::internal::Execution;
use crate::ir::events::ability::filter;
use crate::ir::optional_payment;
use crate::ir::{LimitSpec, RuntimeAbilityReference, UbActivatedEvent};

pub(super) fn check_header(ability: &Ability) -> Result<bool> {
    if filter(ability).is_none() {
        return Ok(false);
    }
    let trigger = ability
        .trigger
        .as_ref()
        .ok_or_else(|| invalid("missing UB trigger"))?;
    if ability.kind != AbilityKind::Triggered
        || !optional_payment::trigger_play(ability)
        || trigger.view != EventSnapshotPolicy::AtReveal
        || trigger
            .limit
            .is_some_and(|limit| !LimitSpec::is_triggered(limit))
        || trigger.aggregation != (TriggerAggregation::PerFact {})
    {
        return Err(unsupported("printed per-fact UB header"));
    }
    Ok(true)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "UB commits share observer admission."
)]
impl Game {
    pub(in crate::game::internal) fn internal_ub_activated_events(
        &mut self,
        execution: &Execution,
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
        let source = execution
            .ability
            .ok_or_else(|| invalid("missing UB source"))?;
        if self.object(&execution.context.source)?.zone != "field" {
            return Ok(());
        }
        let info = self.event_subject_information(&execution.context.source)?;
        let active = player_id(self.active())?;
        let mut matched = Vec::new();
        for holder in self.field_ids() {
            self.require_internal_abilities(&holder, "field UB observer")?;
            let controller = player_id(&self.object(&holder)?.controller)?;
            for ability in self.internal_printed_abilities(&holder)? {
                if ability.node.active_zones == [ZoneKind::Field]
                    && let Some(plan) = filter(&ability.node)
                    && plan.matches_information(
                        &info.borrowed(),
                        holder == execution.context.source,
                        controller,
                        source.controller,
                        active,
                        info.token,
                    )
                {
                    matched.push((holder.clone(), ability));
                }
            }
        }
        if matched.is_empty() {
            return Ok(());
        }
        let header = self.pending_header(
            source.controller,
            Cause::Effect {
                execution: execution.id,
            },
            Some(source),
            SnapshotId(self.state.ir_epoch),
        )?;
        let event = UbActivatedEvent {
            execution: execution.id,
            ability: RuntimeAbilityReference {
                holder: source.holder,
                instance: source.instance,
                definition: source.definition,
            },
            holder: source.holder,
            controller: source.controller,
            play_fact: header.origin,
        };
        let mut candidates = Vec::new();
        for (holder, ability) in matched {
            candidates.push(self.timing_occurrence(
                &holder,
                ability,
                &header,
                EventPayload::UbActivated { event },
            )?);
        }
        self.internal_admit_pending(candidates)
    }
}
