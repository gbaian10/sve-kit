//! EX departures freeze the successful instruction batch before later resolutions change EX.
use super::{
    Ability, AbilityKind, AbilityOrigin, EventPattern, EventPayload, EventSnapshotPolicy, Frame,
    Game, LimitSpec, Object, Result, RuntimeZoneRef, SnapshotId, SourceInformation,
    TriggerAggregation, TriggerCondition, ZoneKind, invalid, optional_payment, player_id,
    unsupported,
};
use crate::ir::events::ex_departure::ExDepartureFilter;
use crate::ir::{
    BatchPolicy, Generation, MoveEvent, MoveMethod, Nat, ObjectRef, PlayerId, Position,
};

fn filter(ability: &Ability) -> Option<ExDepartureFilter> {
    if !matches!(ability.source.origin, AbilityOrigin::Printed { .. }) {
        return None;
    }
    let TriggerCondition::Event {
        pattern: EventPattern::Move { binding, predicate },
    } = &ability.trigger.as_ref()?.on
    else {
        return None;
    };
    ExDepartureFilter::parse(*binding, predicate)
}
pub(super) fn check_header(ability: &Ability) -> Result<bool> {
    if filter(ability).is_none() {
        return Ok(false);
    }
    let trigger = ability
        .trigger
        .as_ref()
        .ok_or_else(|| invalid("missing EX departure header"))?;
    if ability.kind != AbilityKind::Triggered
        || !optional_payment::trigger_play(ability)
        || trigger.active_zones != [ZoneKind::Field]
        || trigger.view != EventSnapshotPolicy::BeforeExit
        || !matches!(
            trigger.aggregation,
            TriggerAggregation::PerFact {}
                | TriggerAggregation::PerBatch {
                    key: BatchPolicy::Instruction {}
                }
        )
        || trigger
            .limit
            .is_some_and(|limit| !LimitSpec::is_triggered(limit))
    {
        return Err(unsupported("EX departure header"));
    }
    Ok(true)
}
struct Observer {
    pending: super::PendingOccurrence,
}
pub(in crate::game) struct ExDeparturePlan {
    observers: Vec<Observer>,
    active: PlayerId,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "EX batches share the common movement transaction."
)]
impl Game {
    pub(in crate::game) fn internal_before_ex_departures(
        &mut self,
        ids: &[String],
        before: SnapshotId,
        frame: &Frame,
    ) -> Result<Option<ExDeparturePlan>> {
        if !ids.iter().any(|id| {
            self.state
                .objects
                .get(id)
                .is_some_and(|object| object.zone == "ex")
        }) {
            return Ok(None);
        }
        let mut observers = Vec::new();
        let active = player_id(self.active())?;
        let first = ids
            .iter()
            .find(|id| {
                self.state
                    .objects
                    .get(*id)
                    .is_some_and(|object| object.zone == "ex")
            })
            .ok_or_else(|| invalid("missing proposed EX departure"))?;
        for holder in self.field_ids() {
            for ability in self.internal_printed_abilities(&holder)? {
                if filter(&ability.node).is_none() {
                    continue;
                }
                let controller = player_id(&self.object(&holder)?.controller)?;
                let source = self.internal_card_source(&holder, ability.node.id, controller)?;
                let mut information = SourceInformation::current(self, source)?;
                information.seal(Some(self.information_snapshot(source.holder)?.at(before)));
                let header = self.movement_header(first, before, frame, active)?;
                let pending =
                    self.new_pending(&holder, source, ability, information, header, None)?;
                observers.push(Observer { pending });
            }
        }
        Ok(Some(ExDeparturePlan {
            observers,
            active: player_id(self.active())?,
        }))
    }

    pub(in crate::game) fn internal_ex_departure_events(
        &mut self,
        plan: Option<ExDeparturePlan>,
        moved: &[(Object, String, String)],
        before: SnapshotId,
        frame: &Frame,
    ) -> Result<()> {
        let Some(plan) = plan else {
            return Ok(());
        };
        let mut pending = Vec::new();
        for observer in plan.observers {
            let filter = filter(&observer.pending.ability.node)
                .ok_or_else(|| invalid("missing EX batch filter"))?;
            let matching = moved
                .iter()
                .filter(|(old, to, controller)| {
                    old.zone == "ex"
                        && (to != "ex" || *controller != old.controller)
                        && player_id(&old.controller).is_ok_and(|owner| {
                            filter.matches(owner, observer.pending.source.controller, plan.active)
                        })
                        && filter.to.is_none_or(|kind| {
                            matches!(
                                (kind, to.as_str()),
                                (ZoneKind::Cemetery, "cemetery") | (ZoneKind::Hand, "hand")
                            )
                        })
                })
                .collect::<Vec<_>>();
            let count = u64::try_from(matching.len()).map_err(invalid)?;
            let batch = observer
                .pending
                .ability
                .node
                .trigger
                .as_ref()
                .is_some_and(|trigger| {
                    matches!(trigger.aggregation, TriggerAggregation::PerBatch { .. })
                });
            for (old, to, controller) in
                matching
                    .into_iter()
                    .take(if batch { 1 } else { usize::MAX })
            {
                let after = self.internal_current_reference(&old.id)?;
                let kind = match to.as_str() {
                    "field" => ZoneKind::Field,
                    "hand" => ZoneKind::Hand,
                    "deck" => ZoneKind::Deck,
                    "cemetery" => ZoneKind::Cemetery,
                    "banish" => ZoneKind::Banish,
                    "resolution" => ZoneKind::Resolution,
                    "ex" => ZoneKind::Ex,
                    "evolve_deck" => ZoneKind::Evolve,
                    _ => return Err(invalid("unknown EX departure destination")),
                };
                let event = MoveEvent {
                    before: ObjectRef {
                        id: after.id,
                        generation: Generation(old.generation),
                    },
                    after,
                    from: RuntimeZoneRef {
                        player: player_id(&old.controller)?,
                        kind: ZoneKind::Ex,
                    },
                    to: RuntimeZoneRef {
                        player: player_id(controller)?,
                        kind,
                    },
                    position: Position::Top {},
                    method: if frame.decision["do"] == "play" {
                        MoveMethod::Play
                    } else {
                        MoveMethod::Put
                    },
                    entered_from: None,
                    batch_size: Nat(if batch { count } else { 1 }),
                };
                let header = self.movement_header(&old.id, before, frame, plan.active)?;
                let mut occurrence = observer.pending.clone();
                self.card_runtime.next_pending = self
                    .card_runtime
                    .next_pending
                    .checked_add(1)
                    .ok_or_else(|| invalid("EX occurrence identities exhausted"))?;
                occurrence.id = self.card_runtime.next_pending;
                occurrence.header = header;
                occurrence.timing = Some(EventPayload::Move { event });
                pending.push(occurrence);
            }
        }
        self.internal_admit_pending(pending)
    }
}
