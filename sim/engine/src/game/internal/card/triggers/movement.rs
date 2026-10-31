//! Movement observers separate the event subject from their live or departed source information.
use super::{
    Ability, AbilityKind, AbilityOrigin, Cause, EventHeader, EventPattern, EventPayload,
    EventSnapshotPolicy, Frame, Game, LeaveFieldEvent, LimitSpec, Located, Object,
    PendingOccurrence, Result, RuntimeZoneRef, SnapshotId, SourceInformation, TriggerAggregation,
    TriggerCondition, ZoneKind, invalid, optional_payment, player_id, unsupported,
};
use crate::ir::events::subject::{SubjectFields, SubjectFilter};
use crate::ir::{DestroyEvent, EnterExEvent, EntryOrigin, PlayerId};
use alloc::collections::BTreeMap;

fn filter(ability: &Ability) -> Option<SubjectFilter> {
    if !matches!(ability.source.origin, AbilityOrigin::Printed { .. }) {
        return None;
    }
    let TriggerCondition::Event { pattern } = &ability.trigger.as_ref()?.on else {
        return None;
    };
    match pattern {
        EventPattern::LeaveField { binding, predicate } => {
            SubjectFilter::parse(SubjectFields::Cemetery(*binding), predicate)
                .or_else(|| SubjectFilter::parse(SubjectFields::Leave(*binding), predicate))
        }
        EventPattern::Destroy { binding, predicate } => {
            SubjectFilter::parse(SubjectFields::Destroy(*binding), predicate)
        }
        EventPattern::EnterEx { binding, predicate } => {
            SubjectFilter::parse(SubjectFields::Ex(*binding), predicate)
        }
        _ => None,
    }
}

pub(super) fn check_header(ability: &Ability) -> Result<bool> {
    if filter(ability).is_none() {
        return Ok(false);
    }
    let trigger = ability
        .trigger
        .as_ref()
        .ok_or_else(|| invalid("missing movement trigger"))?;
    if ability.kind != AbilityKind::Triggered
        || !optional_payment::trigger_play(ability)
        || trigger.active_zones != [ZoneKind::Field]
        || trigger.view
            != if matches!(
                trigger.on,
                TriggerCondition::Event {
                    pattern: EventPattern::LeaveField { .. } | EventPattern::Destroy { .. }
                }
            ) {
                EventSnapshotPolicy::BeforeExit
            } else {
                EventSnapshotPolicy::AfterEntry
            }
        || trigger.aggregation != (TriggerAggregation::PerFact {})
        || trigger
            .limit
            .is_some_and(|limit| limit != LimitSpec::once_per_turn())
    {
        return Err(unsupported("printed per-fact movement header"));
    }
    Ok(true)
}

pub(super) fn matches_kind(ability: &Ability, exit: bool) -> bool {
    matches!(
        ability.trigger.as_ref().map(|trigger| &trigger.on),
        Some(TriggerCondition::Event {
            pattern: EventPattern::LeaveField { .. } | EventPattern::Destroy { .. }
        })
    ) == exit
        && filter(ability).is_some()
}

fn origin(previous: &Object) -> Result<EntryOrigin> {
    Ok(match previous.zone.as_str() {
        "void" => EntryOrigin::Other {},
        "ex" => EntryOrigin::Zone { zone: ZoneKind::Ex },
        "field" => EntryOrigin::Zone {
            zone: ZoneKind::Field,
        },
        "hand" => EntryOrigin::Zone {
            zone: ZoneKind::Hand,
        },
        "deck" => EntryOrigin::Zone {
            zone: ZoneKind::Deck,
        },
        "cemetery" => EntryOrigin::Zone {
            zone: ZoneKind::Cemetery,
        },
        "banish" => EntryOrigin::Zone {
            zone: ZoneKind::Banish,
        },
        "evolve_deck" => EntryOrigin::Zone {
            zone: ZoneKind::Evolve,
        },
        _ => return Err(unsupported("EX entry origin")),
    })
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Both projections belong to the common movement transaction."
)]
impl Game {
    fn movement_header(
        &mut self,
        id: &str,
        before: SnapshotId,
        frame: &Frame,
        active: PlayerId,
    ) -> Result<EventHeader> {
        let cause = self.departure_cause(id, frame)?;
        let source = self.card_runtime.active_source;
        self.pending_header(
            source.map_or(active, |source| source.controller),
            cause,
            if matches!(cause, Cause::Rule { .. }) {
                None
            } else {
                source
            },
            before,
        )
    }

    fn movement_observers(&self, exit: bool) -> Result<Vec<(String, Vec<Located<Ability>>)>> {
        if !self.card_runtime.definitions.cards.values().any(|program| {
            program
                .card()
                .abilities
                .iter()
                .any(|ability| matches_kind(&ability.node, exit))
        }) {
            return Ok(Vec::new());
        }
        self.field_ids()
            .into_iter()
            .map(|id| {
                let abilities = self
                    .internal_printed_abilities(&id)?
                    .into_iter()
                    .filter(|ability| matches_kind(&ability.node, exit))
                    .collect();
                Ok((id, abilities))
            })
            .collect()
    }

    fn departure_destination(
        &self,
        previous: &Object,
        destination: &str,
        side: Option<&str>,
        frame: &Frame,
    ) -> Result<RuntimeZoneRef> {
        let kind = match destination {
            "cemetery" => ZoneKind::Cemetery,
            "hand" => ZoneKind::Hand,
            "deck" => ZoneKind::Deck,
            "banish" => ZoneKind::Banish,
            "ex" => ZoneKind::Ex,
            "evolve_deck" => ZoneKind::Evolve,
            _ => return Err(unsupported("departure destination")),
        };
        let to_player =
            player_id(&self.placement_controller(previous, destination, side, frame)?)?;
        Ok(RuntimeZoneRef {
            player: to_player,
            kind,
        })
    }

    pub(super) fn movement_departures(
        &mut self,
        id: &str,
        destinations: &BTreeMap<String, String>,
        before: SnapshotId,
        side: Option<&str>,
        frame: &Frame,
    ) -> Result<Vec<PendingOccurrence>> {
        let previous = self.object(id)?.clone();
        let Some(destination) = destinations.get(id).map(String::as_str) else {
            return Ok(Vec::new());
        };
        if previous.zone != "field" || destination == "field" {
            return Ok(Vec::new());
        }
        let observers = self.movement_observers(true)?;
        if observers.iter().all(|(_, abilities)| abilities.is_empty()) {
            return Ok(Vec::new());
        }
        let object = self.internal_current_reference(id)?;
        let lki = self.information_snapshot(object)?.at(before).view();
        let characters = self.internal_characteristics(id)?;
        let controller = player_id(&previous.controller)?;
        let token = self.object_type(id)?.contains("トークン");
        let active = player_id(self.active())?;
        let mut candidates = Vec::new();
        let mut shared_header: Option<EventHeader> = None;
        for (holder, abilities) in observers {
            let actor = player_id(&self.object(&holder)?.controller)?;
            for ability in abilities {
                let on = &ability
                    .node
                    .trigger
                    .as_ref()
                    .ok_or_else(|| invalid("missing departure trigger"))?
                    .on;
                let destroyed = matches!(
                    on,
                    TriggerCondition::Event {
                        pattern: EventPattern::Destroy { .. }
                    }
                );
                let marked_destroyed = frame
                    .values
                    .get("destroyed_subjects")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|ids| ids.iter().any(|value| value.as_str() == Some(id)));
                if destroyed && (destination != "cemetery" || !marked_destroyed) {
                    continue;
                }
                if let TriggerCondition::Event {
                    pattern: EventPattern::LeaveField { binding, predicate },
                } = on
                    && SubjectFilter::parse(SubjectFields::Cemetery(*binding), predicate).is_some()
                    && destination != "cemetery"
                {
                    continue;
                }
                let plan =
                    filter(&ability.node).ok_or_else(|| invalid("missing movement filter"))?;
                if !plan.matches(&characters, holder == id, actor, controller, active, token) {
                    continue;
                }
                let source = self.internal_card_source(&holder, ability.node.id, actor)?;
                let mut information = SourceInformation::current(self, source)?;
                if destinations
                    .get(&holder)
                    .is_some_and(|zone| zone != "field")
                {
                    information.seal(Some(self.information_snapshot(source.holder)?.at(before)));
                }
                let header = if let Some(header) = &shared_header {
                    header.clone()
                } else {
                    let generated = self.movement_header(id, before, frame, active)?;
                    shared_header = Some(generated.clone());
                    generated
                };
                let leave = LeaveFieldEvent {
                    object_before: object,
                    controller,
                    lki,
                    to: self.departure_destination(&previous, destination, side, frame)?,
                };
                let mut pending = self.new_pending(
                    &holder,
                    source,
                    ability,
                    information,
                    header,
                    if destroyed { None } else { Some(leave) },
                )?;
                if destroyed {
                    pending.timing = Some(EventPayload::Destroy {
                        event: DestroyEvent {
                            object_before: object,
                            characters: characters.clone(),
                            controller,
                        },
                    });
                }
                candidates.push(pending);
            }
        }
        Ok(candidates)
    }

    pub(in crate::game) fn internal_ex_events(
        &mut self,
        previous: &[Object],
        before: SnapshotId,
        frame: &Frame,
    ) -> Result<()> {
        if previous.is_empty() {
            return Ok(());
        }
        let observers = self.movement_observers(false)?;
        if observers.iter().all(|(_, abilities)| abilities.is_empty()) {
            return Ok(());
        }
        let active = player_id(self.active())?;
        let mut candidates = Vec::new();
        for old in previous {
            let object = self.internal_current_reference(&old.id)?;
            if object.generation.0
                != old
                    .generation
                    .checked_add(1)
                    .ok_or_else(|| invalid("EX generation overflow"))?
            {
                return Err(invalid("EX entry did not commit"));
            }
            let controller = player_id(&self.object(&old.id)?.controller)?;
            let characters = self.internal_characteristics(&old.id)?;
            let mut shared_header: Option<EventHeader> = None;
            for (holder, abilities) in &observers {
                let actor = player_id(&self.object(holder)?.controller)?;
                for ability in abilities {
                    let plan = filter(&ability.node).ok_or_else(|| invalid("missing EX filter"))?;
                    if !plan.matches(
                        &characters,
                        *holder == old.id,
                        actor,
                        controller,
                        active,
                        false,
                    ) {
                        continue;
                    }
                    let source = self.internal_card_source(holder, ability.node.id, actor)?;
                    let information = SourceInformation::current(self, source)?;
                    let header = if let Some(header) = &shared_header {
                        header.clone()
                    } else {
                        let generated = self.movement_header(&old.id, before, frame, active)?;
                        shared_header = Some(generated.clone());
                        generated
                    };
                    let mut pending = self.new_pending(
                        holder,
                        source,
                        ability.clone(),
                        information,
                        header,
                        None,
                    )?;
                    pending.timing = Some(EventPayload::EnterEx {
                        event: EnterExEvent {
                            object,
                            from: origin(old)?,
                            controller,
                            characters: characters.clone(),
                        },
                    });
                    candidates.push(pending);
                }
            }
        }
        self.internal_admit_pending(candidates)
    }
}
