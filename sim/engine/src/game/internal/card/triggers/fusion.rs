//! Fusion observers run at resolution, using material traits captured before payment.
use super::{
    Ability, AbilityKind, Cause, EventPayload, EventSnapshotPolicy, Game, Result, SnapshotId,
    TriggerAggregation, ZoneKind, invalid, player_id, unsupported,
};
use crate::game::internal::{Execution, Fact};
use crate::ir::events::fusion::{combined, filter};
use crate::ir::optional_payment;
use crate::ir::{FusedEvent, LimitSpec, TriggerCondition};

pub(super) fn check_header(ability: &Ability) -> Result<bool> {
    if filter(ability).is_none() {
        return Ok(false);
    }
    let trigger = ability
        .trigger
        .as_ref()
        .ok_or_else(|| invalid("Fusion observer trigger"))?;
    let compound = matches!(&trigger.on, TriggerCondition::Event { pattern } if combined(pattern));
    if ability.kind != AbilityKind::Triggered
        || !optional_payment::trigger_play(ability)
        || trigger.view
            != if compound {
                EventSnapshotPolicy::AtReveal
            } else {
                EventSnapshotPolicy::BeforeExit
            }
        || trigger
            .limit
            .is_some_and(|limit| !LimitSpec::is_triggered(limit))
        || trigger.aggregation != (TriggerAggregation::PerFact {})
    {
        return Err(unsupported("per-Fusion material trait observer"));
    }
    Ok(true)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Fusion resolution shares pending observer admission."
)]
impl Game {
    pub(in crate::game::internal) fn internal_fused_events(
        &mut self,
        execution: &Execution,
    ) -> Result<()> {
        for fact in &execution.facts {
            if let Fact::Fused(event) = fact {
                self.fused_event(execution, event)?;
            }
        }
        Ok(())
    }

    fn fused_event(&mut self, execution: &Execution, event: &FusedEvent) -> Result<()> {
        if !self.card_runtime.definitions.cards.values().any(|program| {
            program
                .card()
                .abilities
                .iter()
                .any(|ability| filter(&ability.node).is_some())
        }) {
            return Ok(());
        }
        let mut observers = self.field_ids();
        observers.extend(self.zone_ids("P1", "ex"));
        observers.extend(self.zone_ids("P2", "ex"));
        let mut matched = Vec::new();
        for holder in observers {
            let zone = if self.object(&holder)?.zone == "field" {
                ZoneKind::Field
            } else {
                ZoneKind::Ex
            };
            let (printing, _) = self.internal_effective_print(&holder)?;
            if !self.card_runtime.is_loaded(&printing) {
                if zone == ZoneKind::Field {
                    self.require_internal_abilities(&holder, "field Fusion observer")?;
                }
                continue;
            }
            if player_id(&self.object(&holder)?.controller)? != event.controller {
                continue;
            }
            for ability in self.internal_effective_abilities(&holder)? {
                if ability.node.active_zones == [zone]
                    && filter(&ability.node)
                        .is_some_and(|trait_id| event.material_traits.contains(&trait_id))
                {
                    matched.push((holder.clone(), ability));
                }
            }
        }
        if matched.is_empty() {
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
        let mut candidates = Vec::new();
        for (holder, ability) in matched {
            candidates.push(self.timing_occurrence(
                &holder,
                ability,
                &header,
                EventPayload::Fused {
                    event: event.clone(),
                },
            )?);
        }
        self.internal_admit_pending(candidates)
    }
}
