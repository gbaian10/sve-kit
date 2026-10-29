//! Cemetery observers match the committed card before its body changes any zone.
use crate::ir::{
    Ability, AbilityOrigin, Binding, Bool, CardKind, CharacteristicView, ClassId, ClassRead,
    EventContext, EventPattern, EventSnapshotPolicy, Expr, Info, KindRead, ObjectOperand,
    ObjectRef, ObjectView, PlayEvent, PlaySubjectFields, PlayerFrozen, PlayerRef, Predicate,
    TriggerCondition, ZoneKind,
};

#[derive(Clone, Debug)]
pub(crate) struct PlayedFilter {
    pub(crate) class: ClassId,
    pub(crate) card: Binding<ObjectRef>,
}

impl PlayedFilter {
    pub(crate) fn predicate(&self, event: Binding<PlayEvent>) -> Expr<Bool> {
        let card = self.card;
        let object = ObjectView::EventSnapshot {
            object: Box::new(ObjectOperand::FromExpression {
                value: Box::new(Expr::Bound(card)),
            }),
            event: EventContext::Play { event },
            policy: EventSnapshotPolicy::AtReveal,
        };
        Expr::Compute(Box::new(Predicate::And {
            predicates: vec![
                Expr::Compute(Box::new(Predicate::PlayerEquals {
                    left: Expr::Frozen(Box::new(PlayerFrozen::PlayController { event })),
                    right: Expr::Literal(PlayerRef::Controller {}),
                })),
                Expr::Compute(Box::new(Predicate::PlayerEquals {
                    left: Expr::Literal(PlayerRef::Active {}),
                    right: Expr::Literal(PlayerRef::Controller {}),
                })),
                Expr::Compute(Box::new(Predicate::PlayedCard {
                    subject: Expr::Frozen(Box::new(PlaySubjectFields::PlaySubject { event })),
                    card,
                    predicate: Expr::Compute(Box::new(Predicate::And {
                        predicates: vec![
                            Expr::Compute(Box::new(Predicate::KindEquals {
                                left: Expr::Live(Box::new(KindRead::Kind {
                                    object: object.clone(),
                                    view: CharacteristicView::Effective,
                                })),
                                right: Expr::Literal(CardKind::Spell),
                            })),
                            Expr::Compute(Box::new(Predicate::ClassEquals {
                                left: Expr::Live(Box::new(ClassRead::Class {
                                    object,
                                    view: CharacteristicView::Effective,
                                })),
                                right: Expr::Literal(Info::Present(self.class.clone())),
                            })),
                        ],
                    })),
                })),
            ],
        }))
    }

    pub(crate) fn parse(event: Binding<PlayEvent>, expression: &Expr<Bool>) -> Option<Self> {
        let Expr::Compute(outer) = expression else {
            return None;
        };
        let Predicate::And { predicates } = outer.as_ref() else {
            return None;
        };
        let [_, _, Expr::Compute(subject)] = predicates.as_slice() else {
            return None;
        };
        let Predicate::PlayedCard {
            card,
            predicate: Expr::Compute(filter),
            ..
        } = subject.as_ref()
        else {
            return None;
        };
        let Predicate::And {
            predicates: card_predicates,
        } = filter.as_ref()
        else {
            return None;
        };
        let [_, Expr::Compute(class)] = card_predicates.as_slice() else {
            return None;
        };
        let Predicate::ClassEquals {
            right: Expr::Literal(Info::Present(class)),
            ..
        } = class.as_ref()
        else {
            return None;
        };
        let plan = Self {
            class: class.clone(),
            card: *card,
        };
        (!class.0.is_empty() && *expression == plan.predicate(event)).then_some(plan)
    }
}

pub(crate) fn filter(ability: &Ability) -> Option<PlayedFilter> {
    if !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        || ability.active_zones != [ZoneKind::Cemetery]
    {
        return None;
    }
    let trigger = ability.trigger.as_ref()?;
    let TriggerCondition::Event {
        pattern: EventPattern::Play { binding, predicate },
    } = &trigger.on
    else {
        return None;
    };
    (trigger.active_zones == [ZoneKind::Cemetery]).then_some(())?;
    PlayedFilter::parse(*binding, predicate)
}
