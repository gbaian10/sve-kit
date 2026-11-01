//! Attack subscriptions reuse the committed subject and existing characteristic predicates.
use super::subject::{SubjectFields, SubjectFilter};
use crate::ir::{
    AbilitiesRead, Ability, AbilityPredicate, AttackEvent, Binding, Bool, CharacteristicView,
    Comparison, Expr, Int, IntOperation, KeywordId, NumberRead, Predicate, Stat,
};

#[derive(Clone, Default)]
pub(crate) struct AttackFilter {
    pub(crate) subject: SubjectFilter,
    pub(crate) keywords_any: Vec<KeywordId>,
    pub(crate) power_at_least: Option<Int>,
    pub(crate) own_turn: Option<bool>,
}

impl AttackFilter {
    fn keyword(event: Binding<AttackEvent>, keyword: KeywordId) -> Expr<Bool> {
        Expr::Compute(Box::new(Predicate::HasAbility {
            abilities: Expr::Live(Box::new(AbilitiesRead::Abilities {
                object: super::subject::attack_view(event),
                view: CharacteristicView::Effective,
            })),
            predicate: Box::new(AbilityPredicate {
                kind: None,
                keyword: Some(keyword),
                except: vec![],
                holder: None,
                origin: None,
            }),
        }))
    }
    fn power(event: Binding<AttackEvent>, minimum: Int) -> Expr<Bool> {
        Expr::Compute(Box::new(Predicate::CompareInt {
            left: Expr::Compute(Box::new(IntOperation::InformationOrZero {
                information: NumberRead::Stats {
                    object: super::subject::attack_view(event),
                    view: CharacteristicView::Effective,
                    property: Stat::Attack,
                },
            })),
            comparison: Comparison::GreaterEqual,
            right: Expr::Literal(minimum),
        }))
    }
    pub(crate) fn predicate(&self, event: Binding<AttackEvent>) -> Expr<Bool> {
        let fields = SubjectFields::Attack(event);
        let mut predicates = vec![self.subject.predicate(fields)];
        if !self.keywords_any.is_empty() {
            predicates.push(Expr::Compute(Box::new(Predicate::Or {
                predicates: self
                    .keywords_any
                    .iter()
                    .map(|keyword| Self::keyword(event, *keyword))
                    .collect(),
            })));
        }
        if let Some(minimum) = self.power_at_least {
            predicates.push(Self::power(event, minimum));
        }
        if let Some(own) = self.own_turn {
            predicates.push(super::timing::turn_predicate(own));
        }
        Expr::Compute(Box::new(Predicate::And { predicates }))
    }
    pub(crate) fn parse(event: Binding<AttackEvent>, expression: &Expr<Bool>) -> Option<Self> {
        let fields = SubjectFields::Attack(event);
        let Expr::Compute(root) = expression else {
            return None;
        };
        let Predicate::And { predicates } = root.as_ref() else {
            return None;
        };
        let mut plan = Self {
            subject: SubjectFilter::parse(fields, predicates.first()?)?,
            ..Self::default()
        };
        for part in predicates.iter().skip(1) {
            let Expr::Compute(predicate) = part else {
                return None;
            };
            if *part == super::timing::turn_predicate(true) {
                plan.own_turn = Some(true);
                continue;
            }
            if *part == super::timing::turn_predicate(false) {
                plan.own_turn = Some(false);
                continue;
            }
            if let Predicate::Or {
                predicates: alternatives,
            } = predicate.as_ref()
            {
                if alternatives.is_empty() {
                    return None;
                }
                for alternative in alternatives {
                    let Expr::Compute(inner) = alternative else {
                        return None;
                    };
                    let Predicate::HasAbility {
                        predicate: ability, ..
                    } = inner.as_ref()
                    else {
                        return None;
                    };
                    let keyword = ability.keyword?;
                    if !matches!(
                        keyword,
                        KeywordId::Ward
                            | KeywordId::Aura
                            | KeywordId::Storm
                            | KeywordId::Rush
                            | KeywordId::Bane
                            | KeywordId::Intimidate
                            | KeywordId::Assail
                            | KeywordId::SingleDrive
                            | KeywordId::TwinDrive
                    ) || *alternative != Self::keyword(event, keyword)
                    {
                        return None;
                    }
                    plan.keywords_any.push(keyword);
                }
            } else if let Predicate::CompareInt {
                right: Expr::Literal(minimum),
                ..
            } = predicate.as_ref()
            {
                if *part != Self::power(event, *minimum) {
                    return None;
                }
                plan.power_at_least = Some(*minimum);
            } else {
                return None;
            }
        }
        (plan.predicate(event) == *expression).then_some(plan)
    }
}

pub(crate) fn filter(ability: &Ability) -> Option<AttackFilter> {
    use crate::ir::{EventPattern, TriggerCondition};
    let TriggerCondition::Event {
        pattern: EventPattern::Attack { binding, predicate },
    } = &ability.trigger.as_ref()?.on
    else {
        return None;
    };
    AttackFilter::parse(*binding, predicate).or_else(|| {
        (*predicate == super::timing::attack_predicate(*binding)).then_some(AttackFilter {
            subject: SubjectFilter {
                itself: Some(true),
                ..SubjectFilter::default()
            },
            ..AttackFilter::default()
        })
    })
}
