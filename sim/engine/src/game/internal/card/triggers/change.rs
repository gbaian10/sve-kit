//! Numeric and posture commits enqueue ordinary immutable pending occurrences.
use super::{
    Ability, AbilityKind, AbilityOrigin, Cause, EventPattern, EventPayload, EventSnapshotPolicy,
    Game, LimitSpec, Result, SnapshotId, TriggerAggregation, TriggerCondition, ZoneKind, invalid,
    optional_payment, player_id, unsupported,
};
use crate::game::{Object, int};
use crate::ir::events::change::{PostureFilter, StatFilter};
use crate::ir::{ExecutionId, ObjectRef, PlayerId, RuntimeHolder};
use serde_json::Value;

fn subscribes(ability: &Ability) -> bool {
    matches!(
        ability.trigger.as_ref().map(|trigger| &trigger.on),
        Some(TriggerCondition::Event {
            pattern: EventPattern::Posture { .. } | EventPattern::StatChange { .. }
        })
    )
}

pub(super) fn check_header(ability: &Ability) -> Result<bool> {
    let Some(trigger) = &ability.trigger else {
        return Ok(false);
    };
    let TriggerCondition::Event { pattern } = &trigger.on else {
        return Ok(false);
    };
    let parsed = match pattern {
        EventPattern::Posture { binding, predicate } => {
            PostureFilter::parse(*binding, predicate).is_some()
        }
        EventPattern::StatChange { binding, predicate } => {
            StatFilter::parse(*binding, predicate).is_some()
        }
        _ => return Ok(false),
    };
    if !parsed
        || !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        || ability.kind != AbilityKind::Triggered
        || !optional_payment::trigger_play(ability)
        || trigger.active_zones != [ZoneKind::Field]
        || trigger.view != EventSnapshotPolicy::AfterEntry
        || trigger.aggregation != (TriggerAggregation::PerFact {})
        || trigger
            .limit
            .is_some_and(|limit| !LimitSpec::is_triggered(limit))
    {
        return Err(unsupported("printed per-fact change header"));
    }
    Ok(true)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Change events share the common pending authority."
)]
impl Game {
    pub(in crate::game) fn internal_posture_events(
        &mut self,
        changed: &[Object],
        acted: bool,
        cause: &Value,
        before: SnapshotId,
    ) -> Result<()> {
        use crate::ir::{Posture, PostureEvent};
        if changed.is_empty() || self.card_runtime.is_empty() {
            return Ok(());
        }
        let events = changed
            .iter()
            .map(|object| {
                Ok(EventPayload::Posture {
                    event: PostureEvent {
                        object: self.internal_current_reference(&object.id)?,
                        before: if acted {
                            Posture::Standing
                        } else {
                            Posture::Acted
                        },
                        after: if acted {
                            Posture::Acted
                        } else {
                            Posture::Standing
                        },
                    },
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.internal_change_events(&events, cause, before)
    }

    pub(in crate::game) fn internal_change_events(
        &mut self,
        events: &[EventPayload],
        cause: &Value,
        before: SnapshotId,
    ) -> Result<()> {
        if events.is_empty()
            || !self.card_runtime.definitions.cards.values().any(|program| {
                program
                    .card()
                    .abilities
                    .iter()
                    .any(|ability| subscribes(&ability.node))
            })
        {
            return Ok(());
        }
        let active = player_id(self.active())?;
        let mut candidates = Vec::new();
        for event in events {
            let observers = self.change_observers(event, active)?;
            if observers.is_empty() {
                continue;
            }
            let source = self.card_runtime.active_source;
            let authority = if let Some(number) = cause["effect"]["execution"].as_u64() {
                Cause::Effect {
                    execution: ExecutionId(number),
                }
            } else if cause["decision"].is_string() {
                Cause::PlayerAction { player: active }
            } else {
                Cause::Rule {
                    rule: self.rule_id(
                        cause["rule"]
                            .as_str()
                            .ok_or_else(|| unsupported("change without authority cause"))?,
                    )?,
                }
            };
            let header = self.pending_header(
                source.map_or(active, |source| source.controller),
                authority,
                if matches!(authority, Cause::Effect { .. }) {
                    source
                } else {
                    None
                },
                before,
            )?;
            for (holder, ability) in observers {
                candidates.push(self.timing_occurrence(
                    &holder,
                    ability,
                    &header,
                    event.clone(),
                )?);
            }
        }
        self.internal_admit_pending(candidates)
    }

    fn change_observers(
        &mut self,
        event: &EventPayload,
        active: PlayerId,
    ) -> Result<Vec<(String, super::Located<Ability>)>> {
        let mut observers = Vec::new();
        for holder in self.field_ids() {
            let (card, _) = self.internal_effective_print(&holder)?;
            // Unadmitted evolution prints retain their existing pending-ability boundary.
            if !self
                .card_runtime
                .definitions
                .cards
                .get(&card)
                .is_some_and(|program| {
                    program
                        .card()
                        .abilities
                        .iter()
                        .any(|ability| subscribes(&ability.node))
                })
            {
                continue;
            }
            let actor = player_id(&self.object(&holder)?.controller)?;
            let source = self.internal_current_reference(&holder)?;
            for ability in self.internal_printed_abilities(&holder)? {
                if let Some(trigger) = &ability.node.trigger
                    && let TriggerCondition::Event { pattern } = &trigger.on
                    && self.change_matches(event, pattern, source, actor, active)?
                {
                    observers.push((holder.clone(), ability));
                }
            }
        }
        Ok(observers)
    }

    fn change_matches(
        &mut self,
        event: &EventPayload,
        pattern: &EventPattern,
        source: ObjectRef,
        actor: PlayerId,
        active: PlayerId,
    ) -> Result<bool> {
        match (event, pattern) {
            (
                EventPayload::StatChange { event },
                EventPattern::StatChange { binding, predicate },
            ) => Ok(StatFilter::parse(*binding, predicate)
                .is_some_and(|plan| plan.matches(event, source))),
            (EventPayload::Posture { event }, EventPattern::Posture { binding, predicate }) => {
                let Some(plan) = PostureFilter::parse(*binding, predicate) else {
                    return Ok(false);
                };
                if event.before == event.after || event.after != plan.after {
                    return Ok(false);
                }
                let id = self
                    .internal_identities()?
                    .get(&event.object.id)
                    .cloned()
                    .ok_or_else(|| invalid("unknown change subject"))?;
                let information = self.event_subject_information(&id)?;
                let controller = player_id(&self.object(&id)?.controller)?;
                Ok(plan.subject.matches_information(
                    &information.borrowed(),
                    source == event.object,
                    actor,
                    controller,
                    active,
                    information.token,
                ))
            }
            _ => Ok(false),
        }
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Super evolution's explicit addition shares change-event admission."
)]
impl Game {
    pub(in crate::game) fn internal_super_stat_event(
        &mut self,
        previous: &Object,
        cause: &Value,
        before: SnapshotId,
    ) -> Result<()> {
        use crate::ir::{
            CommitId, Int, NonEmpty, Stat, StatChangeEvent, StatChangeReason, StatDelta,
            StatOperation,
        };
        if self.card_runtime.is_empty() {
            return Ok(());
        }
        let current = self.object(&previous.id)?;
        let mut facets = Vec::new();
        for (field, property) in [("power", Stat::Attack), ("hp", Stat::Life)] {
            let old = int(&previous.state[field]);
            let new = int(&current.state[field]);
            if old != new {
                facets.push(StatDelta {
                    property,
                    before: Int(old),
                    after: Int(new),
                    delta: Int(new
                        .checked_sub(old)
                        .ok_or_else(|| invalid("super evolution delta overflow"))?),
                    operation: StatOperation::Add,
                });
            }
        }
        let mut facets = facets.into_iter();
        let Some(first) = facets.next() else {
            return Ok(());
        };
        self.state.ir_epoch = self
            .state
            .ir_epoch
            .checked_add(1)
            .ok_or_else(|| invalid("super evolution commit overflow"))?;
        let event = StatChangeEvent {
            subject: RuntimeHolder::Object {
                object: self.internal_current_reference(&previous.id)?,
            },
            facets: NonEmpty {
                first,
                rest: facets.collect(),
            },
            reason: StatChangeReason::SuperEvolutionRule,
            commit: CommitId(self.state.ir_epoch),
            instruction: None,
        };
        self.internal_change_events(&[EventPayload::StatChange { event }], cause, before)
    }
}
