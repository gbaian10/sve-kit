use super::{BindingKind, CaptureTime, Checked, Checker, WellFormedError, WellFormedReason};
use crate::ir::OncePerEvent;
use crate::ir::{
    Ability, AbilityKind, AbilityQualification, Capture, CardProgram, Cardinality,
    CharacteristicPatch, ChoiceDomain, ContinuousLayer, DecisionKind, Duration, DurationEnd,
    Effect, EffectTarget, EventContext, EventPattern, ExecutionRef, GrantedAbility, Nat,
    NumericChange, PlayerRef, PlayerRelation, PlayerSet, Procedure, ScopeId, TextQuantifier,
    TimeAnchor, TimestampBasis, TriggerAggregation, TriggerCondition, TriggerSpec, Visibility,
    ZoneKind,
};
use crate::ir::{
    AbilityOrigin, Binding, Bool, EnterFieldEvent, Expr, KeywordId, ObjectFrozen, ObjectOperand,
    Predicate,
};
use crate::ir::{
    ActionPattern, EventRewrite, PermissionSubject, ProposedEventPattern, ReplacementScope,
    Selector,
};
use crate::ir::{CostChange, Int, LimitSpec};
use alloc::collections::BTreeSet;

/// An immutable card whose entire accepted ability tree passed structural checking.
/// This does not grant capability support or permission to execute.
/// ```compile_fail
/// use sve_engine::ir::{CardProgram, WellFormedCardProgram};
/// let unchecked = WellFormedCardProgram { card: CardProgram { abilities: vec![], card_play_costs: vec![] } };
/// ```
/// ```compile_fail
/// use sve_engine::ir::WellFormedCardProgram;
/// fn mutate(checked: &mut WellFormedCardProgram) { checked.card().abilities.clear(); }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WellFormedCardProgram {
    card: CardProgram,
}
impl WellFormedCardProgram {
    /// Check every ability, including declarations and unselected branches.
    ///
    /// # Errors
    /// Returns the first invalid or unsupported shape.
    pub fn check(card: CardProgram) -> Result<Self, WellFormedError> {
        let mut faces = BTreeSet::new();
        let mut checker = Checker::new(ScopeId(0));
        for (index, face) in card.card_play_costs.iter().enumerate() {
            checker.at(&format!("card_play_costs/{index}"), |checker| {
                checker.located(face, |checker, face| {
                    if face.face.0.is_empty()
                        || (face.modifiers.is_empty() && face.exchange.is_none())
                        || !faces.insert(face.face.0.clone())
                    {
                        return Err(checker.error(WellFormedReason::InvalidEntry));
                    }
                    let mut has_set = false;
                    for (modifier_index, modifier) in face.modifiers.iter().enumerate() {
                        checker.at(&format!("modifiers/{modifier_index}"), |checker| {
                            checker.located(modifier, |checker, modifier| {
                                match &modifier.change {
                                    CostChange::Delta {
                                        amount: Expr::Literal(Int(amount)),
                                    } if *amount <= 0 => {}
                                    CostChange::Set {
                                        amount: Expr::Literal(Int(amount)),
                                    } if *amount >= 0 && !has_set => {
                                        has_set = true;
                                    }
                                    CostChange::Delta { .. } | CostChange::Set { .. } => {
                                        return checker.reject();
                                    }
                                }
                                if let Some(condition) = &modifier.condition {
                                    checker.at("condition", |checker| {
                                        checker.isolated(|checker| checker.boolean(condition))
                                    })?;
                                }
                                Ok(())
                            })
                        })?;
                    }
                    if let Some(exchange) = &face.exchange {
                        checker.at("exchange", |checker| {
                            checker.located(exchange, |checker, exchange| {
                                checker.exchange(exchange, &mut has_set)?;
                                checker
                                    .card_payments
                                    .insert(face.face.0.clone(), exchange.receipt);
                                Ok(())
                            })
                        })?;
                    }
                    Ok(())
                })
            })?;
        }
        for (index, ability) in card.abilities.iter().enumerate() {
            checker.at(&format!("abilities/{index}"), |checker| {
                checker.located(ability, Checker::ability)
            })?;
        }
        Ok(Self { card })
    }

    /// Inspect exactly the checked card.
    #[must_use]
    pub const fn card(&self) -> &CardProgram {
        &self.card
    }
}

/// A checked rule or quick-window procedure with no caller-supplied environment.
/// ```compile_fail
/// use sve_engine::ir::{Procedure, WellFormedProcedure};
/// fn forge(procedure: Procedure) { let _ = WellFormedProcedure { procedure }; }
/// ```
/// ```compile_fail
/// use sve_engine::ir::{Procedure, WellFormedProcedure};
/// fn mutate(checked: &mut WellFormedProcedure, other: Procedure) {
///     *checked.procedure() = other;
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WellFormedProcedure {
    procedure: Procedure,
}
impl WellFormedProcedure {
    /// Check a decision at its explicit procedure entry.
    ///
    /// # Errors
    /// Rejects mismatched procedures and domains.
    pub fn check(procedure: Procedure) -> Result<Self, WellFormedError> {
        match &procedure {
            Procedure::Rule {
                procedure,
                rule,
                decision,
                scope,
            } => {
                let mut checker = Checker::new(*scope);
                if decision.kind
                    != (DecisionKind::RuleChoice {
                        procedure: *procedure,
                        rule: *rule,
                    })
                {
                    return Err(checker.error(WellFormedReason::DecisionTiming));
                }
                checker.rule_decision(*procedure, decision)?;
            }
            Procedure::Quick { window, decision } => {
                let mut checker = Checker::new(ScopeId(0));
                if decision.kind != (DecisionKind::QuickWindow { window: *window })
                    || decision.actor
                        != (PlayerRef::RequireOne {
                            players: Box::new(PlayerSet {
                                relation: PlayerRelation::Opponents,
                                ..self_player(PlayerRef::Active {})
                            }),
                        })
                    || decision.domain
                        != (ChoiceDomain::QuickActionOrPass {
                            player: decision.actor.clone(),
                        })
                    || !decision.captures.is_empty()
                {
                    return Err(checker.error(WellFormedReason::DecisionTiming));
                }
                checker.decision_header(decision)?;
                if decision.visibility
                    != (Visibility::Only {
                        players: self_player(decision.actor.clone()),
                    })
                {
                    return checker.reject().map(|()| Self { procedure });
                }
            }
        }
        Ok(Self { procedure })
    }

    /// Inspect the checked procedure.
    #[must_use]
    pub const fn procedure(&self) -> &Procedure {
        &self.procedure
    }
}

fn self_entry(event: Binding<EnterFieldEvent>) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::ObjectEquals {
        left: Expr::Frozen(Box::new(ObjectFrozen::EnterFieldObject { event })),
        right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
    }))
}

const fn self_player(player: PlayerRef) -> PlayerSet {
    PlayerSet {
        relation: PlayerRelation::SelfPlayer,
        relative_to: player,
        cardinality: Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        },
        text_quantifier: TextQuantifier::Implicit,
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Entry checks share the structural checker state."
)]
impl Checker {
    fn ability(&mut self, ability: &Ability) -> Checked {
        let previous_payment = self.card_payment_body.take();
        let previous_env = self.env.clone();
        let previous_root = self.root;
        let previous_entry = self.entry;
        let previous_resolution = self.resolution_entry.clone();
        let previous_play = self.play_entry.clone();
        let previous_controller = self.play_controller.clone();
        let previous_source = self.ability_source.replace(ability.source.clone());
        let previous_information = self.source_information_entry;
        self.source_information_entry = true;
        let previous_revoked = self.revoked;
        self.env = Self::new(ability.scope).env;
        self.root = ability.scope;
        self.entry = true;
        self.revoked = false;
        self.env.play_modes = ability.play.is_some();
        self.play_entry = None;
        self.play_controller = ability
            .play
            .as_ref()
            .map(|_| ability.source.controller.clone());
        let result = self.ability_contents(ability);
        self.card_payment_body = previous_payment;
        self.env = previous_env;
        self.root = previous_root;
        self.entry = previous_entry;
        self.resolution_entry = previous_resolution;
        self.play_entry = previous_play;
        self.play_controller = previous_controller;
        self.ability_source = previous_source;
        self.source_information_entry = previous_information;
        self.revoked = previous_revoked;
        result
    }

    fn ability_contents(&mut self, ability: &Ability) -> Checked {
        if !self.ability_ids.insert(ability.id) {
            return Err(self.error(WellFormedReason::InvalidEntry));
        }
        if !self.scopes.insert(ability.scope) {
            return Err(self.error(WellFormedReason::DuplicateScope));
        }
        if ability.active_zones.is_empty()
            || ability.active_zones.iter().collect::<BTreeSet<_>>().len()
                != ability.active_zones.len()
            || (matches!(ability.kind, AbilityKind::Spell | AbilityKind::Activated)
                && ability.play.is_none())
            || ability
                .body
                .as_ref()
                .is_some_and(|body| body.scope != ability.scope)
            || (ability.kind == AbilityKind::Triggered) != ability.trigger.is_some()
            || (ability.kind == AbilityKind::Static
                && (ability.play.is_some() || ability.body.is_some()))
            || (ability.kind != AbilityKind::Static && ability.body.is_none())
            || (ability.kind != AbilityKind::Static && !ability.declarations.is_empty())
            || (ability.trigger.is_none() && !ability.event_captures.is_empty())
        {
            return Err(self.error(WellFormedReason::InvalidEntry));
        }
        self.play_header(ability)?;
        if ability.qualification != AbilityQualification::Ordinary {
            return self.reject();
        }
        self.at("source", |checker| checker.ability_source(&ability.source))?;
        if matches!(
            ability.source.origin,
            AbilityOrigin::Keyword {
                keyword: KeywordId::Evolution,
                ..
            }
        ) {
            return self.evolution_entry(ability);
        }
        if matches!(
            ability.source.origin,
            AbilityOrigin::Keyword {
                keyword: KeywordId::Fanfare,
                ..
            }
        ) {
            self.fanfare_header(ability)?;
        }
        if let Some(trigger) = &ability.trigger {
            self.at("trigger", |checker| checker.trigger(trigger))?;
            self.at("event_captures", |checker| {
                checker.captures(
                    &ability.event_captures,
                    CaptureTime::AtEvent {
                        event: EventContext::Trigger {},
                        policy: trigger.view,
                    },
                )
            })?;
        }
        if let Some(play) = &ability.play {
            self.play_entry = Some(self.env.clone());
            self.at("play", |checker| {
                checker.captures(
                    &play.captures,
                    CaptureTime::AtPlay {
                        execution: ExecutionRef::Current {},
                    },
                )?;
                checker.play_entry = Some(checker.env.clone());
                for (index, decision) in play.parameters.iter().enumerate() {
                    checker.at(&format!("parameters/{index}"), |checker| {
                        checker.play_decision(decision, &ability.source.controller)
                    })?;
                }
                checker.at("costs", |checker| checker.costs(&play.costs))?;
                Ok(())
            })?;
        }
        self.env.play_payments = ability.play.as_ref().map(|_| ability.scope);
        self.resolution_entry = self.env.clone();
        let previous_payment = self.card_payment_body.take();
        if ability.kind == AbilityKind::Spell
            && ability.qualification == AbilityQualification::Ordinary
            && let AbilityOrigin::Printed { face, .. } = &ability.source.origin
        {
            self.card_payment_body = Some(face.clone());
        }
        if let Some(body) = &ability.body {
            self.at("body/steps", |checker| checker.program_steps(body))?;
        }
        self.card_payment_body = previous_payment;
        for (index, effect) in ability.declarations.iter().enumerate() {
            // Only this complete header grants PlayCard; generic permission remains closed.
            if super::super::quick::card_permission(ability) {
                continue;
            }
            self.at(&format!("declarations/{index}"), |checker| {
                checker.effect(effect, &ability.active_zones)
            })?;
        }
        Ok(())
    }

    fn play_header(&self, ability: &Ability) -> Checked {
        if ability
            .play
            .as_ref()
            .and_then(|play| play.limit)
            .is_some_and(|limit| {
                ability.kind != AbilityKind::Activated
                    || !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
                    || ability.active_zones != [ZoneKind::Field]
                    || !limit.is_activated()
            })
        {
            return self.reject();
        }
        if let Some(body) = &ability.body {
            let count = super::modes::play_mode_groups(body);
            if count > 1
                || (count > 0
                    && ability
                        .play
                        .as_ref()
                        .is_some_and(|play| !play.parameters.is_empty()))
            {
                return self.reject();
            }
        }
        Ok(())
    }

    fn fanfare_header(&self, ability: &Ability) -> Checked {
        let Some(TriggerSpec {
            on:
                TriggerCondition::Event {
                    pattern: EventPattern::EnterField { binding, predicate },
                },
            active_zones,
            ..
        }) = &ability.trigger
        else {
            return self.reject();
        };
        if ability.kind != AbilityKind::Triggered
            || ability.play.is_some()
            || ability.source.holder != (ObjectOperand::SourceHolder {})
            || ability.active_zones != [ZoneKind::Field]
            || *active_zones != [ZoneKind::Field]
            || *predicate != self_entry(*binding)
        {
            return self.reject();
        }
        Ok(())
    }

    pub(super) fn captures(&mut self, captures: &[Capture], at: CaptureTime) -> Checked {
        let entry = self.env.clone();
        let mut after = entry.clone();
        let previous = self.capture_time.replace(at);
        for (index, capture) in captures.iter().enumerate() {
            self.env = entry.clone();
            self.at(&index.to_string(), |checker| checker.capture(capture))?;
            after.bindings.extend(self.env.bindings.clone());
            after.access.extend(self.env.access.clone());
        }
        self.capture_time = previous;
        self.env = after;
        Ok(())
    }

    fn trigger(&mut self, trigger: &TriggerSpec) -> Checked {
        if trigger.active_zones.is_empty() {
            return Err(self.error(WellFormedReason::InvalidEntry));
        }
        // Union narrowing and batch event envelopes require their own proof.
        if trigger.aggregation != (TriggerAggregation::PerFact {})
            || trigger
                .limit
                .is_some_and(|limit| limit != LimitSpec::once_per_turn())
        {
            return self.reject();
        }
        match &trigger.on {
            TriggerCondition::Event { pattern } => {
                self.env.trigger = Some(trigger.view);
                self.event_pattern(pattern, Some(trigger.view), true)
            }
            TriggerCondition::State { .. } => self.reject(),
        }
    }

    fn effect(&mut self, effect: &Effect, ability_zones: &[ZoneKind]) -> Checked {
        match effect {
            Effect::Continuous {
                id,
                source,
                target,
                condition,
                patch,
                layer,
                timestamp,
                active_zones,
                duration,
            } => {
                if !self.effect_ids.insert(*id) {
                    return Err(self.error(WellFormedReason::InvalidEntry));
                }
                if active_zones != ability_zones {
                    return Err(self.error(WellFormedReason::InvalidEntry));
                }
                self.ability_source(source)?;
                match target {
                    EffectTarget::Dynamic { selector } => self.selector(selector)?,
                    EffectTarget::Captured { objects } => {
                        self.reference(objects.binding, BindingKind::Capture)?;
                    }
                }
                if let Some(condition) = condition {
                    self.boolean(condition)?;
                }
                self.patch(patch, *layer)?;
                match timestamp {
                    TimestampBasis::ZoneEntry { object } => self.object(object)?,
                    TimestampBasis::ProperPlay { .. } => return self.reject(),
                }
                self.duration(duration)
            }
            Effect::Replacement {
                id,
                source,
                pattern,
                condition,
                rewrite,
                scope,
                affected_player,
                once_per_event: OncePerEvent::Enforced,
                usage,
                prospective_chars,
                duration,
            } => {
                if !self.effect_ids.insert(*id) {
                    return Err(self.error(WellFormedReason::InvalidEntry));
                }
                self.ability_source(source)?;
                if usage.is_some() || prospective_chars.is_some() {
                    return self.reject();
                }
                let ProposedEventPattern::LeavesZone { objects, from } = pattern else {
                    return self.reject();
                };
                // SelfOnly is a self-replacement inside its own effect, not a holder's static rule.
                if !ability_zones.contains(from)
                    || *scope != ReplacementScope::General
                    || *objects
                        != (Selector::Objects {
                            objects: vec![ObjectOperand::SourceHolder {}],
                        })
                {
                    return self.reject();
                }
                self.at("pattern/objects", |checker| checker.selector(objects))?;
                self.at("condition", |checker| checker.boolean(condition))?;
                self.at("affected_player", |checker| checker.player(affected_player))?;
                let EventRewrite::RedirectMove { to, position } = rewrite else {
                    return self.reject();
                };
                self.at("rewrite/to", |checker| {
                    checker.positioned_destination(to, *position)
                })?;
                self.duration(duration)
            }
            Effect::Permission {
                subject,
                action,
                condition,
                duration,
                ..
            } => {
                self.at("action", |checker| {
                    checker.permission_action(subject, action)
                })?;
                self.at("condition", |checker| checker.boolean(condition))?;
                self.duration(duration)
            }
            Effect::RuleModifier { .. } | Effect::DeckRule { .. } | Effect::CostModifier { .. } => {
                self.reject()
            }
        }
    }

    pub(super) fn permission_action(
        &mut self,
        subject: &PermissionSubject,
        action: &ActionPattern,
    ) -> Checked {
        match subject {
            PermissionSubject::Objects { objects } => self.selector(objects)?,
            PermissionSubject::Players { players } => self.players(players)?,
        }
        match action {
            ActionPattern::Destroy { objects, cause }
            | ActionPattern::Banish { objects, cause } => {
                self.selector(objects)?;
                self.cause_pattern(cause)
            }
            ActionPattern::PlayCard { .. }
            | ActionPattern::PlayAbility { .. }
            | ActionPattern::Attack { .. }
            | ActionPattern::ChooseTarget { .. }
            | ActionPattern::Draw { .. }
            | ActionPattern::Stand { .. }
            | ActionPattern::IncreasePpMaximum { .. }
            | ActionPattern::IgnoreWard { .. }
            | ActionPattern::Reroll { .. }
            | ActionPattern::ChooseMode { .. }
            | ActionPattern::Win { .. }
            | ActionPattern::Lose { .. }
            | ActionPattern::TriggerAdmission { .. } => self.reject(),
        }
    }

    pub(super) fn patch(&mut self, patch: &CharacteristicPatch, layer: ContinuousLayer) -> Checked {
        match patch {
            CharacteristicPatch::Combined { patches } if !patches.is_empty() => {
                for (index, child) in patches.iter().enumerate() {
                    self.at(&format!("patches/{index}"), |checker| {
                        checker.patch(child, layer)
                    })?;
                }
                Ok(())
            }
            CharacteristicPatch::Numeric { change, .. } if layer == ContinuousLayer::Numeric => {
                match change {
                    NumericChange::Add { amount } | NumericChange::Set { amount } => {
                        self.integer(amount)
                    }
                }
            }
            CharacteristicPatch::GrantAbility { abilities }
                if layer == ContinuousLayer::Ability && !abilities.is_empty() =>
            {
                for (index, granted) in abilities.iter().enumerate() {
                    self.at(&format!("abilities/{index}"), |checker| match granted {
                        GrantedAbility::Definition { ability } => checker.ability(ability),
                        GrantedAbility::Keyword { .. } => checker.reject(),
                    })?;
                }
                Ok(())
            }
            CharacteristicPatch::GrantAbility { .. }
            | CharacteristicPatch::SuppressAbility { .. }
            | CharacteristicPatch::SetKind { .. }
            | CharacteristicPatch::AddName { .. }
            | CharacteristicPatch::AddTrait { .. }
            | CharacteristicPatch::RemoveTrait { .. }
            | CharacteristicPatch::Numeric { .. }
            | CharacteristicPatch::Combined { .. } => self.reject(),
        }
    }

    fn duration(&self, duration: &Duration) -> Checked {
        if duration.anchor != (TimeAnchor::Installation {})
            || duration.usage.is_some()
            || !duration.intervals.is_empty()
        {
            return self.reject();
        }
        if duration.end != (DurationEnd::NoTemporalExpiry {}) {
            return self.reject();
        }
        Ok(())
    }
}
