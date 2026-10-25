//! Entry matching runs after the whole successful batch has installed its effective information.
use super::{
    Ability, AbilityKind, AbilityOrigin, Cause, EventPattern, EventPayload, EventSnapshotPolicy,
    Game, PlayerId, Result, SnapshotId, SourceInformation, TriggerAggregation, TriggerCondition,
    ZoneKind, invalid, player_id, unsupported,
};
use crate::game::{Frame, Object};
use crate::ir::events::entry::EntryFilter;
use crate::ir::{CardKind, Characteristics, LimitSpec};
use crate::ir::{EnterFieldEvent, EntryMethod, EntryOrigin, ExecutionId};

pub(super) fn check_header(ability: &Ability) -> Result<bool> {
    let Some(trigger) = &ability.trigger else {
        return Ok(false);
    };
    let TriggerCondition::Event {
        pattern: EventPattern::EnterField { binding, predicate },
    } = &trigger.on
    else {
        return Ok(false);
    };
    if !matches!(ability.source.origin, AbilityOrigin::Printed { .. }) {
        return Ok(false);
    }
    if ability.kind != AbilityKind::Triggered
        || ability.play.is_some()
        || trigger.active_zones != [ZoneKind::Field]
        || trigger.view != EventSnapshotPolicy::AfterEntry
        || trigger.aggregation != (TriggerAggregation::PerFact {})
        || trigger
            .limit
            .is_some_and(|limit| limit != LimitSpec::once_per_turn())
        || EntryFilter::parse(*binding, predicate).is_none()
    {
        return Err(unsupported("printed per-fact entry header"));
    }
    Ok(true)
}

fn filter(ability: &Ability) -> Option<EntryFilter> {
    let TriggerCondition::Event {
        pattern: EventPattern::EnterField { binding, predicate },
    } = &ability.trigger.as_ref()?.on
    else {
        return None;
    };
    EntryFilter::parse(*binding, predicate)
}

fn entry_origin(previous: &Object) -> Result<ZoneKind> {
    let from = if previous.zone == "resolution" {
        previous.state["entered_from"]
            .as_str()
            .ok_or_else(|| invalid("missing original play zone"))?
    } else {
        &previous.zone
    };
    match from {
        "hand" => Ok(ZoneKind::Hand),
        "ex" => Ok(ZoneKind::Ex),
        "cemetery" => Ok(ZoneKind::Cemetery),
        "banish" => Ok(ZoneKind::Banish),
        "evolve_deck" => Ok(ZoneKind::Evolve),
        _ => Err(unsupported("unadmitted entry origin")),
    }
}

fn matches(
    plan: &EntryFilter,
    entered: (PlayerId, &Characteristics),
    same: bool,
    controller: PlayerId,
    active: PlayerId,
) -> bool {
    let (entry_controller, characters) = entered;
    plan.itself.is_none_or(|itself| itself == same)
        && plan
            .own
            .is_none_or(|own| own == (entry_controller == controller))
        && (!plan.follower || characters.kind == CardKind::Follower)
        && plan
            .trait_id
            .as_ref()
            .is_none_or(|trait_id| characters.traits.contains(trait_id))
        && (!plan.own_turn || active == controller)
}

fn entry_cause(frame: &Frame) -> Result<(Cause, PlayerId, EntryMethod)> {
    Ok(
        if let Some(execution) = frame.cause["effect"]["execution"].as_u64() {
            (
                Cause::Effect {
                    execution: ExecutionId(execution),
                },
                player_id(&frame.controller)?,
                EntryMethod::AbilityPut,
            )
        } else if frame.decision["do"] == "play" {
            (
                Cause::PlayerAction {
                    player: player_id(&frame.controller)?,
                },
                player_id(&frame.controller)?,
                EntryMethod::Play,
            )
        } else {
            return Err(unsupported("entry without admitted authority cause"));
        },
    )
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "The shared movement boundary owns entry event admission."
)]
impl Game {
    pub(in crate::game) fn internal_field_events(
        &mut self,
        previous: &[Object],
        before: SnapshotId,
        frame: &Frame,
    ) -> Result<()> {
        if previous.is_empty()
            || !self.card_runtime.cards.values().any(|program| {
                program
                    .card()
                    .abilities
                    .iter()
                    .any(|ability| filter(&ability.node).is_some())
            })
        {
            return Ok(());
        }
        let observers = self
            .field_ids()
            .into_iter()
            .map(|id| {
                let abilities = self
                    .internal_printed_abilities(&id)?
                    .into_iter()
                    .filter(|ability| filter(&ability.node).is_some())
                    .collect::<Vec<_>>();
                Ok((id, abilities))
            })
            .collect::<Result<Vec<_>>>()?;
        let active = player_id(self.active())?;
        let effect_source = self.card_runtime.active_source;
        let (cause, actor, method) = entry_cause(frame)?;
        let mut candidates = Vec::new();
        for old in previous {
            let object = self.internal_current_reference(&old.id)?;
            if object.generation.0
                != old
                    .generation
                    .checked_add(1)
                    .ok_or_else(|| invalid("entry generation overflow"))?
            {
                return Err(invalid("entry did not commit"));
            }
            let entry_controller = player_id(&self.object(&old.id)?.controller)?;
            let characters = self.internal_characteristics(&old.id)?;
            let header = self.pending_header(actor, cause, effect_source, before)?;
            let mut matched = Vec::new();
            for (holder, abilities) in &observers {
                let controller = player_id(&self.object(holder)?.controller)?;
                for ability in abilities {
                    let plan =
                        filter(&ability.node).ok_or_else(|| invalid("missing entry filter"))?;
                    if matches(
                        &plan,
                        (entry_controller, &characters),
                        *holder == old.id,
                        controller,
                        active,
                    ) {
                        matched.push((holder, ability, controller));
                    }
                }
            }
            if matched.is_empty() {
                continue;
            }
            // Only a matching occurrence publishes the origin; unadmitted zones still reject it.
            let event = EnterFieldEvent {
                object,
                from: EntryOrigin::Zone {
                    zone: entry_origin(old)?,
                },
                controller: entry_controller,
                characters,
                method,
                play_receipt: None,
            };
            for (holder, ability, controller) in matched {
                let source = self.internal_card_source(holder, ability.node.id, controller)?;
                let information = SourceInformation::current(self, source)?;
                let mut pending = self.new_pending(
                    holder,
                    source,
                    ability.clone(),
                    information,
                    header.clone(),
                    None,
                )?;
                pending.field_position = u64::try_from(
                    self.zone_ids(super::super::seat_name(event.controller)?, "field")
                        .iter()
                        .position(|id| *id == old.id)
                        .ok_or_else(|| invalid("entry has no field position"))?,
                )
                .map_err(invalid)?
                .checked_add(1)
                .ok_or_else(|| invalid("entry position overflow"))?;
                pending.timing = Some(EventPayload::EnterField {
                    event: event.clone(),
                });
                candidates.push(pending);
            }
        }
        self.internal_admit_pending(candidates)
    }
}
