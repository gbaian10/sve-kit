use super::{BindingKind, Checked, Checker, WellFormedReason};
use crate::ir::{
    ActionPattern, Binding, CharacteristicPatch, ContinuousLayer, CostChange, Duration,
    DurationEnd, Effect, EffectReference, EffectTarget, ExecutionRef, Expr, InstallationSubjects,
    InstallationTime, Interval, PermissionSubject, Polarity, Selector, TimeAnchor, TimestampBasis,
    UsageScope,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Installed effects share the structural checker's closed environment."
)]
impl Checker {
    pub(super) fn modify(
        &mut self,
        subjects: &Selector,
        patch: &CharacteristicPatch,
        expires: &Duration,
        source: EffectReference,
    ) -> Checked {
        self.at("subjects", |checker| checker.selector(subjects))?;
        self.at("source", |checker| match source {
            EffectReference::CurrentInstallation {} => Ok(()),
            EffectReference::Bound { binding } => checker.reference(binding, BindingKind::Value),
        })?;
        // No consumption rule exists for characteristic modifications.
        if expires.usage.is_some() {
            return self.reject();
        }
        self.at("expires", |checker| checker.installed_duration(expires))?;
        self.at("patch", |checker| match patch {
            CharacteristicPatch::SetKind { .. } => Ok(()),
            CharacteristicPatch::Numeric { .. } | CharacteristicPatch::Combined { .. } => {
                checker.patch(patch, ContinuousLayer::Numeric)
            }
            CharacteristicPatch::GrantAbility { .. }
            | CharacteristicPatch::SuppressAbility { .. }
            | CharacteristicPatch::AddName { .. }
            | CharacteristicPatch::AddTrait { .. }
            | CharacteristicPatch::RemoveTrait { .. } => checker.reject(),
        })
    }

    pub(super) fn install(
        &mut self,
        subjects: &InstallationSubjects,
        capture: InstallationTime,
        effect: &Effect,
        duration: &Duration,
    ) -> Checked {
        if capture != InstallationTime::InstructionStart {
            return self.reject();
        }
        let InstallationSubjects::Objects { objects, affected } = subjects else {
            return self.reject();
        };
        self.selector(objects)?;
        // Duration captures the installation input, not the newly bound affected set.
        self.installed_duration(duration)?;
        self.isolated(|checker| {
            let scope = affected.key().0;
            if scope == checker.env.scope || !checker.scopes.insert(scope) {
                return Err(checker.error(WellFormedReason::DuplicateScope));
            }
            checker.env.scope = scope;
            checker.define(*affected, BindingKind::Value)?;
            checker.at("effect", |checker| {
                checker.installed_effect(effect, *affected, duration)
            })
        })
    }

    fn installed_effect(
        &mut self,
        effect: &Effect,
        affected: Binding<Selector>,
        duration: &Duration,
    ) -> Checked {
        // IR §14 defines use consumption only for cost modifiers; other effects fail closed.
        if duration.usage.is_some() && !matches!(effect, Effect::CostModifier { .. }) {
            return self.reject();
        }
        let targets = Selector::Bound {
            reference: affected,
        };
        match effect {
            Effect::CostModifier {
                subjects,
                change,
                duration: inner,
                usage: None,
            } if *subjects == targets && inner == duration => match change {
                CostChange::Set {
                    amount: Expr::Literal(amount),
                } if amount.0 < 0 => self.reject(),
                CostChange::Delta { amount } | CostChange::Set { amount } => self.integer(amount),
            },
            Effect::Continuous {
                id,
                source,
                target: EffectTarget::Dynamic { selector },
                condition,
                patch,
                layer: ContinuousLayer::Numeric,
                timestamp,
                active_zones,
                duration: inner,
            } if *selector == targets && inner == duration && active_zones.is_empty() => {
                if !self.effect_ids.insert(*id) {
                    return Err(self.error(WellFormedReason::InvalidEntry));
                }
                self.at("source", |checker| checker.ability_source(source))?;
                if let Some(condition) = condition {
                    self.at("condition", |checker| checker.boolean(condition))?;
                }
                self.at("patch", |checker| {
                    checker.patch(patch, ContinuousLayer::Numeric)
                })?;
                self.at("timestamp", |checker| match timestamp {
                    TimestampBasis::ProperPlay {
                        execution: ExecutionRef::Current {},
                    } => Ok(()),
                    TimestampBasis::ProperPlay {
                        execution: ExecutionRef::Bound { binding },
                    } => checker.reference(*binding, BindingKind::Value),
                    TimestampBasis::ZoneEntry { .. } => checker.reject(),
                })
            }
            Effect::Permission {
                subject: subject @ PermissionSubject::Objects { objects },
                action,
                polarity: Polarity::Forbid,
                condition,
                duration: inner,
                ..
            } if *objects == targets && inner == duration => {
                let (ActionPattern::Destroy {
                    objects: action_objects,
                    ..
                }
                | ActionPattern::Banish {
                    objects: action_objects,
                    ..
                }) = action
                else {
                    return self.reject();
                };
                if *action_objects != targets {
                    return self.reject();
                }
                self.at("action", |checker| {
                    checker.permission_action(subject, action)
                })?;
                self.at("condition", |checker| checker.boolean(condition))
            }
            Effect::Continuous { .. }
            | Effect::Permission { .. }
            | Effect::Replacement { .. }
            | Effect::RuleModifier { .. }
            | Effect::DeckRule { .. }
            | Effect::CostModifier { .. } => self.reject(),
        }
    }

    fn installed_duration(&mut self, duration: &Duration) -> Checked {
        match duration.anchor {
            TimeAnchor::Installation {} => {}
            TimeAnchor::Bound { binding } => self.reference(binding, BindingKind::Value)?,
        }
        match &duration.end {
            DurationEnd::CurrentTurnEnd {}
            | DurationEnd::EndResolution {}
            | DurationEnd::NoTemporalExpiry {} => {}
            DurationEnd::NextTurnEnd { player }
            | DurationEnd::NextPhaseEdge { player, .. }
            | DurationEnd::NextTurnEdge { player, .. } => self.player(player)?,
            DurationEnd::While { predicate } => self.boolean(predicate)?,
            DurationEnd::SourceZoneExit { source, .. } => self.object(source)?,
        }
        if let Some(usage) = &duration.usage {
            self.at("duration/usage/scope", |checker| {
                checker.installed_usage(&usage.scope)
            })?;
        }
        for (index, interval) in duration.intervals.iter().enumerate() {
            self.at(
                &format!("duration/intervals/{index}/player"),
                |checker| match interval {
                    Interval::DuringNextPhase { player, .. }
                    | Interval::DuringNextTurn { player } => checker.player(player),
                },
            )?;
        }
        Ok(())
    }

    fn installed_usage(&mut self, usage: &UsageScope) -> Checked {
        match usage {
            UsageScope::PerObject { object } => self.object(object),
            UsageScope::SharedPlayer { player, effect } => {
                self.player(player)?;
                match effect {
                    EffectReference::CurrentInstallation {} => Ok(()),
                    EffectReference::Bound { binding } => {
                        self.reference(*binding, BindingKind::Value)
                    }
                }
            }
        }
    }
}
