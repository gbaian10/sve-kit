//! Discard headers use the committed hand action and its original subject identity.
use super::timing::{opponent, turn_predicate};
use crate::ir::{
    Binding, Bool, CardKind, CausePattern, CharacteristicView, Characteristics,
    CharacteristicsFields, ClassFrozen, ClassId, DiscardEvent, EffectSource, EventContext,
    EventSnapshotPolicy, Expr, Info, KindFrozen, ObjectFrozen, ObjectOperand, ObjectRef,
    ObjectView, PlayerFrozen, PlayerId, PlayerRef, Predicate, SourcePredicate, TraitId, TraitsRead,
    TriggerSpec,
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
        // DiscardFilter::parse checks the extracted candidate against the complete canonical predicate.
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

/// The committed hand characteristics of the discarded card, read before it leaves.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct DiscardCard {
    pub(crate) kind: Option<CardKind>,
    pub(crate) class: Option<ClassId>,
}

impl DiscardCard {
    fn characters(event: Binding<DiscardEvent>) -> Expr<Characteristics> {
        Expr::Frozen(Box::new(CharacteristicsFields::DiscardCharacters { event }))
    }

    fn predicates(&self, event: Binding<DiscardEvent>) -> Vec<Expr<Bool>> {
        let mut predicates = Vec::new();
        if let Some(kind) = self.kind {
            predicates.push(Expr::Compute(Box::new(Predicate::KindEquals {
                left: Expr::Frozen(Box::new(KindFrozen::Characters {
                    characters: Self::characters(event),
                })),
                right: Expr::Literal(kind),
            })));
        }
        if let Some(class) = &self.class {
            predicates.push(Expr::Compute(Box::new(Predicate::ClassEquals {
                left: Expr::Frozen(Box::new(ClassFrozen::Characters {
                    characters: Self::characters(event),
                })),
                right: Expr::Literal(Info::Present(class.clone())),
            })));
        }
        predicates
    }

    fn parse(predicates: &[Expr<Bool>]) -> Option<Self> {
        // DiscardFilter::parse checks the extracted candidate against the complete canonical predicate.
        let mut card = Self {
            kind: None,
            class: None,
        };
        for predicate in predicates {
            let Expr::Compute(operation) = predicate else {
                continue;
            };
            if let Predicate::KindEquals {
                right: Expr::Literal(kind),
                ..
            } = operation.as_ref()
            {
                card.kind = Some(*kind);
            }
            if let Predicate::ClassEquals {
                right: Expr::Literal(Info::Present(class)),
                ..
            } = operation.as_ref()
            {
                card.class = Some(class.clone());
            }
        }
        (card.kind.is_some() || card.class.as_ref().is_some_and(|class| !class.0.is_empty()))
            .then_some(card)
    }

    fn matches(&self, characters: &Characteristics) -> bool {
        self.kind.is_none_or(|kind| kind == characters.kind)
            && self
                .class
                .as_ref()
                .is_none_or(|class| characters.class.as_ref() == Some(class))
    }
}

#[derive(Clone)]
pub(crate) struct DiscardFilter {
    pub(crate) own: Option<bool>,
    pub(crate) itself: bool,
    pub(crate) own_turn: bool,
    pub(crate) by: Option<DiscardSource>,
    pub(crate) card: Option<DiscardCard>,
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
        if let Some(card) = &self.card {
            predicates.extend(card.predicates(event));
        }
        Expr::Compute(Box::new(Predicate::And { predicates }))
    }
    pub(crate) fn parse(event: Binding<DiscardEvent>, expression: &Expr<Bool>) -> Option<Self> {
        let (by, card) = if let Expr::Compute(outer) = expression
            && let Predicate::And { predicates } = outer.as_ref()
        {
            (
                predicates.iter().find_map(DiscardSource::parse),
                DiscardCard::parse(predicates),
            )
        } else {
            (None, None)
        };
        for own in [None, Some(false), Some(true)] {
            for itself in [false, true] {
                for own_turn in [false, true] {
                    if (by.is_some() && (own != Some(true) || !itself || own_turn))
                        || (card.is_some() && (itself || by.is_some()))
                    {
                        continue;
                    }
                    let plan = Self {
                        own,
                        itself,
                        own_turn,
                        by: by.clone(),
                        card: card.clone(),
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

    /// Characteristic filters read the card's hand characteristics before it leaves.
    pub(crate) fn matches_card(&self, characters: &Characteristics) -> bool {
        self.card
            .as_ref()
            .is_none_or(|card| card.matches(characters))
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
