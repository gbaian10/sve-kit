//! A hand source is captured before the banish instruction commits its new generation.
use super::{
    Ability, AbilityKind, AbilityOrigin, Cause, EventPayload, EventSnapshotPolicy, Game,
    PendingOccurrence, Result, SnapshotId, SourceInformation, TriggerAggregation, ZoneKind,
    invalid, optional_payment, player_id, unsupported,
};
use crate::game::internal::Execution;
use crate::game::internal::admission::PlannedAbility;
use crate::ir::events::banish as header;
use crate::ir::events::subscriptions;
use crate::ir::{BanishEvent, MoveMethod, MoveResult, RuntimeZoneRef};

pub(in crate::game::internal) fn hand_header(ability: &Ability) -> bool {
    ability.active_zones == [ZoneKind::Hand]
        && ability
            .trigger
            .as_ref()
            .and_then(header::self_event)
            .is_some()
}

pub(super) fn check_header(ability: &Ability) -> Result<bool> {
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
}
