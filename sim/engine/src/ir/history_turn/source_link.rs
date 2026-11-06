//! Departure subscriptions join a committed damage recipient by object generation.
use crate::ir::events::subject::{SubjectFields, SubjectFilter};
use crate::ir::{
    Binding, Bool, BoolRead, CardKind, DamageEvent, DamageRecipient, EventPattern, Expr,
    HistoryCutoff, HistoryQuery, HistoryRange, HistoryWindow, InclusionPolicy, LeaveFieldEvent,
    ObjectFrozen, ObjectOperand, Predicate, RecipientFields, Selector,
};

#[derive(Clone)]
pub(crate) struct SourceLink {
    pub(crate) subject: SubjectFilter,
    pub(crate) source: SubjectFilter,
    pub(crate) damage: Binding<DamageEvent>,
}
impl SourceLink {
    pub(crate) fn predicate(&self, event: Binding<LeaveFieldEvent>) -> Expr<Bool> {
        Expr::Compute(Box::new(Predicate::And {
            predicates: vec![
                self.subject.predicate(SubjectFields::Cemetery(event)),
                Expr::Live(Box::new(BoolRead::HistoryExists {
                    query: HistoryQuery {
                        pattern: EventPattern::Damage {
                            binding: self.damage,
                            predicate: Expr::Compute(Box::new(Predicate::And {
                                predicates: vec![
                                    self.source
                                        .predicate(SubjectFields::DamageSource(self.damage)),
                                    Expr::Compute(Box::new(Predicate::RecipientIsFollower {
                                        recipient: DamageRecipient::FromExpression {
                                            value: Box::new(Expr::Frozen(Box::new(
                                                RecipientFields::DamageRecipient {
                                                    event: self.damage,
                                                },
                                            ))),
                                        },
                                        objects: Selector::Objects {
                                            objects: vec![ObjectOperand::FromExpression {
                                                value: Box::new(Expr::Frozen(Box::new(
                                                    ObjectFrozen::LeaveFieldObjectBefore { event },
                                                ))),
                                            }],
                                        },
                                    })),
                                ],
                            })),
                        },
                        window: HistoryWindow {
                            range: HistoryRange::CurrentTurn {},
                            cutoff: HistoryCutoff::CurrentPoint {},
                            relative_player: None,
                        },
                        include_execution: InclusionPolicy::IncludeAll {},
                    },
                })),
            ],
        }))
    }

    pub(crate) fn parse(event: Binding<LeaveFieldEvent>, expression: &Expr<Bool>) -> Option<Self> {
        let Expr::Compute(root) = expression else {
            return None;
        };
        let Predicate::And { predicates } = root.as_ref() else {
            return None;
        };
        let [subject, Expr::Live(read)] = predicates.as_slice() else {
            return None;
        };
        let subject = SubjectFilter::parse(SubjectFields::Cemetery(event), subject)?;
        let BoolRead::HistoryExists { query } = read.as_ref() else {
            return None;
        };
        let EventPattern::Damage {
            binding: damage,
            predicate: Expr::Compute(inner),
        } = &query.pattern
        else {
            return None;
        };
        let Predicate::And {
            predicates: damage_predicates,
        } = inner.as_ref()
        else {
            return None;
        };
        let source = SubjectFilter::parse(
            SubjectFields::DamageSource(*damage),
            damage_predicates.first()?,
        )?;
        let only_holder = SubjectFilter {
            itself: Some(true),
            ..SubjectFilter::default()
        };
        let own_trait = SubjectFilter {
            own: Some(true),
            trait_id: source.trait_id.clone(),
            ..SubjectFilter::default()
        };
        let plan = Self {
            subject,
            source,
            damage: *damage,
        };
        let expected_subject = SubjectFilter {
            own: Some(false),
            kind: Some(CardKind::Follower),
            own_turn: true,
            ..SubjectFilter::default()
        };
        (plan.subject == expected_subject
            && (plan.source == only_holder
                || (plan.source == own_trait
                    && plan
                        .source
                        .trait_id
                        .as_ref()
                        .is_some_and(|id| !id.0.is_empty())))
            && plan.predicate(event) == *expression)
            .then_some(plan)
    }
}
