use super::{
    AbilitySource, Binding, Bool, CardKind, Cardinality, CharacteristicView, DamageKind,
    DamageRecipient, Duration, Effect, EffectReference, EventRewrite, Expr, KindRead, Nat,
    ObjectOperand, ObjectRef, ObjectView, OncePerEvent, PlayerRef, PlayerRelation, PlayerSet,
    Predicate, ProposedEventPattern, RecipientPredicate, ReplacementScope, SelectionView, Selector,
    SourcePredicate, TextQuantifier, UsageScope, UseLimit, ZoneKind,
};

pub(crate) enum Recipients<'effect> {
    Objects(&'effect Selector),
    Leaders(&'effect PlayerSet),
}

#[derive(Clone, Copy)]
pub(crate) enum DamageSource<'effect> {
    Objects(&'effect Selector),
    OpponentController,
}

pub(crate) fn opponent_controller(binding: Binding<ObjectRef>) -> SourcePredicate {
    SourcePredicate {
        binding,
        condition: Expr::Compute(Box::new(Predicate::PlayerEquals {
            left: Expr::Literal(PlayerRef::ProposedDamageController {}),
            right: Expr::Literal(super::events::timing::opponent()),
        })),
    }
}

fn controller_recipients(recipients: Option<&Recipients<'_>>) -> bool {
    let Some(Recipients::Objects(objects)) = recipients else {
        return false;
    };
    if *objects
        == &(Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        })
    {
        return true;
    }
    let Selector::Zone { binding, .. } = objects else {
        return false;
    };
    **objects
        == Selector::Zone {
            players: PlayerSet {
                relation: PlayerRelation::SelfPlayer,
                relative_to: PlayerRef::Controller {},
                cardinality: Cardinality {
                    minimum: Nat(0),
                    maximum: None,
                },
                text_quantifier: TextQuantifier::Implicit,
            },
            zone: ZoneKind::Field,
            view: SelectionView::Current {},
            binding: *binding,
            predicate: Box::new(Expr::Compute(Box::new(Predicate::And {
                predicates: vec![Expr::Compute(Box::new(Predicate::KindEquals {
                    left: Expr::Live(Box::new(KindRead::Kind {
                        object: ObjectView::Current {
                            object: Box::new(ObjectOperand::Bound { binding: *binding }),
                        },
                        view: CharacteristicView::Effective,
                    })),
                    right: Expr::Literal(CardKind::Follower),
                }))],
            }))),
        }
}

pub(crate) struct Prevention<'effect> {
    pub source: &'effect AbilitySource,
    pub recipients: Option<Recipients<'effect>>,
    pub damage_source: Option<DamageSource<'effect>>,
    pub kind: DamageKind,
    pub condition: &'effect Expr<Bool>,
    pub rewrite: &'effect EventRewrite,
    pub duration: &'effect Duration,
    pub usage: Option<&'effect UseLimit>,
}

fn recipient_filter(predicate: &RecipientPredicate) -> Option<Recipients<'_>> {
    let Expr::Compute(condition) = &predicate.condition else {
        return None;
    };
    if let Predicate::RecipientIsFollower {
        recipient: DamageRecipient::Bound { binding },
        objects,
    } = condition.as_ref()
        && *binding == predicate.binding
    {
        return Some(Recipients::Objects(objects));
    }
    if let Predicate::RecipientIsLeader {
        recipient: DamageRecipient::Bound { binding },
        players,
    } = condition.as_ref()
        && *binding == predicate.binding
    {
        return Some(Recipients::Leaders(players));
    }
    None
}

fn source_filter(predicate: &SourcePredicate) -> Option<DamageSource<'_>> {
    if *predicate == opponent_controller(predicate.binding) {
        return Some(DamageSource::OpponentController);
    }
    let Expr::Compute(condition) = &predicate.condition else {
        return None;
    };
    let Predicate::MemberOf {
        object: ObjectOperand::Bound { binding },
        set,
    } = condition.as_ref()
    else {
        return None;
    };
    (*binding == predicate.binding).then_some(DamageSource::Objects(set))
}

pub(crate) fn projection(effect: &Effect) -> Option<Prevention<'_>> {
    let Effect::Replacement {
        source,
        pattern:
            ProposedEventPattern::Damage {
                recipient,
                source: damage_source,
                kind,
            },
        condition,
        rewrite,
        scope: ReplacementScope::General,
        affected_player: PlayerRef::ProposedDamageRecipient {},
        once_per_event: OncePerEvent::Enforced,
        usage,
        prospective_chars: None,
        duration,
        ..
    } = effect
    else {
        return None;
    };
    if let Some(usage) = usage
        && (!matches!(
            usage,
            UseLimit {
                count: Nat(1),
                scope: UsageScope::SharedPlayer {
                    player: PlayerRef::Controller {},
                    effect: EffectReference::CurrentInstallation {}
                }
            }
        ) || *kind != DamageKind::Any
            || damage_source.is_some()
            || !matches!(
                rewrite,
                EventRewrite::PreventDamage { .. }
                    | EventRewrite::AddDamage {
                        amount: Expr::Literal(super::Int(i64::MIN..=-1))
                    }
            ))
    {
        return None;
    }
    let supported = matches!(
        rewrite,
        EventRewrite::PreventDamage {
            recipient: None,
            source: None,
            kind: DamageKind::Any
        } | EventRewrite::AddDamage {
            amount: Expr::Literal(_)
        }
    ) || matches!(rewrite, EventRewrite::CapDamage { threshold: Expr::Literal(threshold), replacement: Expr::Literal(replacement) } if threshold.0 > replacement.0 && threshold.0 <= i64::MAX.unsigned_abs());
    if !supported {
        return None;
    }
    if recipient.is_none() && damage_source.is_none() {
        return None;
    }
    let recipients = match recipient {
        Some(predicate) => Some(recipient_filter(predicate)?),
        None => None,
    };
    let damage_source = match damage_source {
        Some(predicate) => Some(source_filter(predicate)?),
        None => None,
    };
    if matches!(damage_source, Some(DamageSource::OpponentController))
        && (*kind != DamageKind::Ability
            || !controller_recipients(recipients.as_ref())
            || !matches!(
                rewrite,
                EventRewrite::PreventDamage { .. } | EventRewrite::AddDamage { .. }
            ))
    {
        return None;
    }
    Some(Prevention {
        source,
        recipients,
        damage_source,
        kind: *kind,
        condition,
        rewrite,
        duration,
        usage: usage.as_ref(),
    })
}

pub(crate) const fn own_leader() -> PlayerSet {
    PlayerSet {
        relation: PlayerRelation::SelfPlayer,
        relative_to: PlayerRef::Controller {},
        cardinality: Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        },
        text_quantifier: TextQuantifier::One,
    }
}
