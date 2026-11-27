use super::{
    Ability, AbilityId, Allocator, BodyContext, CardCoordinates, Located, LoweringError,
    SourceSpan, ability_source, ast, numeric, source_span, unsupported,
};
use crate::ir::attack_restrictions::owner_field;
use crate::ir::{
    AbilityKind, AbilityQualification, Action, ActionPattern, Binding, Bool, BoolRead, Duration,
    DurationEnd, Effect, EffectDefinitionId, Expr, Interval, ObjectOperand, PermissionSubject,
    Polarity, RecipientSet, RuleId, ScopeId, Selector, TimeAnchor, ZoneKind,
};

pub(super) fn lower(
    allocator: &mut Allocator,
    key: &CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
) -> Result<Located<Ability>, LoweringError> {
    let head = entry.r#static.as_ref().ok_or_else(|| unsupported(path))?;
    if head.zone.as_ref().is_some_and(|zones| match zones {
        ast::ActivationZones::One(zone) => zone.kind() != ZoneKind::Field,
        ast::ActivationZones::Many(zones) => {
            !matches!(zones.as_slice(), [zone] if zone.kind() == ZoneKind::Field)
        }
    }) {
        return Err(unsupported(path));
    }
    if head.effects.iter().any(|effect| {
        matches!(
            effect,
            ast::StaticEffect::Prevent(_) | ast::StaticEffect::If(_)
        )
    }) {
        return super::prevention::lower(allocator, key, path, entry, scope);
    }
    let source = source_span(key, path, entry)?;
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

pub(super) fn lower_at_source(
    allocator: &mut Allocator,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
    source: SourceSpan,
    effects: &[ast::StaticEffect],
    _bindings: &mut Allocator,
) -> Result<Located<Ability>, LoweringError> {
    let head = entry.r#static.as_ref().ok_or_else(|| unsupported(path))?;
    if head
        .zone
        .as_ref()
        .is_some_and(|zones| zones.single() != Some(ZoneKind::Field))
    {
        return Err(unsupported(path));
    }
    let origin = ability_source(&source, None);
    let empty: ast::Body = super::decode(serde_json::json!({"do":[]}), path)?;
    let mut context = BodyContext::new(entry, scope, &empty, None, allocator);
    let condition = head
        .r#while
        .as_ref()
        .map(|condition| context.qualification(condition, path))
        .transpose()?
        .unwrap_or(Expr::Literal(Bool(true)));
    let mut declarations = Vec::new();
    for effect in effects {
        if let ast::StaticEffect::Cannot(ast::Cannot::Removal(shape)) = effect {
            if head.r#while.is_some() {
                return Err(unsupported(path));
            }
            declarations.push(removal(shape, path)?);
            continue;
        }
        if let ast::StaticEffect::Cannot(ast::Cannot::Stand(restriction)) = effect {
            if head.r#while.is_some() {
                return Err(unsupported(path));
            }
            declarations.push(super::posture::static_no_stand(restriction, path)?);
            continue;
        }
        if let ast::StaticEffect::Cannot(ast::Cannot::Player(shape)) = effect {
            if head.r#while.is_some() {
                return Err(unsupported(path));
            }
            declarations.push(super::victory::permission(shape, path)?);
            continue;
        }
        declarations.push(permission(&mut context, effect, &condition, path)?);
    }
    Ok(Located {
        source,
        node: Ability {
            id: AbilityId(scope.0),
            scope,
            source: origin,
            kind: AbilityKind::Static,
            qualification: AbilityQualification::Ordinary,
            active_zones: vec![ZoneKind::Field],
            event_captures: vec![],
            play: None,
            trigger: None,
            body: None,
            declarations,
        },
    })
}

fn permission(
    context: &mut BodyContext<'_>,
    effect: &ast::StaticEffect,
    condition: &Expr<Bool>,
    path: &str,
) -> Result<Effect, LoweringError> {
    let (attacker, action, polarity, rule) = match effect {
        ast::StaticEffect::MustAttack(shape) => {
            let attacker = context.objects(&shape.on, path)?;
            (
                attacker.clone(),
                ActionPattern::Attack {
                    attacker,
                    target: None,
                },
                Polarity::Require,
                RuleId(133),
            )
        }
        ast::StaticEffect::IgnoreWard(shape) => {
            if !matches!(shape.on, ast::Objects::This(_)) {
                return Err(unsupported(path));
            }
            let attacker = numeric::holder();
            (
                attacker.clone(),
                ActionPattern::IgnoreWard { attacker },
                Polarity::Allow,
                RuleId(128),
            )
        }
        ast::StaticEffect::Cannot(ast::Cannot::Attack(shape)) => {
            let ast::AttackAction::Attack = shape.what;
            let mut attacker = shape
                .on
                .as_ref()
                .map_or_else(|| Ok(numeric::holder()), |on| context.objects(on, path))?;
            let target = match &shape.target {
                Some(ast::AttackRecipients::OwnerField {
                    who:
                        ast::AttackOwner {
                            owner_of: ast::This::This,
                        },
                    kind: ast::AttackableKind::Follower,
                    zone: None | Some(ast::FieldZone::Field),
                    ..
                }) if attacker == numeric::holder()
                    && shape.if_can_attack.is_none()
                    && *condition == Expr::Literal(Bool(true)) =>
                {
                    Some(owner_field(Binding::new(
                        context.allocator.id(path)?,
                        context.scope,
                    )))
                }
                Some(ast::AttackRecipients::OwnerField { .. }) => return Err(unsupported(path)),
                Some(ast::AttackRecipients::Other(target)) => {
                    Some(context.recipients(target, path)?)
                }
                None => None,
            };
            if shape.if_can_attack.is_some() {
                attacker = conditional_attacker(context, attacker, target.as_ref(), path)?;
            }
            (
                attacker.clone(),
                ActionPattern::Attack { attacker, target },
                Polarity::Forbid,
                RuleId(133),
            )
        }
        ast::StaticEffect::BanishInstead(_)
        | ast::StaticEffect::MustTarget(_)
        | ast::StaticEffect::EntersActed(_)
        | ast::StaticEffect::ExtraTrigger(_)
        | ast::StaticEffect::MakeType(_)
        | ast::StaticEffect::Reroll(_)
        | ast::StaticEffect::UnlimitedEvolve(_)
        | ast::StaticEffect::ChoiceAny(_)
        | ast::StaticEffect::RuleOverride(_)
        | ast::StaticEffect::AlsoNamed(_)
        | ast::StaticEffect::If(_)
        | ast::StaticEffect::EvolveCostMod(_)
        | ast::StaticEffect::CostMod(_)
        | ast::StaticEffect::Grant(_)
        | ast::StaticEffect::Cannot(_)
        | ast::StaticEffect::Prevent(_)
        | ast::StaticEffect::DamageMod(_)
        | ast::StaticEffect::DamageCap(_)
        | ast::StaticEffect::NoTrigger(_) => return Err(unsupported(path)),
    };
    Ok(Effect::Permission {
        subject: PermissionSubject::Objects { objects: attacker },
        action,
        polarity,
        condition: condition.clone(),
        duration: numeric::duration(None, path)?,
        rule,
    })
}

fn conditional_attacker(
    context: &mut BodyContext<'_>,
    attacker: Selector,
    target: Option<&RecipientSet>,
    path: &str,
) -> Result<Selector, LoweringError> {
    if !matches!(target, Some(RecipientSet::Leaders { .. })) {
        return Err(unsupported(path));
    }
    let binding = Binding::new(context.allocator.id(path)?, context.scope);
    Ok(Selector::Filter {
        input: Box::new(attacker),
        binding,
        predicate: Box::new(Expr::Live(Box::new(BoolRead::LegalAttack {
            attacker: ObjectOperand::Bound { binding },
            ignore_querying_clause: EffectDefinitionId(context.card_allocator.id(path)?),
        }))),
    })
}

impl BodyContext<'_> {
    pub(super) fn no_attack(
        &mut self,
        installation: &ast::Apply,
        path: &str,
    ) -> Result<Action, LoweringError> {
        use crate::ir::{Binding, InstallationSubjects, InstallationTime, Selector};
        let ast::InstalledEffect::Cannot(ast::Cannot::Attack(shape)) = &installation.effect else {
            return Err(unsupported(path));
        };
        if shape.if_can_attack.is_some() {
            return Err(unsupported(path));
        }
        // A field entry's result names its successful members, as for numeric modifications.
        let objects = if let ast::Objects::Follow { follow } = &installation.on {
            self.followed(follow, path)?
        } else {
            self.objects(&installation.on, path)?
        };
        let affected = Binding::new(1, self.card_allocator.scope(path)?);
        let targets = Selector::Bound {
            reference: affected,
        };
        if shape
            .on
            .as_ref()
            .is_some_and(|on| !matches!(on, ast::Objects::Bound(name) if name == "affected"))
        {
            return Err(unsupported(path));
        }
        let target = match &shape.target {
            Some(ast::AttackRecipients::OwnerField { .. }) => return Err(unsupported(path)),
            Some(ast::AttackRecipients::Other(target)) => Some(self.recipients(target, path)?),
            None => None,
        };
        let duration = match &installation.until {
            Some(ast::Until::DuringNextTurn { during_next_turn }) => {
                let player = super::posture::player(during_next_turn.who);
                Duration {
                    end: DurationEnd::NextTurnEnd {
                        player: player.clone(),
                    },
                    anchor: TimeAnchor::Installation {},
                    usage: None,
                    intervals: vec![Interval::DuringNextTurn { player }],
                }
            }
            _ => numeric::duration(installation.until.as_ref(), path)?,
        };
        Ok(Action::InstallEffect {
            subjects: InstallationSubjects::Objects { objects, affected },
            capture: InstallationTime::InstructionStart,
            effect: Box::new(Effect::Permission {
                subject: PermissionSubject::Objects {
                    objects: targets.clone(),
                },
                action: ActionPattern::Attack {
                    attacker: targets,
                    target,
                },
                polarity: Polarity::Forbid,
                condition: Expr::Literal(Bool(true)),
                duration: duration.clone(),
                rule: RuleId(133),
            }),
            duration,
        })
    }
}

pub(super) fn removal(shape: &ast::CannotRemoval, path: &str) -> Result<Effect, LoweringError> {
    use crate::ir::{CausePattern, EffectSource};
    if shape
        .on
        .as_ref()
        .is_some_and(|on| !matches!(on, ast::Objects::This(ast::This::This)))
    {
        return Err(unsupported(path));
    }
    let objects = numeric::holder();
    let action = match (&shape.what, &shape.by) {
        (ast::RemovalAction::Destroy, Some(ast::RemovalBy::Ability)) => ActionPattern::Destroy {
            objects: objects.clone(),
            cause: CausePattern::Effect {
                source: EffectSource::Any {},
            },
        },
        (ast::RemovalAction::Banish, None) => ActionPattern::Banish {
            objects: objects.clone(),
            cause: CausePattern::Any {},
        },
        _ => return Err(unsupported(path)),
    };
    Ok(Effect::Permission {
        subject: PermissionSubject::Objects { objects },
        action,
        polarity: Polarity::Forbid,
        condition: Expr::Literal(Bool(true)),
        duration: numeric::duration(None, path)?,
        rule: RuleId(133),
    })
}
