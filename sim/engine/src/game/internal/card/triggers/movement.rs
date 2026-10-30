//! Movement observers separate the event subject from their live or departed source information.
use super::{
    Ability, AbilityKind, AbilityOrigin, Cause, EventHeader, EventPattern, EventPayload,
    EventSnapshotPolicy, Frame, Game, LeaveFieldEvent, LimitSpec, Located, Object,
    PendingOccurrence, Result, RuntimeZoneRef, SnapshotId, SourceInformation, TriggerAggregation,
    TriggerCondition, ZoneKind, invalid, optional_payment, player_id, unsupported,
};
use crate::ir::events::movement::{MovementFields, MovementFilter};
use crate::ir::{EnterExEvent, EntryOrigin, PlayerId};
use alloc::collections::BTreeMap;

fn filter(ability: &Ability) -> Option<MovementFilter> {
    if !matches!(ability.source.origin, AbilityOrigin::Printed { .. }) {
        return None;
    }
    let TriggerCondition::Event { pattern } = &ability.trigger.as_ref()?.on else {
        return None;
    };
    match pattern {
        EventPattern::LeaveField { binding, predicate } => {
            MovementFilter::parse(MovementFields::Leave(*binding), predicate)
        }
        EventPattern::EnterEx { binding, predicate } => {
            MovementFilter::parse(MovementFields::Ex(*binding), predicate)
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
                    pattern: EventPattern::LeaveField { .. }
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

fn matches_kind(ability: &Ability, exit: bool) -> bool {
    matches!(
        ability.trigger.as_ref().map(|trigger| &trigger.on),
        Some(TriggerCondition::Event {
            pattern: EventPattern::LeaveField { .. }
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

    pub(super) fn movement_departures(
        &mut self,
        id: &str,
        destinations: &BTreeMap<String, String>,
        before: SnapshotId,
        frame: &Frame,
    ) -> Result<Vec<PendingOccurrence>> {
        let previous = self.object(id)?.clone();
        if previous.zone != "field" || destinations.get(id).map(String::as_str) != Some("cemetery")
        {
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
                    to: RuntimeZoneRef {
                        player: player_id(&previous.owner)?,
                        kind: ZoneKind::Cemetery,
                    },
                };
                candidates.push(self.new_pending(
                    &holder,
                    source,
                    ability,
                    information,
                    header,
                    Some(leave),
                )?);
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
