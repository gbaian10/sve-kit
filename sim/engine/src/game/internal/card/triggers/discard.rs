//! A hand source leaves its generation before observers run, so its authority is captured first.
use super::{
    Ability, AbilityKind, AbilityOrigin, Cause, EventPattern, EventPayload, EventSnapshotPolicy,
    Frame, Game, LimitSpec, Located, PendingOccurrence, Result, RuntimeAbilitySource, SnapshotId,
    SourceInformation, TriggerAggregation, TriggerCondition, ZoneKind, invalid, optional_payment,
    player_id, unsupported,
};
use crate::ir::events::discard::DiscardFilter;
use crate::ir::{Characteristics, DiscardEvent, ObjectRef, PlayerId, RuntimeZoneRef};

fn filter(ability: &Ability) -> Option<DiscardFilter> {
    if !matches!(ability.source.origin, AbilityOrigin::Printed { .. }) {
        return None;
    }
    let TriggerCondition::Event {
        pattern: EventPattern::Discard { binding, predicate },
    } = &ability.trigger.as_ref()?.on
    else {
        return None;
    };
    DiscardFilter::parse(*binding, predicate)
}

pub(in crate::game::internal) fn hand_header(ability: &Ability) -> bool {
    ability.active_zones == [ZoneKind::Hand] && filter(ability).is_some_and(|plan| plan.itself)
}

pub(super) fn check_header(ability: &Ability) -> Result<bool> {
    let Some(plan) = filter(ability) else {
        return Ok(false);
    };
    let trigger = ability
        .trigger
        .as_ref()
        .ok_or_else(|| invalid("missing discard trigger"))?;
    let zone = if plan.itself {
        ZoneKind::Hand
    } else {
        ZoneKind::Field
    };
    if ability.kind != AbilityKind::Triggered
        || !optional_payment::trigger_play(ability)
        || trigger.active_zones != ability.active_zones
        || trigger.active_zones != [zone]
        || trigger.view != EventSnapshotPolicy::BeforeExit
        || trigger.aggregation != (TriggerAggregation::PerFact {})
        || trigger
            .limit
            .is_some_and(|limit| !LimitSpec::is_triggered(limit))
    {
        return Err(unsupported("printed per-card hand discard header"));
    }
    Ok(true)
}

pub(in crate::game::internal) struct DiscardPlan(Vec<(ObjectRef, PendingOccurrence)>);

struct DiscardedCard {
    id: String,
    before: ObjectRef,
    player: PlayerId,
    characters: Characteristics,
}

pub(in crate::game) struct RuleDiscard {
    cards: Vec<DiscardedCard>,
    pending: DiscardPlan,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Discard shares the authoritative pending queue and movement transaction."
)]
impl Game {
    fn has_discard_headers(&self) -> bool {
        self.card_runtime.definitions.cards.values().any(|program| {
            program
                .card()
                .abilities
                .iter()
                .any(|ability| filter(&ability.node).is_some())
        })
    }

    pub(in crate::game) fn internal_prepare_rule_discard(
        &mut self,
        ids: &[String],
        frame: &Frame,
    ) -> Result<RuleDiscard> {
        let mut cards = Vec::new();
        if !self.has_discard_headers() {
            return Ok(RuleDiscard {
                cards,
                pending: DiscardPlan(Vec::new()),
            });
        }
        for id in ids {
            cards.push(DiscardedCard {
                id: id.clone(),
                before: self.internal_current_reference(id)?,
                player: player_id(&self.object(id)?.controller)?,
                characters: self.internal_hand_characteristics(id)?,
            });
        }
        let actor = player_id(&frame.controller)?;
        let rule = self.rule_id(
            frame.cause["rule"]
                .as_str()
                .ok_or_else(|| invalid("missing discard rule"))?,
        )?;
        let pending =
            self.internal_prepare_discard_events(ids, actor, Cause::Rule { rule }, None)?;
        Ok(RuleDiscard { cards, pending })
    }

    pub(in crate::game) fn internal_commit_rule_discard(
        &mut self,
        plan: RuleDiscard,
    ) -> Result<()> {
        let mut events = Vec::new();
        for card in plan.cards {
            let actual = self.object(&card.id)?;
            if actual.generation == card.before.generation.0 {
                continue;
            }
            events.push(DiscardEvent {
                card_before: card.before,
                player: card.player,
                characters: card.characters,
                actual_to: RuntimeZoneRef {
                    player: player_id(&actual.controller)?,
                    kind: super::departure_destination(&actual.zone)?,
                },
            });
        }
        self.internal_commit_discard_events(plan.pending, &events)
    }

    fn discard_observers(
        &mut self,
        id: &str,
        card_before: ObjectRef,
        player: PlayerId,
    ) -> Result<Vec<(String, Located<Ability>)>> {
        let mut observers = Vec::new();
        let mut holders = self.field_ids();
        holders.push(id.to_owned());
        for holder in holders {
            let (card, _) = self.internal_effective_print(&holder)?;
            if !self
                .card_runtime
                .definitions
                .cards
                .get(&card)
                .is_some_and(|program| {
                    program
                        .card()
                        .abilities
                        .iter()
                        .any(|ability| filter(&ability.node).is_some())
                })
            {
                continue;
            }
            let actor = player_id(&self.object(&holder)?.controller)?;
            let source = self.internal_current_reference(&holder)?;
            let zone = if self.object(&holder)?.zone == "hand" {
                ZoneKind::Hand
            } else {
                ZoneKind::Field
            };
            for ability in self.internal_printed_abilities(&holder)? {
                if ability.node.active_zones == [zone]
                    && filter(&ability.node)
                        .is_some_and(|plan| plan.matches(player, card_before, source, actor))
                {
                    observers.push((holder.clone(), ability));
                }
            }
        }
        Ok(observers)
    }

    pub(in crate::game::internal) fn internal_prepare_discard_events(
        &mut self,
        members: &[String],
        cause_actor: PlayerId,
        cause: Cause,
        cause_source: Option<RuntimeAbilitySource>,
    ) -> Result<DiscardPlan> {
        if !self.has_discard_headers() {
            return Ok(DiscardPlan(Vec::new()));
        }
        let mut candidates = Vec::new();
        let before = SnapshotId(self.state.ir_epoch);
        for id in members {
            let card_before = self.internal_current_reference(id)?;
            let player = player_id(&self.object(id)?.controller)?;
            let observers = self.discard_observers(id, card_before, player)?;
            if observers.is_empty() {
                continue;
            }
            let header = self.pending_header(cause_actor, cause, cause_source, before)?;
            for (holder, ability) in observers {
                let actor = player_id(&self.object(&holder)?.controller)?;
                let source = self.internal_card_source(&holder, ability.node.id, actor)?;
                let mut information = SourceInformation::current(self, source)?;
                if self.object(&holder)?.zone == "hand" {
                    information.seal(Some(
                        self.information_snapshot(source.holder)?.at(header.before),
                    ));
                }
                let pending =
                    self.new_pending(&holder, source, ability, information, header.clone(), None)?;
                candidates.push((card_before, pending));
            }
        }
        Ok(DiscardPlan(candidates))
    }

    pub(in crate::game::internal) fn internal_commit_discard_events(
        &mut self,
        plan: DiscardPlan,
        events: &[DiscardEvent],
    ) -> Result<()> {
        let mut pending = Vec::new();
        for (card, mut occurrence) in plan.0 {
            if let Some(event) = events.iter().find(|event| event.card_before == card) {
                occurrence.header.after = SnapshotId(self.state.ir_epoch);
                occurrence.timing = Some(EventPayload::Discard {
                    event: event.clone(),
                });
                pending.push(occurrence);
            }
        }
        self.internal_admit_pending(pending)
    }
}
