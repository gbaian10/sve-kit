//! Evolution facts are captured around the actual rule procedure, after its effective print changes.
use super::{Cause, Game, Result, SnapshotId, player_id, unsupported};
use crate::game::Frame;
use crate::ir::events::subject::{SubjectFields, SubjectFilter};
use crate::ir::{Ability, AbilityOrigin, KeywordId, Located};
use crate::ir::{
    Characteristics, EventPattern, EventPayload, EvolutionState, EvolveEvent, ExecutionId,
    TriggerCondition,
};

pub(super) fn observer_filter(ability: &Ability) -> Option<SubjectFilter> {
    if !matches!(ability.source.origin, AbilityOrigin::Printed { .. }) {
        return None;
    }
    let TriggerCondition::Event {
        pattern: EventPattern::Evolve { binding, predicate },
    } = &ability.trigger.as_ref()?.on
    else {
        return None;
    };
    SubjectFilter::parse(SubjectFields::Evolve(*binding), predicate)
}

pub(in crate::game) struct EvolutionPlan {
    before: Characteristics,
    at: SnapshotId,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Evolution shares the authority's rule commit boundary."
)]
impl Game {
    fn evolution_observers(&self) -> Result<Vec<(String, Vec<Located<Ability>>)>> {
        self.field_ids()
            .into_iter()
            .map(|id| {
                let abilities = self
                    .internal_printed_abilities(&id)?
                    .into_iter()
                    .filter(|ability| {
                        matches!(
                            ability.node.trigger.as_ref().map(|trigger| &trigger.on),
                            Some(TriggerCondition::Event {
                                pattern: EventPattern::Evolve { .. }
                            })
                        )
                    })
                    .collect();
                Ok((id, abilities))
            })
            .collect()
    }

    pub(in crate::game) fn internal_before_evolution(
        &mut self,
        holder: &str,
        form: &str,
        face: usize,
    ) -> Result<Option<EvolutionPlan>> {
        let form = self.object(form)?;
        let interested = self.card_runtime.definitions.cards.get(&form.card).is_some_and(|program| program.card().abilities.iter().any(|a| {
            matches!(&a.node.trigger, Some(trigger) if matches!(&trigger.on, TriggerCondition::Event {pattern:EventPattern::Evolve {..}}))
                && super::printed_origin(&a.node.source.origin).is_ok_and(|(printed,_)| self.printed_face_ordinal(&form.card, printed).is_ok_and(|ordinal| ordinal==face))
        }));
        let external = self.card_runtime.definitions.cards.values().any(|program| {
            program
                .card()
                .abilities
                .iter()
                .any(|ability| observer_filter(&ability.node).is_some())
        });
        let interested = interested
            || (external
                && self.evolution_observers()?.iter().any(|(_, abilities)| {
                    abilities
                        .iter()
                        .any(|ability| observer_filter(&ability.node).is_some())
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
        original_cause: &serde_json::Value,
        frame: &Frame,
    ) -> Result<()> {
        let Some(plan) = plan else { return Ok(()) };
        let observers = self.evolution_observers()?;
        let subject_controller = player_id(&self.object(holder)?.controller)?;
        let cause = if let Some(execution) = original_cause["effect"]["execution"].as_u64() {
            Cause::Effect {
                execution: ExecutionId(execution),
            }
        } else if frame.decision["do"] == "evolve" {
            Cause::PlayerAction {
                player: subject_controller,
            }
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
        let header = self.pending_header(subject_controller, cause, source, plan.at)?;
        let mut pending = Vec::new();
        let active = player_id(self.active())?;
        for (observer, abilities) in observers {
            let controller = player_id(&self.object(&observer)?.controller)?;
            for ability in abilities {
                let eligible = if let Some(filter) = observer_filter(&ability.node) {
                    filter.matches(
                        &event.after,
                        observer == holder,
                        controller,
                        subject_controller,
                        active,
                        false,
                    )
                } else {
                    observer == holder
                        && (super_evolved
                            || !matches!(
                                ability.node.source.origin,
                                AbilityOrigin::Keyword {
                                    keyword: KeywordId::OnSuperEvolve,
                                    ..
                                }
                            ))
                };
                if !eligible {
                    continue;
                }
                pending.push(self.timing_occurrence(
                    &observer,
                    ability,
                    &header,
                    EventPayload::Evolve {
                        event: Box::new(event.clone()),
                    },
                )?);
            }
        }
        self.internal_admit_pending(pending)
    }
}
