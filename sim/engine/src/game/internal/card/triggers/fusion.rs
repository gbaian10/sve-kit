//! Fusion observers run at resolution, using material traits captured before payment.
use super::{
    Ability, AbilityKind, Cause, EventPayload, EventSnapshotPolicy, Game, PendingOccurrence,
    Result, SnapshotId, SourceInformation, TriggerAggregation, ZoneKind, invalid, player_id,
    unsupported,
};
use crate::game::internal::conditions::subject as subject_condition;
use crate::game::internal::{Execution, Fact};
use crate::ir::events::fusion::{combined, filter, material_filter};
use crate::ir::events::subscriptions;
use crate::ir::optional_payment;
use crate::ir::{FusedEvent, LimitSpec, ObjectRef, TriggerCondition};
use core::mem;

pub(super) fn check_header(ability: &Ability) -> Result<bool> {
    if filter(ability).is_none() && material_filter(ability).is_none() {
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
        || trigger.active_zones != ability.active_zones
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
    pub(in crate::game::internal) fn internal_prepare_fused_materials(
        &mut self,
        execution: &mut Execution,
        materials: &[ObjectRef],
    ) -> Result<()> {
        let mut candidates = Vec::new();
        for &material in materials {
            let holder = execution.object_id(material)?;
            let (card, _) = self.internal_effective_print(holder)?;
            if !self.card_runtime.is_loaded(&card) {
                continue;
            }
            for ability in self.internal_printed_abilities(holder)? {
                if let Some(filter) = material_filter(&ability.node) {
                    candidates.push((holder.to_owned(), material, ability, filter));
                }
            }
        }
        if candidates.is_empty() {
            return Ok(());
        }
        let cause = execution.source_snapshot(self)?;
        let mut matched = Vec::new();
        for candidate in candidates {
            if subject_condition::source(&cause.subject, candidate.3.kind, &candidate.3.trait_id)? {
                matched.push(candidate);
            }
        }
        if matched.is_empty() {
            return Ok(());
        }
        let actor = execution.context.controller;
        let header = self.pending_header(
            actor,
            Cause::Effect {
                execution: execution.id,
            },
            execution.ability,
            SnapshotId(self.state.ir_epoch),
        )?;
        for (holder, material, ability, _) in matched {
            let source = self.internal_card_source(&holder, ability.node.id, actor)?;
            let mut information = SourceInformation::current(self, source)?;
            information.seal(Some(self.information_snapshot(material)?.at(header.before)));
            execution.fusion_material_pending.push(self.new_pending(
                &holder,
                source,
                ability,
                information,
                header.clone(),
                None,
            )?);
        }
        Ok(())
    }

    pub(in crate::game::internal) fn internal_fused_events(
        &mut self,
        execution: &mut Execution,
    ) -> Result<()> {
        for fact in &execution.facts {
            if let Fact::Fused(event) = fact {
                let candidates = mem::take(&mut execution.fusion_material_pending);
                self.fused_event(execution, event, candidates)?;
            }
        }
        Ok(())
    }

    fn fused_event(
        &mut self,
        execution: &Execution,
        event: &FusedEvent,
        mut candidates: Vec<PendingOccurrence>,
    ) -> Result<()> {
        for occurrence in &mut candidates {
            if !event
                .materials
                .iter()
                .any(|member| member.object == occurrence.source.holder)
            {
                return Err(invalid("Fusion material occurrence without a paid member"));
            }
            occurrence.header.after = SnapshotId(self.state.ir_epoch);
            occurrence.timing = Some(EventPayload::Fused {
                event: event.clone(),
            });
        }
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
                .any(|ability| filter(&ability.node).is_some())
        }) {
            return if candidates.is_empty() {
                Ok(())
            } else {
                self.internal_admit_pending(candidates)
            };
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
            if !subject_condition::controlled_by(
                event.controller,
                player_id(&self.object(&holder)?.controller)?,
            )? {
                continue;
            }
            for ability in self.internal_effective_abilities(&holder)? {
                if ability.node.active_zones == [zone]
                    && let Some(trait_id) = filter(&ability.node)
                    && subject_condition::traits(&event.material_traits, &trait_id)?
                {
                    matched.push((holder.clone(), ability));
                }
            }
        }
        if matched.is_empty() {
            return if candidates.is_empty() {
                Ok(())
            } else {
                self.internal_admit_pending(candidates)
            };
        }
        let header = self.pending_header(
            event.controller,
            Cause::Effect {
                execution: execution.id,
            },
            execution.ability,
            SnapshotId(self.state.ir_epoch),
        )?;
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
