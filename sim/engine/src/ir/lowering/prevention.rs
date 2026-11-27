use super::{
    Ability, AbilityId, Allocator, BodyContext, CardCoordinates, Located, LoweringError,
    ability_source, ast, numeric, source_span, unsupported,
};
use crate::ir::phase_restrictions::{Restriction, affected_player};
use crate::ir::player_hand_limit;
use crate::ir::prevention::{holder_counter_step, opponent_controller, own_leader, projection};
use crate::ir::{
    AbilityKind, AbilityQualification, AbilitySource, Action, Binding, Bool, Cardinality,
    DamageKind, DamageRecipient, Duration, DurationEnd, Effect, EffectDefinitionId,
    EffectReference, EventRewrite, Expr, InstallationSubjects, InstallationTime, Int, Nat,
    ObjectOperand, OncePerEvent, Phase, PlayerRef, PlayerRelation, PlayerSet, Predicate,
    ProposedEventPattern, RecipientPredicate, ReplacementScope, ScopeId, Selector, SourcePredicate,
    TextQuantifier, UsageScope, UseLimit, ZoneKind,
};
use core::mem::swap;
use core::num::NonZeroU64;

fn recipient(objects: Selector, binding: Binding<DamageRecipient>) -> RecipientPredicate {
    RecipientPredicate {
        binding,
        condition: Expr::Compute(Box::new(Predicate::RecipientIsFollower {
            recipient: DamageRecipient::Bound { binding },
            objects,
        })),
    }
}

const fn damage_kind(kind: Option<&ast::PreventionKind>) -> DamageKind {
    match kind {
        None | Some(ast::PreventionKind::Any) => DamageKind::Any,
        Some(ast::PreventionKind::Attack) => DamageKind::Attack,
        Some(ast::PreventionKind::Combat) => DamageKind::Combat,
        Some(ast::PreventionKind::Ability) => DamageKind::Ability,
    }
}

impl BodyContext<'_> {
    fn prevention_recipients(
        &mut self,
        on: Option<&ast::Recipients>,
        fallback: Selector,
        path: &str,
    ) -> Result<RecipientPredicate, LoweringError> {
        let binding = Binding::new(1, self.card_allocator.scope(path)?);
        let players = match on {
            None => return Ok(recipient(fallback, binding)),
            Some(ast::Recipients::Target(name)) if name == "affected" => {
                return Ok(recipient(fallback, binding));
            }
            Some(ast::Recipients::Target(name)) if name == "this" => {
                return Ok(recipient(numeric::holder(), binding));
            }
            Some(ast::Recipients::Target(name)) => {
                return Ok(recipient(
                    self.objects(&ast::Objects::Bound(name.clone()), path)?,
                    binding,
                ));
            }
            Some(ast::Recipients::Field(query)) => {
                return Ok(recipient(
                    self.objects(&ast::Objects::Zone(query.clone()), path)?,
                    binding,
                ));
            }
            Some(ast::Recipients::Union { any_of }) => {
                let mut sets = Vec::new();
                for (index, limb) in any_of.iter().enumerate() {
                    let limb_path = format!("{path}/any_of/{index}");
                    let filter =
                        self.prevention_recipients(Some(limb), fallback.clone(), &limb_path)?;
                    let Expr::Compute(condition) = filter.condition else {
                        return Err(unsupported(path));
                    };
                    let Predicate::RecipientIsFollower { objects, .. } = *condition else {
                        return Err(unsupported(path));
                    };
                    sets.push(objects);
                }
                return Ok(recipient(Selector::Union { sets }, binding));
            }
            Some(ast::Recipients::Leader { leader }) => PlayerSet {
                relation: PlayerRelation::SelfPlayer,
                relative_to: match leader {
                    ast::LeaderSide::Controller => PlayerRef::Controller {},
                    ast::LeaderSide::Opponent => super::super::events::timing::opponent(),
                },
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
                text_quantifier: TextQuantifier::One,
            },
            _ => return Err(unsupported(path)),
        };
        Ok(RecipientPredicate {
            binding,
            condition: Expr::Compute(Box::new(Predicate::RecipientIsLeader {
                recipient: DamageRecipient::Bound { binding },
                players,
            })),
        })
    }

    fn prevention_source(
        &mut self,
        objects: Selector,
        path: &str,
    ) -> Result<SourcePredicate, LoweringError> {
        let binding = Binding::new(1, self.card_allocator.scope(path)?);
        Ok(SourcePredicate {
            binding,
            condition: Expr::Compute(Box::new(Predicate::MemberOf {
                object: ObjectOperand::Bound { binding },
                set: objects,
            })),
        })
    }

    fn prevention_filter(
        &mut self,
        source: &ast::PreventionSource,
        path: &str,
    ) -> Result<SourcePredicate, LoweringError> {
        match source {
            ast::PreventionSource::Objects(objects) => {
                let objects = self.objects(objects, path)?;
                self.prevention_source(objects, path)
            }
            ast::PreventionSource::Characteristics(shape) => {
                let trait_id = super::super::TraitId(shape.r#trait.clone());
                if trait_id.0.is_empty() {
                    return Err(unsupported(path));
                }
                Ok(super::super::prevention::source_filter::Filter {
                    own: matches!(shape.who, ast::LeaderSide::Controller),
                    kind: super::search::card_kind(&shape.kind),
                    trait_id: &trait_id,
                }
                .predicate(Binding::new(1, self.card_allocator.scope(path)?)))
            }
            ast::PreventionSource::Controller(shape)
                if matches!(shape.who, ast::LeaderSide::Opponent) =>
            {
                Ok(opponent_controller(Binding::new(
                    1,
                    self.card_allocator.scope(path)?,
                )))
            }
            ast::PreventionSource::Controller(_) => Err(unsupported(path)),
        }
    }

    fn prevention_effect(
        &mut self,
        shape: &ast::Prevent,
        fallback: Selector,
        source: &AbilitySource,
        condition: Expr<Bool>,
        duration: Duration,
        path: &str,
    ) -> Result<Effect, LoweringError> {
        let recipient = Some(self.prevention_recipients(shape.on.as_ref(), fallback, path)?);
        let damage_source = shape
            .source
            .as_ref()
            .map(|filter| self.prevention_filter(filter, path))
            .transpose()?;
        let kind = damage_kind(Some(&shape.kind));
        let effect = self.damage_prevention(
            ProposedEventPattern::Damage {
                recipient,
                source: damage_source,
                kind,
            },
            source,
            condition,
            duration,
            path,
        )?;
        limit_next(effect, shape.next_only == Some(true), path)
    }

    fn damage_prevention(
        &mut self,
        pattern: ProposedEventPattern,
        source: &AbilitySource,
        condition: Expr<Bool>,
        duration: Duration,
        path: &str,
    ) -> Result<Effect, LoweringError> {
        Ok(Effect::Replacement {
            id: EffectDefinitionId(self.card_allocator.id(path)?),
            source: source.clone(),
            pattern,
            condition,
            rewrite: EventRewrite::PreventDamage {
                recipient: None,
                source: None,
                kind: DamageKind::Any,
            },
            scope: ReplacementScope::General,
            affected_player: PlayerRef::ProposedDamageRecipient {},
            once_per_event: OncePerEvent::Enforced,
            usage: None,
            prospective_chars: None,
            duration,
        })
    }

    pub(super) fn player_effect(
        &mut self,
        value: &ast::PlayerEffect,
        source: &AbilitySource,
        path: &str,
    ) -> Result<Action, LoweringError> {
        if matches!(
            value.effect,
            ast::InstalledEffect::HandLimit(ast::HandLimit::Unlimited)
        ) {
            if !matches!(value.who, ast::Side::Controller)
                || !matches!(value.until, Some(ast::Until::Game))
            {
                return Err(unsupported(path));
            }
            let (players, effect, duration) = player_hand_limit::modifier(source.clone());
            return Ok(Action::InstallEffect {
                subjects: InstallationSubjects::Players {
                    players,
                    affected: Binding::new(1, self.card_allocator.scope(path)?),
                },
                capture: InstallationTime::InstructionStart,
                effect: Box::new(effect),
                duration,
            });
        }
        if let ast::InstalledEffect::Cannot(ast::Cannot::RulePhase(rule)) = &value.effect {
            return self.rule_phase_permission(value, rule.what, path);
        }
        if matches!(value.effect, ast::InstalledEffect::PlayPermission(_)) {
            return self.player_play_permission(value, path);
        }
        if matches!(
            value.effect,
            ast::InstalledEffect::Prevent(_)
                | ast::InstalledEffect::DamageMod(_)
                | ast::InstalledEffect::DamageCap(_)
        ) {
            self.player_prevention(value, source, path)
        } else {
            self.player_cost(value, path)
        }
    }

    fn rule_phase_permission(
        &mut self,
        value: &ast::PlayerEffect,
        rule: ast::RulePhaseAction,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let Some(ast::Until::DuringNextPhase { during_next_phase }) = &value.until else {
            return Err(unsupported(path));
        };
        let player = match (&value.who, &during_next_phase.who) {
            (ast::Side::Controller, ast::StandPlayer::Controller) => PlayerRef::Controller {},
            (ast::Side::Opponent, ast::StandPlayer::Opponent) => {
                super::super::events::timing::opponent()
            }
            _ => return Err(unsupported(path)),
        };
        let phase = match during_next_phase.phase {
            ast::RuleWindowPhase::Start => Phase::Start,
            ast::RuleWindowPhase::Main => Phase::Main,
        };
        let rule = match rule {
            ast::RulePhaseAction::NormalDraw => Restriction::Draw,
            ast::RulePhaseAction::NormalMaxPpGain => Restriction::MaximumPp,
            ast::RulePhaseAction::PlayFollower => Restriction::FollowerPlay,
        };
        if phase != rule.phase() {
            return Err(unsupported(path));
        }
        let players = affected_player(player.clone());
        Ok(Action::InstallEffect {
            subjects: InstallationSubjects::Players {
                players: players.clone(),
                affected: Binding::new(1, self.card_allocator.scope(path)?),
            },
            capture: InstallationTime::InstructionStart,
            effect: Box::new(rule.permission(players, player.clone())),
            duration: rule.duration(player),
        })
    }

    pub(super) fn player_prevention(
        &mut self,
        value: &ast::PlayerEffect,
        source: &AbilitySource,
        path: &str,
    ) -> Result<Action, LoweringError> {
        if !matches!(value.who, ast::Side::Controller) {
            return Err(unsupported(path));
        }
        let (rewrite, on, kind, next, damage_source) = match &value.effect {
            ast::InstalledEffect::Prevent(shape) => (
                EventRewrite::PreventDamage {
                    recipient: None,
                    source: None,
                    kind: DamageKind::Any,
                },
                shape.on.as_ref(),
                Some(&shape.kind),
                shape.next_only,
                shape.source.as_ref(),
            ),
            ast::InstalledEffect::DamageMod(shape)
                if shape.literal_delta().is_some_and(|delta| delta < 0) =>
            {
                (
                    EventRewrite::AddDamage {
                        amount: Expr::Literal(Int(shape.literal_delta().unwrap_or_default())),
                    },
                    shape.on.as_ref(),
                    shape.kind.as_ref(),
                    shape.next_only,
                    shape.source.as_ref(),
                )
            }
            // 「このターン、自分のリーダーが受けるN以上のダメージをMにする」 (BP05-101).
            ast::InstalledEffect::DamageCap(shape) => (
                damage_cap(shape, path)?,
                shape.on.as_ref(),
                None,
                None,
                None,
            ),
            ast::InstalledEffect::DamageMod(_)
            | ast::InstalledEffect::PlayPermission(_)
            | ast::InstalledEffect::LoseAbilities(_)
            | ast::InstalledEffect::AddTrait(_)
            | ast::InstalledEffect::Stat(_)
            | ast::InstalledEffect::CostMod(_)
            | ast::InstalledEffect::EvolveCostMod(_)
            | ast::InstalledEffect::HandLimit(_)
            | ast::InstalledEffect::Cannot(_) => return Err(unsupported(path)),
        };
        let limited = next == Some(true);
        let kind = damage_kind(kind);
        if damage_source.is_some()
            || (limited && kind != DamageKind::Any)
            || (!matches!(rewrite, EventRewrite::PreventDamage { .. }) && kind != DamageKind::Any)
            || !matches!(
                on,
                None | Some(ast::Recipients::Leader {
                    leader: ast::LeaderSide::Controller
                })
            )
        {
            return Err(unsupported(path));
        }
        let duration = numeric::duration(value.until.as_ref(), path)?;
        if duration.end != (DurationEnd::CurrentTurnEnd {}) || !duration.intervals.is_empty() {
            return Err(unsupported(path));
        }
        let binding = Binding::new(1, self.card_allocator.scope(path)?);
        let players = own_leader();
        let recipient = RecipientPredicate {
            binding,
            condition: Expr::Compute(Box::new(Predicate::RecipientIsLeader {
                recipient: DamageRecipient::Bound { binding },
                players: players.clone(),
            })),
        };
        let mut effect = self.damage_prevention(
            ProposedEventPattern::Damage {
                recipient: Some(recipient),
                source: None,
                kind,
            },
            source,
            Expr::Literal(Bool(true)),
            duration.clone(),
            path,
        )?;
        if let Effect::Replacement {
            rewrite: current, ..
        } = &mut effect
        {
            *current = rewrite;
        }
        let effect = limit_next(effect, limited, path)?;
        Ok(Action::InstallEffect {
            subjects: InstallationSubjects::Players {
                players,
                affected: Binding::new(1, self.card_allocator.scope(path)?),
            },
            capture: InstallationTime::InstructionStart,
            effect: Box::new(effect),
            duration,
        })
    }

    fn static_damage_modifier(
        &mut self,
        modifier: &ast::DamageModifier,
        path: &str,
    ) -> Result<EventRewrite, LoweringError> {
        match (&modifier.delta, modifier.multiply) {
            (None, Some(multiplier)) if multiplier > 0 && i64::try_from(multiplier).is_ok() => {
                Ok(EventRewrite::MultiplyDamage {
                    numerator: Nat(multiplier),
                    denominator: super::super::PositiveInt(NonZeroU64::MIN),
                    rounding: super::super::Rounding::Exact,
                })
            }
            (Some(delta), None) => {
                let amount = self.number(delta, path, &mut super::BTreeSet::new())?;
                if !matches!(amount, Expr::Literal(_)) && holder_counter_step(&amount).is_none() {
                    return Err(unsupported(path));
                }
                Ok(EventRewrite::AddDamage { amount })
            }
            _ => Err(unsupported(path)),
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Every object prevention, damage modifier and damage cap shares one installation boundary."
    )]
    pub(super) fn prevent_apply(
        &mut self,
        installation: &ast::Apply,
        source: &AbilitySource,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let limited = match &installation.effect {
            ast::InstalledEffect::Prevent(shape) => shape.next_only == Some(true),
            ast::InstalledEffect::DamageMod(shape) => shape.next_only == Some(true),
            ast::InstalledEffect::DamageCap(_)
            | ast::InstalledEffect::PlayPermission(_)
            | ast::InstalledEffect::LoseAbilities(_)
            | ast::InstalledEffect::AddTrait(_)
            | ast::InstalledEffect::Stat(_)
            | ast::InstalledEffect::CostMod(_)
            | ast::InstalledEffect::EvolveCostMod(_)
            | ast::InstalledEffect::HandLimit(_)
            | ast::InstalledEffect::Cannot(_) => false,
        };
        let subjects = if limited {
            match &installation.on {
                ast::Objects::This(_) => numeric::holder(),
                ast::Objects::Bound(name) => {
                    let (_, selection) = self.selected_parameter(name, path)?;
                    Selector::Objects {
                        objects: vec![ObjectOperand::Selected { selection }],
                    }
                }
                ast::Objects::Role { .. }
                | ast::Objects::Zone(_)
                | ast::Objects::Follow { .. }
                | ast::Objects::EventHolder { .. } => {
                    return Err(unsupported(path));
                }
            }
        } else {
            self.objects(&installation.on, path)?
        };
        let affected = Binding::new(1, self.card_allocator.scope(path)?);
        let fallback = Selector::Bound {
            reference: affected,
        };
        let duration = numeric::duration(installation.until.as_ref(), path)?;
        let condition = Expr::Literal(Bool(true));
        let effect = match &installation.effect {
            ast::InstalledEffect::Prevent(shape) => {
                if matches!(shape.source, Some(ast::PreventionSource::Controller(_))) {
                    return Err(unsupported(path));
                }
                self.prevention_effect(shape, fallback, source, condition, duration.clone(), path)?
            }
            ast::InstalledEffect::Cannot(ast::Cannot::DealDamage(shape)) => {
                let objects = match shape.on.as_ref() {
                    None => fallback,
                    Some(ast::Objects::Bound(name)) if name == "affected" => fallback,
                    Some(on) => self.objects(on, path)?,
                };
                let damage_source = Some(self.prevention_source(objects, path)?);
                self.damage_prevention(
                    ProposedEventPattern::Damage {
                        recipient: None,
                        source: damage_source,
                        kind: DamageKind::Any,
                    },
                    source,
                    condition,
                    duration.clone(),
                    path,
                )?
            }
            // 「このターンと次の相手のターン中、それは『これが受けるN以上のダメージをMにする』を持つ」.
            ast::InstalledEffect::DamageCap(shape) if shape.on.is_none() => {
                let recipient = self.prevention_recipients(None, fallback, path)?;
                let mut effect = self.damage_prevention(
                    ProposedEventPattern::Damage {
                        recipient: Some(recipient),
                        source: None,
                        kind: DamageKind::Any,
                    },
                    source,
                    condition,
                    duration.clone(),
                    path,
                )?;
                if let Effect::Replacement { rewrite, .. } = &mut effect {
                    *rewrite = damage_cap(shape, path)?;
                }
                effect
            }
            ast::InstalledEffect::DamageMod(shape) => {
                let Some(delta @ 1..) = shape.literal_delta() else {
                    return Err(unsupported(path));
                };
                if shape.on.is_some()
                    || shape.source.is_some()
                    || !matches!(shape.kind, None | Some(ast::PreventionKind::Any))
                    || shape.next_only == Some(true)
                    || duration.end != (DurationEnd::CurrentTurnEnd {})
                    || !duration.intervals.is_empty()
                {
                    return Err(unsupported(path));
                }
                let recipient = self.prevention_recipients(None, fallback, path)?;
                let mut effect = self.damage_prevention(
                    ProposedEventPattern::Damage {
                        recipient: Some(recipient),
                        source: None,
                        kind: DamageKind::Any,
                    },
                    source,
                    condition,
                    duration.clone(),
                    path,
                )?;
                if let Effect::Replacement { rewrite, .. } = &mut effect {
                    *rewrite = EventRewrite::AddDamage {
                        amount: Expr::Literal(Int(delta)),
                    };
                }
                effect
            }
            ast::InstalledEffect::DamageCap(_)
            | ast::InstalledEffect::PlayPermission(_)
            | ast::InstalledEffect::LoseAbilities(_)
            | ast::InstalledEffect::AddTrait(_)
            | ast::InstalledEffect::Stat(_)
            | ast::InstalledEffect::CostMod(_)
            | ast::InstalledEffect::EvolveCostMod(_)
            | ast::InstalledEffect::HandLimit(_)
            | ast::InstalledEffect::Cannot(
                ast::Cannot::Draw(_)
                | ast::Cannot::RulePhase(_)
                | ast::Cannot::Player(_)
                | ast::Cannot::Stand(_)
                | ast::Cannot::Attack(_)
                | ast::Cannot::Removal(_)
                | ast::Cannot::PlayAbility(_),
            ) => {
                return Err(unsupported(path));
            }
        };
        Ok(Action::InstallEffect {
            subjects: InstallationSubjects::Objects {
                objects: subjects,
                affected,
            },
            capture: InstallationTime::InstructionStart,
            effect: Box::new(effect),
            duration,
        })
    }
}

pub(super) fn lower(
    allocator: &mut Allocator,
    key: &CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
) -> Result<Located<Ability>, LoweringError> {
    let source = source_span(key, path, entry)?;
    let head = entry.r#static.as_ref().ok_or_else(|| unsupported(path))?;
    lower_at_source(
        allocator,
        path,
        entry,
        scope,
        source,
        &head.effects,
        &mut Allocator::default(),
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "Static prevention and numeric rewrites share one declaration construction."
)]
pub(super) fn lower_at_source(
    allocator: &mut Allocator,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
    source: super::SourceSpan,
    effects: &[ast::StaticEffect],
    bindings: &mut Allocator,
) -> Result<Located<Ability>, LoweringError> {
    let head = entry.r#static.as_ref().ok_or_else(|| unsupported(path))?;
    let zone = match &head.zone {
        None => ZoneKind::Field,
        Some(zones) => zones.single().ok_or_else(|| unsupported(path))?,
    };
    if !matches!(zone, ZoneKind::Field | ZoneKind::Equipment)
        || (zone == ZoneKind::Equipment && head.r#while.is_some())
    {
        return Err(unsupported(path));
    }
    let origin = ability_source(&source, None);
    let empty: ast::Body = super::decode(serde_json::json!({"do":[]}), path)?;
    let mut context = BodyContext::new(entry, scope, &empty, None, allocator);
    swap(&mut context.allocator, bindings);
    let condition = head
        .r#while
        .as_ref()
        .map(|condition| context.qualification(condition, path))
        .transpose()?
        .unwrap_or(Expr::Literal(Bool(true)));
    let mut declarations = Vec::new();
    for declaration in effects {
        let (on, damage_source, kind, rewrite) = match declaration {
            ast::StaticEffect::Cannot(ast::Cannot::Removal(shape)) => {
                if head.r#while.is_some() {
                    return Err(unsupported(path));
                }
                declarations.push(super::attack_restrictions::removal(shape, path)?);
                continue;
            }
            ast::StaticEffect::If(shape) => {
                if head.r#while.is_some()
                    || !head.effects.iter().any(|effect| {
                        matches!(effect, ast::StaticEffect::Cannot(ast::Cannot::Removal(_)))
                    })
                {
                    return Err(unsupported(path));
                }
                let [ast::StaticConditionalEffect::Prevent(prevent)] = &shape.then else {
                    return Err(unsupported(path));
                };
                let branch_condition = context.qualification(&shape.cond, path)?;
                declarations.push(context.prevention_effect(
                    prevent,
                    numeric::holder(),
                    &origin,
                    branch_condition,
                    numeric::duration(None, path)?,
                    path,
                )?);
                continue;
            }
            ast::StaticEffect::Prevent(shape) => {
                if shape.next_only == Some(true) {
                    return Err(unsupported(path));
                }
                declarations.push(context.prevention_effect(
                    shape,
                    numeric::holder(),
                    &origin,
                    condition.clone(),
                    numeric::duration(None, path)?,
                    path,
                )?);
                continue;
            }
            ast::StaticEffect::DamageMod(shape) => {
                if shape.next_only == Some(true) {
                    return Err(unsupported(path));
                }
                (
                    shape.on.as_ref(),
                    shape.source.as_ref(),
                    damage_kind(shape.kind.as_ref()),
                    context.static_damage_modifier(shape, path)?,
                )
            }
            ast::StaticEffect::DamageCap(shape) => (
                shape.on.as_ref(),
                None,
                DamageKind::Any,
                damage_cap(shape, path)?,
            ),
            ast::StaticEffect::BanishInstead(_)
            | ast::StaticEffect::MustTarget(_)
            | ast::StaticEffect::EntersActed(_)
            | ast::StaticEffect::ExtraTrigger(_)
            | ast::StaticEffect::MakeType(_)
            | ast::StaticEffect::Reroll(_)
            | ast::StaticEffect::ChoiceAny(_)
            | ast::StaticEffect::RuleOverride(_)
            | ast::StaticEffect::UnlimitedEvolve(_)
            | ast::StaticEffect::AlsoNamed(_)
            | ast::StaticEffect::Grant(_)
            | ast::StaticEffect::NoTrigger(_)
            | ast::StaticEffect::Cannot(_)
            | ast::StaticEffect::CostMod(_)
            | ast::StaticEffect::EvolveCostMod(_)
            | ast::StaticEffect::IgnoreWard(_)
            | ast::StaticEffect::MustAttack(_) => {
                return Err(unsupported(path));
            }
        };
        let recipient = if on.is_none()
            && matches!(
                damage_source,
                Some(ast::PreventionSource::Objects(_) | ast::PreventionSource::Characteristics(_))
            ) {
            None
        } else {
            Some(context.prevention_recipients(on, numeric::holder(), path)?)
        };
        let damage_source = damage_source
            .map(|filter| context.prevention_filter(filter, path))
            .transpose()?;
        let mut effect = context.damage_prevention(
            ProposedEventPattern::Damage {
                recipient,
                source: damage_source,
                kind,
            },
            &origin,
            condition.clone(),
            numeric::duration(None, path)?,
            path,
        )?;
        if let Effect::Replacement {
            rewrite: output, ..
        } = &mut effect
        {
            *output = rewrite;
        }
        declarations.push(effect);
    }
    swap(&mut context.allocator, bindings);
    Ok(Located {
        source,
        node: Ability {
            id: AbilityId(scope.0),
            scope,
            source: origin,
            kind: AbilityKind::Static,
            qualification: AbilityQualification::Ordinary,
            active_zones: vec![zone],
            event_captures: vec![],
            play: None,
            trigger: None,
            body: None,
            declarations,
        },
    })
}

/// 「N以上のダメージをMにする」: each single damage of at least N becomes M (Q760, Q1067).
fn damage_cap(shape: &ast::DamageCap, path: &str) -> Result<EventRewrite, LoweringError> {
    if shape.threshold <= shape.set || shape.threshold > i64::MAX.unsigned_abs() {
        return Err(unsupported(path));
    }
    Ok(EventRewrite::CapDamage {
        threshold: Expr::Literal(Nat(shape.threshold)),
        replacement: Expr::Literal(Nat(shape.set)),
    })
}

fn limit_next(mut effect: Effect, limited: bool, path: &str) -> Result<Effect, LoweringError> {
    if limited {
        let Effect::Replacement {
            usage, duration, ..
        } = &mut effect
        else {
            return Err(unsupported(path));
        };
        if duration.end != (DurationEnd::CurrentTurnEnd {}) || !duration.intervals.is_empty() {
            return Err(unsupported(path));
        }
        *usage = Some(UseLimit {
            count: Nat(1),
            scope: UsageScope::SharedPlayer {
                player: PlayerRef::Controller {},
                effect: EffectReference::CurrentInstallation {},
            },
        });
        if projection(&effect).is_none() {
            return Err(unsupported(path));
        }
    }
    Ok(effect)
}
