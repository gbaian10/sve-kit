use super::{
    Ability, AbilityId, Allocator, BodyContext, CardCoordinates, Located, LoweringError,
    ability_source, ast, numeric, source_span, unsupported,
};
use crate::ir::{
    AbilityKind, AbilityQualification, AbilitySource, Action, Binding, Bool, Cardinality,
    DamageKind, DamageRecipient, Duration, Effect, EffectDefinitionId, EventRewrite, Expr,
    InstallationSubjects, InstallationTime, Int, Nat, ObjectOperand, OncePerEvent, PlayerRef,
    PlayerRelation, PlayerSet, Predicate, ProposedEventPattern, RecipientPredicate,
    ReplacementScope, ScopeId, Selector, SourcePredicate, TextQuantifier, ZoneKind,
};

fn recipient(objects: Selector, binding: Binding<DamageRecipient>) -> RecipientPredicate {
    RecipientPredicate {
        binding,
        condition: Expr::Compute(Box::new(Predicate::RecipientIsFollower {
            recipient: DamageRecipient::Bound { binding },
            objects,
        })),
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Prevention lowering shares the existing body binding allocator."
)]
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

    fn prevention_effect(
        &mut self,
        shape: &ast::Prevent,
        fallback: Selector,
        source: &AbilitySource,
        condition: Expr<Bool>,
        duration: Duration,
        path: &str,
    ) -> Result<Effect, LoweringError> {
        if shape.next_only == Some(true) {
            return Err(unsupported(path));
        }
        let recipient = Some(self.prevention_recipients(shape.on.as_ref(), fallback, path)?);
        let damage_source = shape
            .source
            .as_ref()
            .map(|objects| {
                let objects = self.objects(objects, path)?;
                self.prevention_source(objects, path)
            })
            .transpose()?;
        let kind = match shape.kind {
            ast::PreventionKind::Any => DamageKind::Any,
            ast::PreventionKind::Combat => DamageKind::Combat,
            ast::PreventionKind::Ability => DamageKind::Ability,
        };
        self.damage_prevention(
            ProposedEventPattern::Damage {
                recipient,
                source: damage_source,
                kind,
            },
            source,
            condition,
            duration,
            path,
        )
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

    pub(super) fn prevent_apply(
        &mut self,
        installation: &ast::Apply,
        source: &AbilitySource,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let subjects = self.objects(&installation.on, path)?;
        let affected = Binding::new(1, self.card_allocator.scope(path)?);
        let fallback = Selector::Bound {
            reference: affected,
        };
        let duration = numeric::duration(installation.until.as_ref(), path)?;
        let condition = Expr::Literal(Bool(true));
        let effect = match &installation.effect {
            ast::InstalledEffect::Prevent(shape) => {
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
            ast::InstalledEffect::Stat(_)
            | ast::InstalledEffect::CostMod(_)
            | ast::InstalledEffect::Cannot(ast::Cannot::Stand(_)) => return Err(unsupported(path)),
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

#[expect(
    clippy::too_many_lines,
    reason = "Static prevention and numeric rewrites share one declaration construction."
)]
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
            !matches!(zones.as_slice(),[zone] if zone.kind()==ZoneKind::Field)
        }
    }) {
        return Err(unsupported(path));
    }
    let source = source_span(key, path, entry)?;
    let origin = ability_source(&source, None);
    let empty: ast::Body = super::decode(serde_json::json!({"do":[]}), path)?;
    let mut context = BodyContext::new(entry, scope, &empty, None, allocator);
    let condition = head
        .r#while
        .as_ref()
        .map(|condition| context.qualification(condition, &source, path))
        .transpose()?
        .unwrap_or(Expr::Literal(Bool(true)));
    let mut declarations = Vec::new();
    for declaration in &head.effects {
        let (on, damage_source, kind, rewrite) = match declaration {
            ast::StaticEffect::Prevent(shape) => {
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
                let kind = match shape.kind {
                    None | Some(ast::PreventionKind::Any) => DamageKind::Any,
                    Some(ast::PreventionKind::Combat) => DamageKind::Combat,
                    Some(ast::PreventionKind::Ability) => DamageKind::Ability,
                };
                (
                    shape.on.as_ref(),
                    shape.source.as_ref(),
                    kind,
                    EventRewrite::AddDamage {
                        amount: Expr::Literal(Int(shape.delta)),
                    },
                )
            }
            ast::StaticEffect::DamageCap(shape) => {
                if shape.threshold <= shape.set || shape.threshold > i64::MAX.unsigned_abs() {
                    return Err(unsupported(path));
                }
                (
                    shape.on.as_ref(),
                    None,
                    DamageKind::Any,
                    EventRewrite::CapDamage {
                        threshold: Expr::Literal(Nat(shape.threshold)),
                        replacement: Expr::Literal(Nat(shape.set)),
                    },
                )
            }
            ast::StaticEffect::Grant(_) => return Err(unsupported(path)),
        };
        let recipient = if on.is_none() && damage_source.is_some() {
            None
        } else {
            Some(context.prevention_recipients(on, numeric::holder(), path)?)
        };
        let damage_source = damage_source
            .map(|objects| {
                let objects = context.objects(objects, path)?;
                context.prevention_source(objects, path)
            })
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
