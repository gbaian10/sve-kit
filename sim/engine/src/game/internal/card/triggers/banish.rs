//! Banish observers are captured before the batch commits each card's new generation.
use super::{
    Ability, AbilityKind, AbilityOrigin, Cause, EventPayload, EventSnapshotPolicy, Game,
    PendingOccurrence, Result, SnapshotId, SourceInformation, TriggerAggregation, ZoneKind,
    invalid, optional_payment, player_id, unsupported,
};
use crate::game::Frame;
use crate::game::internal::Execution;
use crate::game::internal::admission::PlannedAbility;
use crate::ir::events::banish as header;
use crate::ir::{BanishEvent, MoveMethod, MoveResult, RuntimeZoneRef};
use alloc::collections::BTreeMap;

pub(in crate::game::internal) fn hand_header(ability: &Ability) -> bool {
    ability.active_zones == [ZoneKind::Hand]
        && ability
            .trigger
            .as_ref()
            .and_then(header::self_event)
            .is_some()
}

/// 「自分の消滅領域にカードが置かれたとき」 observed from the field.
fn zone_header(ability: &Ability) -> Option<header::ZonePlan> {
    if ability.active_zones != [ZoneKind::Field]
        || ability.kind != AbilityKind::Triggered
        || !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
    {
        return None;
    }
    ability.trigger.as_ref().and_then(header::zone_plan)
}

pub(super) fn check_header(ability: &Ability) -> Result<bool> {
    if zone_header(ability).is_some() {
        if !optional_payment::trigger_play(ability)
            || ability
                .trigger
                .as_ref()
                .is_some_and(|trigger| trigger.limit.is_some())
        {
            return Err(unsupported("printed banish zone header"));
        }
        return Ok(true);
    }
    if !hand_header(ability) {
        return Ok(false);
    }
    let trigger = ability
        .trigger
        .as_ref()
        .ok_or_else(|| invalid("missing banish trigger"))?;
    if ability.kind != AbilityKind::Triggered
        || !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        || !optional_payment::trigger_play(ability)
        || trigger.active_zones != ability.active_zones
        || trigger.view != EventSnapshotPolicy::BeforeExit
        || trigger.aggregation != (TriggerAggregation::PerFact {})
        || trigger.limit.is_some()
    {
        return Err(unsupported("printed self hand banish header"));
    }
    Ok(true)
}

pub(in crate::game::internal) struct BanishPlan(Vec<(BanishEvent, PendingOccurrence)>);

/// Occurrences captured before the batch moves, keyed by the card that must reach banish.
pub(in crate::game) struct BanishZonePlan(Vec<(String, PendingOccurrence)>);

fn origin_zone(zone: &str) -> Option<ZoneKind> {
    Some(match zone {
        "field" => ZoneKind::Field,
        "hand" => ZoneKind::Hand,
        "deck" => ZoneKind::Deck,
        "cemetery" => ZoneKind::Cemetery,
        "ex" => ZoneKind::Ex,
        "evolve_deck" => ZoneKind::Evolve,
        "resolution" => ZoneKind::Resolution,
        _ => return None,
    })
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Banish uses the existing movement transaction and pending queue."
)]
impl Game {
    pub(in crate::game::internal) fn internal_prepare_banish_events(
        &mut self,
        members: &[String],
        execution: &Execution,
        method: MoveMethod,
    ) -> Result<Option<BanishPlan>> {
        if method != MoveMethod::Banish {
            return Ok(None);
        }
        let mut captured = Vec::new();
        if !self.card_runtime.definitions.cards.values().any(|program| {
            program
                .card()
                .abilities
                .iter()
                .any(|ability| hand_header(&ability.node))
        }) {
            return Ok(Some(BanishPlan(captured)));
        }
        let before = SnapshotId(self.state.ir_epoch);
        for id in members {
            if self.object(id)?.zone != "hand" {
                continue;
            }
            let abilities = self
                .internal_printed_abilities(id)?
                .into_iter()
                .filter(|ability| hand_header(&ability.node))
                .collect::<Vec<PlannedAbility>>();
            if abilities.is_empty() {
                continue;
            }
            let controller = player_id(&self.object(id)?.controller)?;
            let event = BanishEvent {
                card_before: self.internal_current_reference(id)?,
                from: RuntimeZoneRef {
                    player: controller,
                    kind: ZoneKind::Hand,
                },
                actor: execution.context.controller,
                owner: player_id(&self.object(id)?.owner)?,
                characters: self.internal_hand_characteristics(id)?,
            };
            let envelope = self.pending_header(
                execution.context.controller,
                Cause::Effect {
                    execution: execution.id,
                },
                execution.ability,
                before,
            )?;
            for ability in abilities {
                let source = self.internal_card_source(id, ability.node.id, controller)?;
                let mut information = SourceInformation::current(self, source)?;
                information.seal(Some(
                    self.information_snapshot(source.holder)?
                        .at(envelope.before),
                ));
                let pending =
                    self.new_pending(id, source, ability, information, envelope.clone(), None)?;
                captured.push((event.clone(), pending));
            }
        }
        Ok(Some(BanishPlan(captured)))
    }

    pub(in crate::game::internal) fn internal_commit_banish_events(
        &mut self,
        plan: BanishPlan,
        moves: &[MoveResult],
    ) -> Result<()> {
        let mut pending = Vec::new();
        for (event, mut occurrence) in plan.0 {
            let Some(movement) = moves
                .iter()
                .find(|movement| movement.before == event.card_before && movement.after.is_some())
            else {
                continue;
            };
            occurrence.header.after = SnapshotId(self.state.ir_epoch);
            let current = self.object(&occurrence.holder)?;
            let owner = player_id(&current.owner)?;
            occurrence.movement_after = movement.after.filter(|after| {
                current.zone == "banish"
                    && current.owner == current.controller
                    && current.generation == after.generation.0
                    && owner == occurrence.source.controller
                    && movement.actual_to
                        == Some(RuntimeZoneRef {
                            player: owner,
                            kind: ZoneKind::Banish,
                        })
            });
            occurrence.timing = Some(EventPayload::Banish { event });
            pending.push(occurrence);
        }
        self.internal_admit_pending(pending)
    }

    /// Field observers, including one leaving in this batch (Q2816), see each arrival (Q2813).
    pub(in crate::game) fn internal_before_banish_zone(
        &mut self,
        ids: &[String],
        destinations: &BTreeMap<String, String>,
        frame: &Frame,
    ) -> Result<BanishZonePlan> {
        let mut captured = Vec::new();
        let arriving = ids
            .iter()
            .filter(|id| destinations.get(*id).is_some_and(|zone| zone == "banish"))
            .cloned()
            .collect::<Vec<_>>();
        if arriving.is_empty()
            || !self.card_runtime.definitions.cards.values().any(|program| {
                program
                    .card()
                    .abilities
                    .iter()
                    .any(|ability| zone_header(&ability.node).is_some())
            })
        {
            return Ok(BanishZonePlan(captured));
        }
        let mut observers = Vec::new();
        for holder in self.field_ids() {
            let abilities = self
                .internal_printed_abilities(&holder)?
                .into_iter()
                .filter_map(|ability| zone_header(&ability.node).map(|plan| (plan, ability)))
                .collect::<Vec<_>>();
            if !abilities.is_empty() {
                observers.push((holder, abilities));
            }
        }
        if observers.is_empty() {
            return Ok(BanishZonePlan(captured));
        }
        let before = SnapshotId(self.state.ir_epoch);
        let active = player_id(self.active())?;
        let actor = self
            .card_runtime
            .active_source
            .map_or(active, |source| source.controller);
        for id in arriving {
            let object = self.object(&id)?.clone();
            let Some(from) = origin_zone(&object.zone) else {
                continue;
            };
            let owner = player_id(&object.owner)?;
            let event = BanishEvent {
                card_before: self.internal_current_reference(&id)?,
                from: RuntimeZoneRef {
                    player: player_id(&object.controller)?,
                    kind: from,
                },
                actor,
                owner,
                characters: if from == ZoneKind::Hand {
                    self.internal_hand_characteristics(&id)?
                } else {
                    self.internal_characteristics(&id)?
                },
            };
            let header = self.movement_header(&id, before, frame, active)?;
            for (holder, abilities) in &observers {
                let controller = player_id(&self.object(holder)?.controller)?;
                for (plan, ability) in abilities {
                    if plan.own != (owner == controller)
                        || (plan.own_turn && active != controller)
                        || self.internal_trigger_forbidden(&ability.node, controller)?
                    {
                        continue;
                    }
                    let mut pending = self.timing_occurrence(
                        holder,
                        ability.clone(),
                        &header,
                        EventPayload::Banish {
                            event: event.clone(),
                        },
                    )?;
                    // An observer leaving in this batch resolves with its information from just
                    // before the move (Q2816); the shared departure update ran before this plan.
                    if destinations.get(holder).is_some_and(|zone| zone != "field") {
                        pending.information.seal(Some(
                            self.information_snapshot(pending.source.holder)?.at(before),
                        ));
                    }
                    captured.push((id.clone(), pending));
                }
            }
        }
        Ok(BanishZonePlan(captured))
    }

    /// Only cards that actually arrived in a banish zone keep their occurrences.
    pub(in crate::game) fn internal_commit_banish_zone(
        &mut self,
        plan: BanishZonePlan,
        arrived: &[String],
    ) -> Result<()> {
        let after = SnapshotId(self.state.ir_epoch);
        let pending = plan
            .0
            .into_iter()
            .filter(|(id, _)| arrived.contains(id))
            .map(|(_, mut occurrence)| {
                occurrence.header.after = after;
                occurrence
            })
            .collect::<Vec<_>>();
        // Every common movement reaches here; an empty commit must not disturb another
        // producer's quota choice from the same movement (for example a limited mill observer).
        if pending.is_empty() {
            return Ok(());
        }
        self.internal_admit_pending(pending)
    }
}
