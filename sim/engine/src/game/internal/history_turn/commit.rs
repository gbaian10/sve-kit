//! Each caller commits its own occurrence at its established transaction point.

use super::{
    AbilityPlayHistoryFact, DamageHistoryFact, DiceHistoryFact, Game, Nat, ObjectLink, Result,
    StatGainHistoryFact, SubjectHistoryEvent, SubjectHistoryFact, invalid, seat_name, unsupported,
};
use crate::game::Object;
use crate::game::internal::{DamageOccurrence, player_id};
use crate::game::string;
use crate::ir::{
    CardKind, CardName, DamageEvent, EventPayload, ExecutionId, PlayerId, RuntimeDamageRecipient,
    RuntimeHolder, Stat,
};
use alloc::collections::BTreeSet;
use core::iter::once;
use serde_json::{Value, json};

impl Game {
    pub(in crate::game::internal) fn internal_record_stat_gains(
        &mut self,
        events: &[EventPayload],
    ) {
        let recorded = events
            .iter()
            .filter_map(|payload| {
                let EventPayload::StatChange { event } = payload else {
                    return None;
                };
                let RuntimeHolder::Object { object } = event.subject else {
                    return None;
                };
                let positive = |property| {
                    once(&event.facets.first)
                        .chain(&event.facets.rest)
                        .any(|facet| facet.property == property && facet.delta.0 > 0)
                };
                let attack = positive(Stat::Attack);
                let life = positive(Stat::Life);
                (attack || life).then(|| StatGainHistoryFact {
                    link: ObjectLink::reference(self, object.id.0, object.generation.0),
                    object: object.id.0,
                    generation: object.generation.0,
                    attack,
                    life,
                })
            })
            .collect::<Vec<_>>();
        self.state.ir_stat_gain_history.extend(recorded);
    }

    pub(in crate::game) fn internal_commit_card_play(
        &mut self,
        source: &str,
        controller: &str,
    ) -> Result<()> {
        let fact = if self.card_runtime.is_empty() {
            None
        } else {
            // Later ability loss must not rewrite the names recorded for this play.
            let names = self.card_names(source)?.into_iter().map(CardName).collect();
            let information = self.public_characteristics(source, names)?;
            let kind = match information.kind {
                Some(CardKind::Follower) => "follower",
                Some(CardKind::Spell) => "spell",
                Some(CardKind::Amulet) => "amulet",
                _ => return Err(unsupported("played-card history kind")),
            };
            let object = self.internal_current_reference(source)?;
            Some(SubjectHistoryFact {
                link: ObjectLink::capture(self, source, object.generation.0),
                event: SubjectHistoryEvent::Play,
                destination: None,
                object: object.id.0,
                generation: object.generation.0,
                controller: player_id(controller)?.seat,
                active: player_id(self.active())?.seat,
                kind: kind.into(),
                class: String::new(),
                traits: information.traits.into_iter().map(|id| id.0).collect(),
                names: Some(information.names.into_iter().map(|name| name.0).collect()),
                token: information.token,
                destroyed: false,
            })
        };
        self.bump(&format!("{controller}.cards_played"), 1);
        if let Some(fact) = fact {
            self.state.ir_subject_history.push(fact);
        }
        Ok(())
    }

    pub(in crate::game) fn internal_capture_damage_history(
        &self,
        hits: &[Value],
        facts: Option<&[DamageOccurrence]>,
        battle_events: Option<&[DamageEvent]>,
    ) -> Result<Vec<(DamageEvent, Vec<String>)>> {
        if let Some(facts) = facts {
            return Ok(facts
                .iter()
                .map(|fact| {
                    (
                        fact.event,
                        fact.source
                            .subject
                            .traits
                            .iter()
                            .map(|id| id.0.clone())
                            .collect(),
                    )
                })
                .collect());
        }
        battle_events
            .into_iter()
            .flatten()
            .zip(hits)
            .map(|(event, hit)| {
                // Freeze traits before damage can change the source's characteristics.
                let traits = self
                    .card_traits(string(&hit["source"]))?
                    .into_iter()
                    .collect();
                Ok((*event, traits))
            })
            .collect()
    }

    pub(in crate::game) fn internal_record_damage_history(
        &mut self,
        events: impl IntoIterator<Item = (DamageEvent, Vec<String>)>,
    ) {
        for (event, traits) in events {
            if let RuntimeDamageRecipient::Object { object } = event.recipient {
                self.state.ir_damage_history.push(DamageHistoryFact {
                    recipient_link: ObjectLink::reference(self, object.id.0, object.generation.0),
                    source_link: ObjectLink::reference(
                        self,
                        event.source.object.id.0,
                        event.source.object.generation.0,
                    ),
                    recipient: (object.id.0, object.generation.0),
                    source: (event.source.object.id.0, event.source.object.generation.0),
                    controller: event.controller.seat,
                    traits,
                });
            }
        }
    }

    pub(in crate::game) fn internal_commit_subject_history(
        &mut self,
        observations: Vec<SubjectHistoryFact>,
    ) -> Result<()> {
        for fact in observations {
            let current = self
                .state
                .ir_objects
                .iter()
                .find(|(_, number)| **number == fact.object)
                .and_then(|(id, _)| self.state.objects.get(id))
                .ok_or_else(|| invalid("missing committed history subject"))?;
            if current.generation != fact.generation {
                self.state.ir_subject_history.push(fact);
            }
        }
        Ok(())
    }

    pub(in crate::game) fn internal_record_subject_event(
        &mut self,
        source: &str,
        event: SubjectHistoryEvent,
    ) -> Result<()> {
        if !self.card_runtime.is_empty() {
            let observation = self.internal_subject_history(source, event, None, false)?;
            self.state.ir_subject_history.push(observation);
        }
        Ok(())
    }

    pub(in crate::game::internal) fn internal_commit_union_burst(
        &mut self,
        execution: ExecutionId,
        controller: PlayerId,
    ) -> Result<()> {
        self.bump(&format!("{}.ub_activated", seat_name(controller)?), 1);
        self.state.ir_ub_history.push(AbilityPlayHistoryFact {
            execution: execution.0,
            controller: controller.seat,
        });
        Ok(())
    }

    pub(in crate::game::internal) fn internal_record_dice_history(
        &mut self,
        player: PlayerId,
        value: Nat,
    ) -> Result<()> {
        let turn = self.state.schedule.current()?;
        self.state.ir_dice_history.retain(|fact| fact.turn == turn);
        self.state.ir_dice_history.push(DiceHistoryFact {
            turn,
            player: player.seat,
            value: value.0,
        });
        Ok(())
    }

    pub(in crate::game) fn internal_record_life_history(
        &mut self,
        seat: &str,
        before: i64,
        after: i64,
    ) {
        if after < before {
            self.bump(&format!("{seat}.leader_hp_decreased"), 1);
        }
        if after > before {
            self.bump(&format!("{seat}.leader_hp_increase_count"), 1);
            self.bump(
                &format!("{seat}.leader_hp_increased"),
                after.saturating_sub(before),
            );
        }
    }

    pub(in crate::game::internal) fn internal_record_quick_activation(
        &mut self,
        execution: ExecutionId,
        controller: PlayerId,
    ) {
        self.state
            .ir_quick_activation_history
            .push(AbilityPlayHistoryFact {
                execution: execution.0,
                controller: controller.seat,
            });
    }

    pub(in crate::game) fn internal_record_discard_history(&mut self, controller: &str) {
        self.bump(&format!("{controller}.discarded"), 1);
    }

    pub(in crate::game) fn internal_record_discard_traits(
        &mut self,
        controller: &str,
        traits: BTreeSet<String>,
    ) {
        if traits.is_empty() {
            return;
        }
        self.state
            .ir_discard_traits
            .entry(controller.into())
            .or_default()
            .push(traits);
    }

    pub(in crate::game) fn internal_record_lesson_history(
        &mut self,
        id: &str,
        previous: &Object,
        destination: &str,
    ) -> Result<()> {
        if previous.zone == "ex"
            && destination == "banish"
            && self.catalog.rule_bindings.has_resource("lesson_item")
            && self.single_rules_name_matches_role(id, "lesson_item")?
        {
            self.bump(&format!("{}.magic_item_banished", previous.controller), 1);
        }
        Ok(())
    }

    pub(in crate::game) fn internal_reset_turn_counters_and_facts(&mut self) {
        for value in self.state.counters.values_mut() {
            *value = 0;
        }
        self.state.ir_subject_history.clear();
        self.state.ir_discard_traits.clear();
        self.state.ir_damage_history.clear();
        self.state.ir_stat_gain_history.clear();
        self.state.ir_ub_history.clear();
        self.state.ir_quick_activation_history.clear();
    }

    #[expect(
        clippy::indexing_slicing,
        reason = "Turn marks are written on validated object state."
    )]
    pub(in crate::game) fn internal_reset_object_turn_marks(&mut self) {
        for object in self.state.objects.values_mut() {
            object.state["stats_increased_this_turn"] = json!(false);
            if let Some(state) = object.state.as_object_mut() {
                state.remove("damaged_this_turn");
                state.remove("control_changed_this_turn");
                state.remove("evolved_after_control_change");
            }
            if object.state.get("attacks_this_turn").is_some() {
                object.state["attacks_this_turn"] = json!(0_i64);
            }
        }
    }
}
