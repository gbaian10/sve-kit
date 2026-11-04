//! Completed rule actions produce immutable contexts before Quick or end-phase progression.
use super::{
    Ability, AbilityKind, AbilityOrigin, Cause, EventHeader, EventPattern, EventSnapshotPolicy,
    Game, KeywordId, PendingOccurrence, Phase, Result, SnapshotId, SourceInformation,
    TriggerAggregation, TriggerCondition, TurnId, ZoneKind, invalid, player_id, unsupported,
};
use crate::game::Frame;
use crate::game::internal::SubjectHistoryEvent;
use crate::game::internal::grant;
use crate::game::{Step, int, string};
use crate::ir::events::damage::DamageFilter;
use crate::ir::optional_payment;
use crate::ir::{
    AttackEvent, Edge, EventPayload as Event, Generation, Nat, ObjectRef, PhaseEvent, PlayerId,
    PlayerRef, RuntimeDamageRecipient,
};
use crate::ir::{BatchPolicy, LimitSpec, TriggerSpec};
use crate::ir::{DamageEvent, DamageFlags, PositiveInt, RuntimeObjectView};
use crate::ir::{EntryOrigin, Expr, ObjectFrozen, Predicate};
use crate::ir::{ExecutionId, printed_keywords};
use core::num::NonZeroU64;
use serde_json::Value;

use crate::ir::events::timing::{
    attack_predicate, evolve_predicate, opponent, phase_predicate, second_header,
};

use crate::game::internal::grant::EffectiveAbility;

pub(super) fn check_header(ability: &Ability) -> Result<bool> {
    let Some(trigger) = &ability.trigger else {
        return Ok(false);
    };
    let admitted = if super::evolution::observer_filter(ability).is_some() {
        true
    } else if let Some(valid) = second_header(ability) {
        valid
    } else {
        match (&ability.source.origin, &trigger.on) {
            (
                AbilityOrigin::Keyword {
                    keyword: KeywordId::OnDriveGain,
                    origin,
                },
                TriggerCondition::Event {
                    pattern: EventPattern::DriveGained { binding, predicate },
                },
            ) => {
                matches!(origin.as_ref(), AbilityOrigin::Printed { .. })
                    && *predicate
                        == Expr::Compute(Box::new(Predicate::ObjectEquals {
                            left: Expr::Frozen(Box::new(ObjectFrozen::DriveGainedSubject {
                                event: *binding,
                            })),
                            right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
                        }))
            }
            (
                AbilityOrigin::Keyword {
                    keyword: KeywordId::OnRace,
                    origin,
                },
                TriggerCondition::Event {
                    pattern: EventPattern::Race { binding, predicate },
                },
            ) => {
                matches!(origin.as_ref(), AbilityOrigin::Printed { .. })
                    && *predicate
                        == Expr::Compute(Box::new(Predicate::ObjectEquals {
                            left: Expr::Frozen(Box::new(ObjectFrozen::RaceSubject {
                                event: *binding,
                            })),
                            right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
                        }))
            }
            (
                AbilityOrigin::Keyword {
                    keyword: KeywordId::OnAttack,
                    origin,
                },
                TriggerCondition::Event {
                    pattern: EventPattern::Attack { binding, predicate },
                },
            ) => {
                matches!(origin.as_ref(), AbilityOrigin::Printed { .. })
                    && *predicate == attack_predicate(*binding)
            }
            (
                AbilityOrigin::Keyword {
                    keyword: KeywordId::OnEvolve,
                    origin,
                },
                TriggerCondition::Event {
                    pattern: EventPattern::Evolve { binding, predicate },
                },
            ) => {
                matches!(origin.as_ref(), AbilityOrigin::Printed { .. })
                    && *predicate == evolve_predicate(*binding)
            }
            _ => return Ok(false),
        }
    };
    if !admitted
        || ability.kind != AbilityKind::Triggered
        || !optional_payment::trigger_play(ability)
        || (trigger.active_zones != [ZoneKind::Field] && !super::ex_phase(ability))
        || trigger.view
            != if matches!(
                trigger.on,
                TriggerCondition::Event {
                    pattern: EventPattern::LeaveField { .. }
                }
            ) {
                EventSnapshotPolicy::BeforeExit
            } else {
                EventSnapshotPolicy::AfterEntry
            }
        || trigger
            .limit
            .is_some_and(|limit| !LimitSpec::is_triggered(limit))
        || !aggregation_allowed(trigger)
    {
        return Err(unsupported(
            "printed timing aggregation or self-event header",
        ));
    }
    Ok(true)
}

fn aggregation_allowed(trigger: &TriggerSpec) -> bool {
    if let TriggerCondition::Event {
        pattern: EventPattern::Damage { binding, predicate },
    } = &trigger.on
        && let Some(plan) = DamageFilter::parse(*binding, predicate)
        && plan.requires_instruction_batch()
    {
        plan.instruction_batch(*binding)
            && trigger.aggregation
                == (TriggerAggregation::PerBatch {
                    key: BatchPolicy::Instruction {},
                })
    } else {
        trigger.aggregation == (TriggerAggregation::PerFact {})
    }
}

pub(super) fn public_event(event: &Event, position: u64) -> Result<String> {
    match event {
        Event::DriveGained { .. } => Ok(format!("DriveGained at field slot {position}")),
        Event::Race { event } => Ok(format!("Race {} at field slot {position}", event.ordinal.0)),
        Event::EnterField { event } => {
            let origin = match event.from {
                EntryOrigin::Zone { zone } => format!("from {zone:?}"),
                EntryOrigin::Other {} => "by creation".into(),
            };
            Ok(format!(
                "EnterField {:?} {origin} at field slot {position}",
                event.characters.names
            ))
        }
        // The source may be a field observer or the arriving EX card itself.
        Event::EnterEx { .. } => Ok(format!("EnterEx while source at slot {position}")),
        Event::StatChange { .. } => Ok(format!("StatChange at field slot {position}")),
        Event::Posture { event } => Ok(format!(
            "Posture {:?} at field slot {position}",
            event.after
        )),
        Event::Destroy { .. } => Ok(format!("Destroy while source at field slot {position}")),
        Event::UbActivated { .. } => {
            Ok(format!("UbActivated while source at field slot {position}"))
        }
        Event::Play { .. } => Ok(format!("Play while source at slot {position}")),
        Event::Discard { .. } => Ok("Discard".into()),
        Event::Draw { .. } => Ok("Draw".into()),
        Event::Move { event } => Ok(format!(
            "Move deck to cemetery of player {} at field slot {position}",
            event.from.player.seat
        )),
        Event::Attack { event } => Ok(format!(
            "Attack {} from field slot {position}",
            event.ordinal.0
        )),
        Event::Phase { event } => Ok(format!(
            "{:?} Begin turn {} player {}",
            event.phase, event.turn.0, event.player.seat
        )),
        Event::Evolve { event } => Ok(format!(
            "Evolve {:?} from field slot {position}",
            event.kind
        )),
        Event::LifeChange { event } => Ok(format!(
            "LifeChange {} player {} at field slot {position}",
            event.delta.0, event.subject.seat
        )),
        Event::Damage { event } => Ok(format!(
            "Damage {} at field slot {position}",
            event.amount.0.get()
        )),
        _ => Err(invalid("unadmitted pending rule event")),
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Rule actions and event producers share the authoritative game."
)]
impl Game {
    /// Apply an admitted rule action with loaded internal cards, stopping before pending abilities.
    ///
    /// # Errors
    /// Rejects a busy authority, prototype card plays, or an unsupported intervention atomically.
    pub fn internal_rule_action(&mut self, decision: &Value) -> Result<Step> {
        if !matches!(
            string(&decision["do"]),
            "evolve" | "attack" | "end-phase" | "guard-act" | "end-discard" | "mulligan"
        ) || self.typed.is_some()
            || self.card_runtime.checkpoint.is_some()
            || self.card_runtime.has_pending()
            || self.card_runtime.question.is_some()
            || self.card_runtime.quick.is_some()
            || !self.prepared.is_empty()
        {
            return Err(unsupported("internal rule action or timing"));
        }
        self.internal_prototype_abilities()?;
        self.check_internal_resources()?;
        let node = format!("internal-rule-{}", self.state.next_decision);
        let result = self.apply_rule_decision(decision, &node)?;
        self.state.next_decision = self.state.next_decision.saturating_add(1);
        Ok(result)
    }

    fn timing_abilities(&self, holder: &str) -> Result<Vec<EffectiveAbility>> {
        Ok(self
            .internal_effective_abilities(holder)?
            .into_iter()
            .filter(|a| is_timing(&a.node))
            .collect())
    }

    pub(super) fn timing_occurrence(
        &mut self,
        holder: &str,
        ability: impl Into<EffectiveAbility>,
        header: &EventHeader,
        event: Event,
    ) -> Result<PendingOccurrence> {
        let ability = ability.into();
        let source = self.internal_effective_source(holder, &ability)?;
        let provider = ability
            .grant
            .as_ref()
            .map(|origin| origin.provider().card.clone());
        let information = SourceInformation::current(self, source)?;
        let mut pending = self.new_pending(
            holder,
            source,
            ability.definition,
            information,
            header.clone(),
            None,
        )?;
        if let Some(provider) = provider {
            pending.card = provider;
        }
        pending.timing = Some(event);
        pending.capture_subject_information(self)?;
        Ok(pending)
    }

    pub(in crate::game) fn internal_attack_event(
        &mut self,
        attacker: &str,
        target: &str,
        before: SnapshotId,
    ) -> Result<()> {
        use crate::ir::events::attack::filter;
        if !self.card_runtime.is_empty() {
            let observation =
                self.internal_subject_history(attacker, SubjectHistoryEvent::Attack, None, false)?;
            self.state.ir_subject_history.push(observation);
        }
        if !self.internal_has_granted_definitions()
            && !self.card_runtime.definitions.cards.values().any(|program| {
                program
                    .card()
                    .abilities
                    .iter()
                    .any(|ability| filter(&ability.node).is_some())
            })
        {
            return Ok(());
        }
        let actor = player_id(&self.object(attacker)?.controller)?;
        let active = player_id(self.active())?;
        let observers = self.attack_observers(attacker, actor, active)?;
        if observers.is_empty() {
            return Ok(());
        }
        let target = if let Some(seat) = target.strip_suffix(".leader") {
            RuntimeDamageRecipient::Leader {
                player: player_id(seat)?,
            }
        } else {
            self.internal_object_reference(target, self.object(target)?.generation)?
        };
        let event = AttackEvent {
            attacker: self.internal_current_reference(attacker)?,
            target,
            controller: actor,
            ordinal: Nat(
                u64::try_from(int(&self.object(attacker)?.state["attacks_this_turn"]))
                    .map_err(invalid)?,
            ),
        };
        let header =
            self.pending_header(actor, Cause::PlayerAction { player: actor }, None, before)?;
        let mut pending = Vec::new();
        for (holder, ability) in observers {
            pending.push(self.timing_occurrence(
                &holder,
                ability,
                &header,
                Event::Attack { event },
            )?);
        }
        self.internal_admit_pending(pending)
    }

    fn attack_observers(
        &self,
        attacker: &str,
        actor: PlayerId,
        active: PlayerId,
    ) -> Result<Vec<(String, EffectiveAbility)>> {
        use crate::ir::Stat;
        use crate::ir::events::attack::filter;
        let mut observers = Vec::new();
        let information = self.event_subject_information(attacker)?;
        let mut keywords = None;
        let mut power = None;
        for holder in self.field_ids() {
            self.require_internal_abilities(&holder, "field attack observer")?;
            let controller = player_id(&self.object(&holder)?.controller)?;
            for ability in self.timing_abilities(&holder)? {
                let Some(plan) = filter(&ability.node) else {
                    continue;
                };
                if ability.node.active_zones != [ZoneKind::Field]
                    || plan
                        .own_turn
                        .is_some_and(|own| own != (controller == active))
                {
                    continue;
                }
                if !plan.subject.matches_information(
                    &information.borrowed(),
                    holder == attacker,
                    controller,
                    actor,
                    active,
                    information.token,
                ) {
                    continue;
                }
                if !plan.keywords_any.is_empty() {
                    if keywords.is_none() {
                        keywords = Some(self.keywords(attacker)?);
                    }
                    let actual = keywords
                        .as_ref()
                        .ok_or_else(|| invalid("missing attack keywords"))?;
                    if !plan.keywords_any.iter().any(|keyword| {
                        (match keyword {
                            KeywordId::SingleDrive => Some("single_drive"),
                            KeywordId::TwinDrive => Some("twin_drive"),
                            _ => grant::keyword(*keyword),
                        })
                        .is_some_and(|name| actual.contains(name))
                    }) {
                        continue;
                    }
                }
                if let Some(minimum) = plan.power_at_least {
                    let actual = if let Some(power) = power {
                        power
                    } else {
                        let actual = self.internal_numeric_stat(attacker, Stat::Attack)?;
                        power = Some(actual);
                        actual
                    };
                    if actual < minimum.0 {
                        continue;
                    }
                }
                observers.push((holder.clone(), ability));
            }
        }
        Ok(observers)
    }

    fn combat_damage_event(&mut self, hit: &Value, before: SnapshotId) -> Result<DamageEvent> {
        let source = string(&hit["source"]);
        let recipient = if let Some(seat) = string(&hit["target"]).strip_suffix(".leader") {
            RuntimeDamageRecipient::Leader {
                player: player_id(seat)?,
            }
        } else {
            RuntimeDamageRecipient::Object {
                object: self.internal_current_reference(string(&hit["target"]))?,
            }
        };
        Ok(DamageEvent {
            source: RuntimeObjectView {
                object: self.internal_current_reference(source)?,
                snapshot: before,
            },
            recipient,
            amount: PositiveInt(
                NonZeroU64::new(u64::try_from(int(&hit["amount"])).map_err(invalid)?)
                    .ok_or_else(|| invalid("nonpositive actual damage"))?,
            ),
            flags: if string(&hit["target"]).ends_with(".leader") {
                DamageFlags::Attack
            } else if hit["attack"] == true {
                DamageFlags::AttackCombat
            } else {
                DamageFlags::Combat
            },
            controller: player_id(&self.object(source)?.controller)?,
        })
    }

    pub(in crate::game) fn internal_damage_events(
        &mut self,
        hits: &[Value],
        facts: Option<&[DamageEvent]>,
        frame: &Frame,
        before: SnapshotId,
    ) -> Result<()> {
        if !self.internal_has_drain_grants()
            && !self.internal_has_granted_definitions()
            && !self.card_runtime.definitions.cards.values().any(|program| {
                program.card().abilities.iter().any(|ability| {
                    matches!(
                        ability.node.trigger.as_ref().map(|trigger| &trigger.on),
                        Some(TriggerCondition::Event {
                            pattern: EventPattern::Damage { .. }
                        })
                    )
                })
            })
        {
            return Ok(());
        }
        let active = player_id(self.active())?;
        let mut pending = Vec::new();
        for (index, hit) in hits.iter().enumerate() {
            let recipient_id = string(&hit["target"]);
            let event = if let Some(facts) = facts {
                *facts
                    .get(index)
                    .ok_or_else(|| invalid("missing actual damage fact"))?
            } else {
                if hit["battle"] != true {
                    return Err(unsupported("unadmitted damage producer"));
                }
                self.combat_damage_event(hit, before)?
            };
            let committed_recipient = if let Some(seat) = recipient_id.strip_suffix(".leader") {
                RuntimeDamageRecipient::Leader {
                    player: player_id(seat)?,
                }
            } else {
                RuntimeDamageRecipient::Object {
                    object: self.internal_current_reference(recipient_id)?,
                }
            };
            if event.recipient != committed_recipient {
                return Err(invalid("damage recipient proof mismatch"));
            }
            let observers = self.damage_observers(hit, event, active)?;
            if observers.is_empty() {
                continue;
            }
            let (cause, actor, source) = if facts.is_some() {
                let execution = frame.cause["effect"]["execution"]
                    .as_u64()
                    .ok_or_else(|| invalid("missing damage execution cause"))?;
                let source = self.card_runtime.active_source;
                (
                    Cause::Effect {
                        execution: ExecutionId(execution),
                    },
                    source.map_or(event.controller, |source| source.controller),
                    source,
                )
            } else {
                (
                    Cause::Rule {
                        rule: self.rule_id("8.4.9")?,
                    },
                    active,
                    None,
                )
            };
            let header = self.pending_header(actor, cause, source, before)?;
            for (holder, ability) in observers {
                let occurrence = if let Some(origin) = ability.grant.as_ref().filter(|_| {
                    printed_keywords::card_declaration(&ability.node) == Some(KeywordId::Drain)
                }) {
                    let granted_source = self.internal_granted_drain_source(&holder, origin)?;
                    let information = SourceInformation::current(self, granted_source)?;
                    let mut occurrence = self.new_pending(
                        &holder,
                        granted_source,
                        ability.definition,
                        information,
                        header.clone(),
                        None,
                    )?;
                    occurrence.card.clone_from(&origin.provider().card);
                    occurrence.timing = Some(Event::Damage { event });
                    occurrence
                } else {
                    self.timing_occurrence(&holder, ability, &header, Event::Damage { event })?
                };
                pending.push(occurrence);
            }
        }
        self.internal_admit_pending(pending)
    }

    fn damage_abilities(&self, holder: &str) -> Result<Vec<EffectiveAbility>> {
        let printed = self.timing_abilities(holder)?;
        let printed_drain = printed.iter().any(|ability| {
            printed_keywords::card_declaration(&ability.node) == Some(KeywordId::Drain)
        });
        let mut abilities = printed;
        // CR 12.13.3 treats printed and granted copies as a single Drain.
        if !printed_drain && let Some(origin) = self.internal_drain_grant(holder)? {
            abilities.push(EffectiveAbility {
                definition: origin.drain_ability(),
                grant: Some(origin),
            });
        }
        Ok(abilities)
    }

    fn damage_observers(
        &mut self,
        hit: &Value,
        event: DamageEvent,
        active: PlayerId,
    ) -> Result<Vec<(String, EffectiveAbility)>> {
        use crate::ir::events::damage::RecipientFilter;
        let mut observers = Vec::new();
        let mut subject = None;
        for holder in self.field_ids() {
            self.require_internal_abilities(&holder, "field damage observer")?;
            let controller = player_id(&self.object(&holder)?.controller)?;
            for ability in self.damage_abilities(&holder)? {
                let Some(trigger) = &ability.definition.node.trigger else {
                    continue;
                };
                if trigger.active_zones != [ZoneKind::Field] {
                    continue;
                }
                let TriggerCondition::Event {
                    pattern: EventPattern::Damage { binding, predicate },
                } = &trigger.on
                else {
                    continue;
                };
                let Some(plan) = DamageFilter::parse(*binding, predicate) else {
                    continue;
                };
                if plan.kind.is_some_and(|kind| !kind.matches(event.flags))
                    || plan
                        .own_turn
                        .is_some_and(|own| own != (controller == active))
                {
                    continue;
                }
                let holder_reference = self.internal_current_reference(&holder)?;
                let recipient_matches = match plan.recipient {
                    None => true,
                    Some(RecipientFilter::SourceHolder) => {
                        event.recipient
                            == (RuntimeDamageRecipient::Object {
                                object: holder_reference,
                            })
                    }
                    Some(RecipientFilter::Leader(own)) => match event.recipient {
                        RuntimeDamageRecipient::Leader { player } => {
                            own.is_none_or(|own| own == (controller == player))
                        }
                        RuntimeDamageRecipient::Object { .. } => false,
                    },
                    Some(RecipientFilter::OpposingFollowers { .. }) => match event.recipient {
                        RuntimeDamageRecipient::Object { .. } => {
                            let recipient = self.object(string(&hit["target"]))?;
                            recipient.zone == "field"
                                && player_id(&recipient.controller)? != controller
                                && self.object_type(&recipient.id)?.contains("フォロワー")
                        }
                        RuntimeDamageRecipient::Leader { .. } => false,
                    },
                };
                if !recipient_matches {
                    continue;
                }
                if let Some(filter) = &plan.dealt {
                    let source_id = string(&hit["source"]);
                    if self
                        .internal_identities()?
                        .get(&event.source.object.id)
                        .is_none_or(|id| id != source_id)
                    {
                        return Err(invalid("damage source identity mismatch"));
                    }
                    if (filter.kind.is_some()
                        || filter.trait_id.is_some()
                        || filter.name.is_some()
                        || filter.class.is_some())
                        && self.internal_current_reference(source_id)? != event.source.object
                    {
                        return Err(unsupported("departed damage subject characteristics"));
                    }
                    if subject.is_none() {
                        subject = Some(self.event_subject_information(source_id)?);
                    }
                    let information = subject
                        .as_ref()
                        .ok_or_else(|| invalid("missing damage source information"))?;
                    if !filter.matches_information(
                        &information.borrowed(),
                        holder_reference == event.source.object,
                        controller,
                        event.controller,
                        active,
                        information.token,
                    ) {
                        continue;
                    }
                }
                observers.push((holder.clone(), ability));
            }
        }
        Ok(observers)
    }

    pub(in crate::game) fn internal_life_change_events(
        &mut self,
        changes: &[(String, i64, i64)],
        cause: &Value,
        battle: bool,
        snapshot: SnapshotId,
    ) -> Result<()> {
        use crate::ir::events::life::LifeFilter;
        use crate::ir::{Int, LifeChangeEvent};
        if !self.card_runtime.definitions.cards.values().any(|program| {
            program.card().abilities.iter().any(|ability| {
                matches!(
                    ability.node.trigger.as_ref().map(|trigger| &trigger.on),
                    Some(TriggerCondition::Event {
                        pattern: EventPattern::LifeChange { .. }
                    })
                )
            })
        }) {
            return Ok(());
        }
        let active = player_id(self.active())?;
        let mut pending = Vec::new();
        for (seat, before, after) in changes {
            let delta = after
                .checked_sub(*before)
                .ok_or_else(|| invalid("life delta overflow"))?;
            if delta == 0 {
                continue;
            }
            let event = LifeChangeEvent {
                subject: player_id(seat)?,
                before: Int(*before),
                after: Int(*after),
                delta: Int(delta),
            };
            let mut observers = Vec::new();
            for holder in self.field_ids() {
                self.require_internal_abilities(&holder, "field life observer")?;
                let controller = player_id(&self.object(&holder)?.controller)?;
                for ability in self.timing_abilities(&holder)? {
                    let Some(trigger) = &ability.node.trigger else {
                        continue;
                    };
                    if trigger.active_zones != [ZoneKind::Field] {
                        continue;
                    }
                    let TriggerCondition::Event {
                        pattern: EventPattern::LifeChange { binding, predicate },
                    } = &trigger.on
                    else {
                        continue;
                    };
                    if LifeFilter::parse(*binding, predicate)
                        .is_some_and(|filter| filter.matches(event, controller, active))
                    {
                        observers.push((holder.clone(), ability));
                    }
                }
            }
            if observers.is_empty() {
                continue;
            }
            let source = if battle {
                None
            } else {
                self.card_runtime.active_source
            };
            let (authority, actor) = if let Some(execution) = cause["effect"]["execution"].as_u64()
            {
                (
                    Cause::Effect {
                        execution: ExecutionId(execution),
                    },
                    source.map_or(event.subject, |source| source.controller),
                )
            } else if battle {
                (
                    Cause::Rule {
                        rule: self.rule_id("8.4.9")?,
                    },
                    active,
                )
            } else {
                return Err(unsupported("unadmitted life change producer"));
            };
            let header = self.pending_header(actor, authority, source, snapshot)?;
            for (holder, ability) in observers {
                pending.push(self.timing_occurrence(
                    &holder,
                    ability,
                    &header,
                    Event::LifeChange { event },
                )?);
            }
        }
        self.internal_admit_pending(pending)
    }

    fn internal_object_reference(
        &mut self,
        holder: &str,
        generation: u64,
    ) -> Result<RuntimeDamageRecipient> {
        let id = self
            .internal_identities()?
            .into_iter()
            .find(|(_, physical)| physical == holder)
            .map(|(id, _)| id)
            .ok_or_else(|| invalid("missing rule object identity"))?;
        Ok(RuntimeDamageRecipient::Object {
            object: ObjectRef {
                id,
                generation: Generation(generation),
            },
        })
    }

    pub(in crate::game) fn internal_phase_event(
        &mut self,
        phase: Phase,
        before: SnapshotId,
    ) -> Result<()> {
        let actor = player_id(self.active())?;
        let mut candidates = Vec::new();
        for seat in ["P1", "P2"] {
            for holder in self
                .zone_ids(seat, "field")
                .into_iter()
                .chain(self.zone_ids(seat, "ex"))
            {
                for ability in self.timing_abilities(&holder)? {
                    let Some(trigger) = &ability.node.trigger else {
                        continue;
                    };
                    let zone = if self.object(&holder)?.zone == "ex" {
                        ZoneKind::Ex
                    } else {
                        ZoneKind::Field
                    };
                    if !trigger.active_zones.contains(&zone) {
                        continue;
                    }
                    if let TriggerCondition::Event {
                        pattern: EventPattern::Phase { binding, predicate },
                    } = &trigger.on
                    {
                        let controller = player_id(&self.object(&holder)?.controller)?;
                        let player = if controller == actor {
                            PlayerRef::Controller {}
                        } else {
                            opponent()
                        };
                        if *predicate == phase_predicate(*binding, phase, None)
                            || *predicate == phase_predicate(*binding, phase, Some(player))
                        {
                            candidates.push((holder.clone(), ability));
                        }
                    }
                }
            }
        }
        if candidates.is_empty() {
            return Ok(());
        }
        let event = PhaseEvent {
            turn: TurnId(self.state.schedule.current()?),
            player: actor,
            phase,
            edge: Edge::Begin,
        };
        let rule = self.rule_id(match phase {
            Phase::Main => "7.3.1",
            Phase::End => "7.4.1",
            Phase::Start => return Err(unsupported("start phase event")),
        })?;
        let header = self.pending_header(actor, Cause::Rule { rule }, None, before)?;
        let mut pending = Vec::new();
        for (holder, ability) in candidates {
            pending.push(self.timing_occurrence(
                &holder,
                ability,
                &header,
                Event::Phase { event },
            )?);
        }
        self.internal_admit_pending(pending)
    }
}

pub(super) fn is_timing(ability: &Ability) -> bool {
    matches!(
        ability.trigger.as_ref().map(|t| &t.on),
        Some(TriggerCondition::Event {
            pattern: EventPattern::Attack { .. }
                | EventPattern::Phase { .. }
                | EventPattern::Evolve { .. }
                | EventPattern::Damage { .. }
                | EventPattern::LifeChange { .. }
        })
    )
}
