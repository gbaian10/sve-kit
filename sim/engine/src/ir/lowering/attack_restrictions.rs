use super::{
    Ability, AbilityId, Allocator, BodyContext, CardCoordinates, Located, LoweringError,
    ability_source, ast, numeric, source_span, unsupported,
};
use crate::ir::{
    AbilityKind, AbilityQualification, Action, ActionPattern, Bool, Duration, DurationEnd, Effect,
    Expr, Interval, PermissionSubject, Polarity, RuleId, ScopeId, TimeAnchor, ZoneKind,
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
    for effect in &head.effects {
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
            let attacker = shape
                .on
                .as_ref()
                .map_or_else(|| Ok(numeric::holder()), |on| context.objects(on, path))?;
            let target = shape
                .target
                .as_ref()
                .map(|target| context.recipients(target, path))
                .transpose()?;
            (
                attacker.clone(),
                ActionPattern::Attack { attacker, target },
                Polarity::Forbid,
                RuleId(133),
            )
        }
        ast::StaticEffect::AlsoNamed(_)
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

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Attack permission installation shares body bindings."
)]
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
        let objects = self.objects(&installation.on, path)?;
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
        let duration = match &installation.until {
            Some(ast::Until::ThisAndNextOppTurn) => return Err(unsupported(path)),
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
                    target: shape
                        .target
                        .as_ref()
                        .map(|target| self.recipients(target, path))
                        .transpose()?,
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
