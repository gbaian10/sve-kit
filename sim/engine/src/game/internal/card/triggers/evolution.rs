//! Evolution facts are captured around the actual rule procedure, after its effective print changes.
use super::{Cause, Game, Result, SnapshotId, player_id, unsupported};
use crate::game::Frame;
use crate::ir::{
    Characteristics, EventPattern, EventPayload, EvolutionState, EvolveEvent, ExecutionId,
    TriggerCondition,
};

pub(in crate::game) struct EvolutionPlan {
    before: Characteristics,
    at: SnapshotId,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Evolution shares the authority's rule commit boundary."
)]
impl Game {
    pub(in crate::game) fn internal_before_evolution(
        &mut self,
        holder: &str,
        form: &str,
        face: usize,
    ) -> Result<Option<EvolutionPlan>> {
        let form = self.object(form)?;
        let interested = self.card_runtime.cards.get(&form.card).is_some_and(|program| program.card().abilities.iter().any(|a| {
            matches!(&a.node.trigger, Some(trigger) if matches!(&trigger.on, TriggerCondition::Event {pattern:EventPattern::Evolve {..}}))
                && super::printed_origin(&a.node.source.origin).is_ok_and(|(printed,_)| super::face_ordinal(printed).is_ok_and(|ordinal| ordinal==face))
        }));
        if !interested {
            return Ok(None);
        }
        Ok(Some(EvolutionPlan {
            before: self.internal_characteristics(holder)?,
            at: SnapshotId(self.state.ir_epoch),
        }))
    }

    pub(in crate::game) fn internal_evolve_event(
        &mut self,
        holder: &str,
        form: &str,
        super_evolved: bool,
        plan: Option<EvolutionPlan>,
        frame: &Frame,
    ) -> Result<()> {
        let Some(plan) = plan else { return Ok(()) };
        let abilities = self.internal_printed_abilities(holder)?.into_iter().filter(|a| {
            matches!(&a.node.trigger, Some(trigger) if matches!(&trigger.on, TriggerCondition::Event {pattern:EventPattern::Evolve {..}}))
        }).collect::<Vec<_>>();
        let actor = player_id(&self.object(holder)?.controller)?;
        let cause = if let Some(execution) = frame.cause["effect"]["execution"].as_u64() {
            Cause::Effect {
                execution: ExecutionId(execution),
            }
        } else if frame.decision["do"] == "evolve" {
            Cause::PlayerAction { player: actor }
        } else {
            return Err(unsupported("evolution without an admitted authority cause"));
        };
        let event = EvolveEvent {
            subject: self.internal_current_reference(holder)?,
            with: self.internal_current_reference(form)?,
            kind: if super_evolved {
                EvolutionState::SuperEvolved
            } else {
                EvolutionState::Evolved
            },
            before: plan.before,
            after: self.internal_characteristics(holder)?,
        };
        let source = if matches!(cause, Cause::Effect { .. }) {
            self.card_runtime.active_source
        } else {
            None
        };
        let header = self.pending_header(actor, cause, source, plan.at)?;
        for ability in abilities {
            self.timing_pending(
                holder,
                ability,
                &header,
                EventPayload::Evolve {
                    event: Box::new(event.clone()),
                },
            )?;
        }
        Ok(())
    }
}
