//! Field observers of deck-to-cemetery moves read only the committed zones and owner.
use super::{
    Ability, AbilityKind, AbilityOrigin, EventHeader, EventPattern, EventPayload,
    EventSnapshotPolicy, Frame, Game, LimitSpec, Located, Object, Result, RuntimeZoneRef,
    SnapshotId, SourceInformation, TriggerAggregation, TriggerCondition, ZoneKind, invalid,
    optional_payment, player_id, unsupported,
};
use crate::ir::events::mill::MillFilter;
use crate::ir::{Generation, MoveEvent, MoveMethod, ObjectRef, Position};

fn filter(ability: &Ability) -> Option<MillFilter> {
    if !matches!(ability.source.origin, AbilityOrigin::Printed { .. }) {
        return None;
    }
    let TriggerCondition::Event {
        pattern: EventPattern::Move { binding, predicate },
    } = &ability.trigger.as_ref()?.on
    else {
        return None;
    };
    MillFilter::parse(*binding, predicate)
}

pub(super) fn check_header(ability: &Ability) -> Result<bool> {
    if filter(ability).is_none() {
        return Ok(false);
    }
    let trigger = ability
        .trigger
        .as_ref()
        .ok_or_else(|| invalid("missing mill trigger"))?;
    if ability.kind != AbilityKind::Triggered
        || !optional_payment::trigger_play(ability)
        || trigger.active_zones != [ZoneKind::Field]
        || trigger.view != EventSnapshotPolicy::BeforeExit
        || trigger.aggregation != (TriggerAggregation::PerFact {})
        || trigger
            .limit
            .is_some_and(|limit| !LimitSpec::is_triggered(limit))
    {
        return Err(unsupported("printed per-card mill header"));
    }
    Ok(true)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Mill observers share the common movement transaction."
)]
impl Game {
    fn mill_observers(&self) -> Result<Vec<(String, Vec<Located<Ability>>)>> {
        if !self.card_runtime.definitions.cards.values().any(|program| {
            program
                .card()
                .abilities
                .iter()
                .any(|ability| filter(&ability.node).is_some())
        }) {
            return Ok(Vec::new());
        }
        self.field_ids()
            .into_iter()
            .map(|id| {
                let abilities = self
                    .internal_printed_abilities(&id)?
                    .into_iter()
                    .filter(|ability| filter(&ability.node).is_some())
                    .collect();
                Ok((id, abilities))
            })
            .collect()
    }

    /// Each card moved from a deck to its owner's cemetery is one occurrence per observer.
    pub(in crate::game) fn internal_mill_events(
        &mut self,
        previous: &[Object],
        before: SnapshotId,
        frame: &Frame,
    ) -> Result<()> {
        if previous.is_empty() {
            return Ok(());
        }
        let observers = self.mill_observers()?;
        if observers.iter().all(|(_, abilities)| abilities.is_empty()) {
            return Ok(());
        }
        let active = player_id(self.active())?;
        let mut candidates = Vec::new();
        for old in previous {
            let after = self.internal_current_reference(&old.id)?;
            if after.generation.0
                != old
                    .generation
                    .checked_add(1)
                    .ok_or_else(|| invalid("mill generation overflow"))?
            {
                return Err(invalid("mill did not commit"));
            }
            let owner = player_id(&old.controller)?;
            if player_id(&self.object(&old.id)?.controller)? != owner {
                return Err(invalid("milled card left its owner"));
            }
            let event = MoveEvent {
                before: ObjectRef {
                    id: after.id,
                    generation: Generation(old.generation),
                },
                after,
                from: RuntimeZoneRef {
                    player: owner,
                    kind: ZoneKind::Deck,
                },
                to: RuntimeZoneRef {
                    player: owner,
                    kind: ZoneKind::Cemetery,
                },
                position: Position::Top {},
                method: MoveMethod::Mill,
                entered_from: None,
            };
            let mut shared_header: Option<EventHeader> = None;
            for (holder, abilities) in &observers {
                let observer = player_id(&self.object(holder)?.controller)?;
                for ability in abilities {
                    let plan =
                        filter(&ability.node).ok_or_else(|| invalid("missing mill filter"))?;
                    if !plan.matches(owner, observer, active) {
                        continue;
                    }
                    let source = self.internal_card_source(holder, ability.node.id, observer)?;
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
                    pending.timing = Some(EventPayload::Move { event });
                    candidates.push(pending);
                }
            }
        }
        self.internal_admit_pending(candidates)
    }
}
