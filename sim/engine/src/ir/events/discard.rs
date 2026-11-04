//! Discard headers use the committed hand action and its original subject identity.
use super::timing::{opponent, turn_predicate};
use crate::ir::{
    Binding, Bool, CausePattern, CharacteristicView, DiscardEvent, EffectSource, EventContext,
    EventSnapshotPolicy, Expr, ObjectFrozen, ObjectOperand, ObjectRef, ObjectView, PlayerFrozen,
    PlayerId, PlayerRef, Predicate, SourcePredicate, TraitId, TraitsRead, TriggerSpec,
};

#[derive(Clone)]
pub(crate) struct DiscardSource {
    pub(crate) binding: Binding<ObjectRef>,
    pub(crate) trait_id: TraitId,
}

impl DiscardSource {
    fn predicate(&self, event: Binding<DiscardEvent>) -> Expr<Bool> {
        let context = EventContext::Discard { event };
        Expr::Compute(Box::new(Predicate::And {
            predicates: vec![
                Expr::Compute(Box::new(Predicate::PlayerEquals {
                    left: Expr::Frozen(Box::new(PlayerFrozen::HeaderActor { event: context })),
                    right: Expr::Literal(PlayerRef::Controller {}),
                })),
                Expr::Compute(Box::new(Predicate::EventCause {
                    event: context,
                    pattern: CausePattern::Effect {
                        source: EffectSource::Matching {
                            predicate: Box::new(SourcePredicate {
                                binding: self.binding,
                                condition: Expr::Compute(Box::new(Predicate::HasTrait {
                                    traits: Expr::Live(Box::new(TraitsRead::Traits {
                                        object: ObjectView::EventSnapshot {
                                            object: Box::new(ObjectOperand::Bound {
                                                binding: self.binding,
                                            }),
                                            event: context,
                                            policy: EventSnapshotPolicy::BeforeExit,
                                        },
                                        view: CharacteristicView::Effective,
                                    })),
                                    trait_id: self.trait_id.clone(),
                                })),
                            }),
                        },
                    },
                })),
            ],
        }))
    }

    fn parse(expression: &Expr<Bool>) -> Option<Self> {
        let Expr::Compute(outer) = expression else {
            return None;
        };
        let Predicate::And { predicates } = outer.as_ref() else {
            return None;
        };
        let [_, Expr::Compute(cause)] = predicates.as_slice() else {
            return None;
        };
        let Predicate::EventCause {
            pattern:
                CausePattern::Effect {
                    source: EffectSource::Matching { predicate },
                },
            ..
        } = cause.as_ref()
        else {
            return None;
        };
        let Expr::Compute(condition) = &predicate.condition else {
            return None;
        };
        let Predicate::HasTrait { trait_id, .. } = condition.as_ref() else {
            return None;
        };
        (!trait_id.0.trim().is_empty()).then(|| Self {
            binding: predicate.binding,
            trait_id: trait_id.clone(),
        })
    }
}

#[derive(Clone)]
pub(crate) struct DiscardFilter {
    pub(crate) own: Option<bool>,
    pub(crate) itself: bool,
    pub(crate) own_turn: bool,
    pub(crate) by: Option<DiscardSource>,
}
impl DiscardFilter {
    pub(crate) fn predicate(&self, event: Binding<DiscardEvent>) -> Expr<Bool> {
        let mut predicates = Vec::new();
        if let Some(own) = self.own {
            predicates.push(Expr::Compute(Box::new(Predicate::PlayerEquals {
                left: Expr::Frozen(Box::new(PlayerFrozen::DiscardPlayer { event })),
                right: Expr::Literal(if own {
                    PlayerRef::Controller {}
                } else {
                    opponent()
                }),
            })));
        }
        if self.itself {
            predicates.push(Expr::Compute(Box::new(Predicate::ObjectEquals {
                left: Expr::Frozen(Box::new(ObjectFrozen::DiscardCardBefore { event })),
                right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
            })));
        }
        if self.own_turn {
            predicates.push(turn_predicate(true));
        }
        if let Some(source) = &self.by {
            predicates.push(source.predicate(event));
        }
        Expr::Compute(Box::new(Predicate::And { predicates }))
    }
    pub(crate) fn parse(event: Binding<DiscardEvent>, expression: &Expr<Bool>) -> Option<Self> {
        let by = if let Expr::Compute(outer) = expression
            && let Predicate::And { predicates } = outer.as_ref()
        {
            predicates.iter().find_map(DiscardSource::parse)
        } else {
            None
        };
        for own in [None, Some(false), Some(true)] {
            for itself in [false, true] {
                for own_turn in [false, true] {
                    if by.is_some() && (own != Some(true) || !itself || own_turn) {
                        continue;
                    }
                    let plan = Self {
                        own,
                        itself,
                        own_turn,
                        by: by.clone(),
                    };
                    if plan.predicate(event) == *expression {
                        return Some(plan);
                    }
                }
            }
        }
        None
    }
    pub(crate) fn matches(
        &self,
        player: PlayerId,
        card: ObjectRef,
        source: ObjectRef,
        controller: PlayerId,
        active: PlayerId,
    ) -> bool {
        self.own.is_none_or(|own| own == (player == controller))
            && (!self.itself || card == source)
            && (!self.own_turn || active == controller)
    }
}

/// Explicit self movement follows one committed hand member, never a field observer's batch.
pub(crate) fn self_event(trigger: &TriggerSpec) -> Option<Binding<DiscardEvent>> {
    use crate::ir::{
        EventPattern, EventSnapshotPolicy, TriggerAggregation, TriggerCondition, ZoneKind,
    };
    let TriggerCondition::Event {
        pattern: EventPattern::Discard { binding, predicate },
    } = &trigger.on
    else {
        return None;
    };
    let plan = DiscardFilter::parse(*binding, predicate)?;
    (trigger.active_zones == [ZoneKind::Hand]
        && trigger.aggregation == (TriggerAggregation::PerFact {})
        && trigger.view == EventSnapshotPolicy::BeforeExit
        && plan.itself
        && plan.own == Some(true)
        && !plan.own_turn)
        .then_some(*binding)
}
