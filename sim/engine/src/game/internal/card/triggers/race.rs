//! Race facts enter the same per-fact pending queue as the other printed events.
use super::{Game, Result, invalid, player_id, unsupported};
use crate::game::internal::Execution;
use crate::game::internal::conditions::subject as subject_condition;
use crate::ir::events::race;
use crate::ir::optional_payment;
use crate::ir::{
    Ability, AbilityKind, AbilityOrigin, EventSnapshotPolicy, KeywordId, LimitSpec,
    TriggerAggregation, ZoneKind,
};
use crate::ir::{
    Cause, DriveGainedEvent, EventPattern, EventPayload, RaceEvent, SnapshotId, TriggerCondition,
};

impl Game {
    pub(in crate::game::internal) fn internal_race_event(
        &mut self,
        holder: &str,
        event: RaceEvent,
        execution: &Execution,
    ) -> Result<()> {
        let information = self.event_subject_information(holder)?;
        let active = player_id(self.active())?;
        let mut observers = Vec::new();
        for source in self.field_ids() {
            let actor = player_id(&self.object(&source)?.controller)?;
            for ability in self.internal_effective_abilities(&source)? {
                let matches = if let Some(filter) = race::filter(&ability.node) {
                    subject_condition::matches(
                        &filter,
                        &information.borrowed(),
                        source == holder,
                        actor,
                        event.controller,
                        active,
                        false,
                    )?
                } else {
                    source == holder
                        && matches!(
                            ability.node.source.origin,
                            AbilityOrigin::Keyword {
                                keyword: KeywordId::OnRace,
                                ..
                            }
                        )
                        && matches!(
                            ability.node.trigger.as_ref().map(|trigger| &trigger.on),
                            Some(TriggerCondition::Event {
                                pattern: EventPattern::Race { .. }
                            })
                        )
                };
                if matches {
                    observers.push((source.clone(), ability));
                }
            }
        }
        if observers.is_empty() {
            return Ok(());
        }
        let header = self.pending_header(
            event.controller,
            Cause::Effect {
                execution: execution.id,
            },
            execution.ability,
            SnapshotId(self.state.ir_epoch),
        )?;
        let mut pending = Vec::new();
        for (source, ability) in observers {
            pending.push(self.timing_occurrence(
                &source,
                ability,
                &header,
                EventPayload::Race { event },
            )?);
        }
        self.internal_admit_pending(pending)
    }
}

impl Game {
    pub(in crate::game::internal) fn internal_drive_gained_event(
        &mut self,
        holder: &str,
        event: DriveGainedEvent,
        execution: &Execution,
    ) -> Result<()> {
        let abilities = self
            .internal_printed_abilities(holder)?
            .into_iter()
            .filter(|a| {
                matches!(
                    a.node.trigger.as_ref().map(|t| &t.on),
                    Some(TriggerCondition::Event {
                        pattern: EventPattern::DriveGained { .. }
                    })
                )
            })
            .collect::<Vec<_>>();
        if abilities.is_empty() {
            return Ok(());
        }
        let header = self.pending_header(
            event.controller,
            Cause::Effect {
                execution: execution.id,
            },
            execution.ability,
            SnapshotId(self.state.ir_epoch),
        )?;
        let mut pending = vec![];
        for ability in abilities {
            pending.push(self.timing_occurrence(
                holder,
                ability,
                &header,
                EventPayload::DriveGained { event },
            )?);
        }
        self.internal_admit_pending(pending)
    }
}

pub(super) fn check_header(ability: &Ability) -> Result<bool> {
    if race::filter(ability).is_none() {
        return Ok(false);
    }
    let trigger = ability
        .trigger
        .as_ref()
        .ok_or_else(|| invalid("missing race trigger"))?;
    if ability.kind != AbilityKind::Triggered
        || !optional_payment::trigger_play(ability)
        || ability.active_zones != [ZoneKind::Field]
        || trigger.active_zones != ability.active_zones
        || trigger.view != EventSnapshotPolicy::AfterEntry
        || trigger.aggregation != (TriggerAggregation::PerFact {})
        || trigger
            .limit
            .is_some_and(|limit| !LimitSpec::is_triggered(limit))
    {
        return Err(unsupported("printed race observer header"));
    }
    Ok(true)
}
