use super::{BindingKind, CaptureTime, Checked, Checker, WellFormedError, WellFormedReason};
use crate::ir::{
    Ability, AbilityKind, AbilityQualification, Capture, CardProgram, Cardinality,
    CharacteristicPatch, ChoiceDomain, ContinuousLayer, CostSpec, DecisionKind, Duration,
    DurationEnd, Effect, EffectTarget, EventContext, EventPattern, EventSnapshotPolicy,
    ExecutionRef, GrantedAbility, Nat, NumericChange, PlayerRef, PlayerRelation, PlayerSet,
    Procedure, ScopeId, TextQuantifier, TimeAnchor, TimestampBasis, TriggerAggregation,
    TriggerCondition, TriggerSpec, Visibility, ZoneKind,
};
use alloc::collections::BTreeSet;

/// An immutable card whose entire accepted ability tree passed structural checking.
/// This does not grant capability support or permission to execute.
/// ```compile_fail
/// use sve_engine::ir::{CardProgram, WellFormedCardProgram};
/// let unchecked = WellFormedCardProgram { card: CardProgram { abilities: vec![] } };
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
        let mut checker = Checker::new(ScopeId(0));
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
        let previous_env = self.env.clone();
        let previous_root = self.root;
        let previous_entry = self.entry;
        let previous_resolution = self.resolution_entry.clone();
        let previous_play = self.play_entry.clone();
        let previous_controller = self.play_controller.clone();
        let previous_source = self.ability_source.replace(ability.source.clone());
        self.env = Self::new(ability.scope).env;
        self.root = ability.scope;
        self.entry = true;
        self.env.play_modes = ability.play.is_some();
        self.play_entry = None;
        self.play_controller = ability
            .play
            .as_ref()
            .map(|_| ability.source.controller.clone());
        let result = self.ability_contents(ability);
        self.env = previous_env;
        self.root = previous_root;
        self.entry = previous_entry;
        self.resolution_entry = previous_resolution;
        self.play_entry = previous_play;
        self.play_controller = previous_controller;
        self.ability_source = previous_source;
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
        if ability.qualification != AbilityQualification::Ordinary {
            return self.reject();
        }
        self.at("source", |checker| checker.ability_source(&ability.source))?;
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
                // Full play-time payment remains outside this structural slice.
                if !matches!(&play.costs, CostSpec::Ordered { steps } if steps.is_empty()) {
                    return checker.reject();
                }
                Ok(())
            })?;
        }
        self.resolution_entry = self.env.clone();
        if let Some(body) = &ability.body {
            self.at("body/steps", |checker| checker.program_steps(body))?;
        }
        for (index, effect) in ability.declarations.iter().enumerate() {
            self.at(&format!("declarations/{index}"), |checker| {
                checker.effect(effect, &ability.active_zones)
            })?;
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
        if trigger.aggregation != (TriggerAggregation::PerFact {}) || trigger.limit.is_some() {
            return self.reject();
        }
        match &trigger.on {
            TriggerCondition::Event { pattern } => self.event_pattern(pattern, trigger.view),
            TriggerCondition::State { .. } => self.reject(),
        }
    }

    fn event_pattern(&mut self, pattern: &EventPattern, view: EventSnapshotPolicy) -> Checked {
        macro_rules! event {
            ($binding:expr, $predicate:expr, $policy:expr) => {{
                if view != $policy {
                    return Err(self.error(WellFormedReason::InvalidEntry));
                }
                self.define(*$binding, BindingKind::Value)?;
                self.boolean($predicate)
            }};
        }
        match pattern {
            EventPattern::EnterField { binding, predicate } => {
                event!(binding, predicate, EventSnapshotPolicy::AfterEntry)
            }
            EventPattern::EnterEx { binding, predicate } => {
                event!(binding, predicate, EventSnapshotPolicy::AfterEntry)
            }
            EventPattern::LeaveField { binding, predicate } => {
                event!(binding, predicate, EventSnapshotPolicy::BeforeExit)
            }
            EventPattern::Discard { binding, predicate } => {
                event!(binding, predicate, EventSnapshotPolicy::BeforeExit)
            }
            EventPattern::Destroy { binding, predicate } => {
                event!(binding, predicate, EventSnapshotPolicy::BeforeExit)
            }
            EventPattern::Banish { .. }
            | EventPattern::EarthRitePayment { .. }
            | EventPattern::Play { .. }
            | EventPattern::Targeted { .. }
            | EventPattern::Resolve { .. }
            | EventPattern::Payment { .. }
            | EventPattern::Move { .. }
            | EventPattern::Draw { .. }
            | EventPattern::Damage { .. }
            | EventPattern::StatChange { .. }
            | EventPattern::LifeChange { .. }
            | EventPattern::CounterChange { .. }
            | EventPattern::Posture { .. }
            | EventPattern::Evolve { .. }
            | EventPattern::Attack { .. }
            | EventPattern::Phase { .. }
            | EventPattern::Race { .. }
            | EventPattern::DriveGained { .. }
            | EventPattern::DriveTrigger { .. }
            | EventPattern::UbActivated { .. }
            | EventPattern::Fused { .. }
            | EventPattern::Equip { .. }
            | EventPattern::Dice { .. }
            | EventPattern::DriveChecked { .. }
            | EventPattern::AnyOf { .. } => self.reject(),
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
            Effect::Replacement { .. }
            | Effect::Permission { .. }
            | Effect::RuleModifier { .. }
            | Effect::DeckRule { .. }
            | Effect::CostModifier { .. } => self.reject(),
        }
    }

    fn patch(&mut self, patch: &CharacteristicPatch, layer: ContinuousLayer) -> Checked {
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
