//! Played subscriptions narrow the committed card through one closed subject plan.
use super::subject::{SubjectFields, SubjectFilter, SubjectInformation, played_view};
use crate::ir::{
    Ability, AbilityOrigin, Binding, Bool, CharacteristicView, Comparison, EventPattern, Expr, Int,
    IntOperation, NumberRead, ObjectRef, PlayEvent, PlaySubjectFields, PlayerId, Predicate,
    TraitId, TriggerCondition, ZoneKind,
};

#[derive(Clone, Debug)]
pub(crate) struct PlayedFilter {
    pub(crate) subject: SubjectFilter,
    pub(crate) card: Binding<ObjectRef>,
    pub(crate) original: Option<(Comparison, i64)>,
    pub(crate) traits_any: Vec<TraitId>,
}

fn cost(event: Binding<PlayEvent>, card: Binding<ObjectRef>) -> NumberRead {
    NumberRead::Cost {
        object: played_view(event, card),
        view: CharacteristicView::Original,
    }
}

impl PlayedFilter {
    pub(crate) fn valid(&self) -> bool {
        self.subject.original_cost.is_none()
            && self
                .subject
                .class
                .as_ref()
                .is_none_or(|class| !class.0.is_empty())
            && self.original.is_none_or(|(op, n)| {
                n >= 0
                    && matches!(
                        op,
                        Comparison::Equal | Comparison::LessEqual | Comparison::GreaterEqual
                    )
            })
            && (self.traits_any.is_empty()
                || (self.traits_any.len() >= 2
                    && self.traits_any.iter().enumerate().all(|(i, t)| {
                        !t.0.is_empty()
                            && !self.traits_any.iter().take(i).any(|previous| previous == t)
                    })
                    && self.subject.trait_id.is_none()
                    && self.subject.keyword.is_none()
                    && self.subject.itself.is_none()
                    && self.subject.name.is_none()
                    && self.subject.class.is_none()
                    && self.subject.token.is_none()))
    }

    fn branch(&self, event: Binding<PlayEvent>, trait_id: Option<&TraitId>) -> Expr<Bool> {
        let mut subject = self.subject.clone();
        if let Some(trait_id) = trait_id {
            subject.trait_id = Some(trait_id.clone());
        }
        let mut predicate = subject.predicate(SubjectFields::Played {
            event,
            card: self.card,
        });
        if let Some((comparison, n)) = self.original
            && let Expr::Compute(operation) = &mut predicate
            && let Predicate::And { predicates } = operation.as_mut()
        {
            predicates.push(Expr::Compute(Box::new(Predicate::HasInfo {
                information: cost(event, self.card),
            })));
            predicates.push(Expr::Compute(Box::new(Predicate::CompareInt {
                left: Expr::Compute(Box::new(IntOperation::InformationOrZero {
                    information: cost(event, self.card),
                })),
                comparison,
                right: Expr::Literal(Int(n)),
            })));
        }
        predicate
    }

    pub(crate) fn predicate(&self, event: Binding<PlayEvent>) -> Expr<Bool> {
        let predicate = if self.traits_any.is_empty() {
            self.branch(event, None)
        } else {
            Expr::Compute(Box::new(Predicate::Or {
                predicates: self
                    .traits_any
                    .iter()
                    .map(|t| self.branch(event, Some(t)))
                    .collect(),
            }))
        };
        Expr::Compute(Box::new(Predicate::PlayedCard {
            subject: Expr::Frozen(Box::new(PlaySubjectFields::PlaySubject { event })),
            card: self.card,
            predicate,
        }))
    }

    pub(crate) fn parse(event: Binding<PlayEvent>, expression: &Expr<Bool>) -> Option<Self> {
        let Expr::Compute(predicate) = expression else {
            return None;
        };
        let Predicate::PlayedCard {
            card, predicate, ..
        } = predicate.as_ref()
        else {
            return None;
        };
        let fields = SubjectFields::Played { event, card: *card };
        let (subject, original, traits_any) = if let Expr::Compute(operation) = predicate
            && let Predicate::Or { predicates } = operation.as_ref()
        {
            if predicates.len() < 2 {
                return None;
            }
            let mut branches = predicates
                .iter()
                .map(|p| parse_branch(event, *card, fields, p));
            let (mut subject, original) = branches.next()??;
            let mut traits = vec![subject.trait_id.take()?];
            for branch in branches {
                let (mut other, cost) = branch?;
                traits.push(other.trait_id.take()?);
                if other != subject || cost != original {
                    return None;
                }
            }
            (subject, original, traits)
        } else {
            let (subject, original) = parse_branch(event, *card, fields, predicate)?;
            (subject, original, vec![])
        };
        let plan = Self {
            subject,
            card: *card,
            original,
            traits_any,
        };
        (plan.valid() && *expression == plan.predicate(event)).then_some(plan)
    }

    pub(crate) fn matches_information(
        &self,
        information: &SubjectInformation<'_>,
        same: bool,
        observer: PlayerId,
        actor: PlayerId,
        active: PlayerId,
        token: bool,
    ) -> bool {
        self.subject
            .matches_information(information, same, observer, actor, active, token)
            && self.original.is_none_or(|(op, n)| {
                information.original_cost.is_some_and(|value| match op {
                    Comparison::Equal => value == n,
                    Comparison::LessEqual => value <= n,
                    Comparison::GreaterEqual => value >= n,
                    Comparison::NotEqual | Comparison::Less | Comparison::Greater => false,
                })
            })
            && (self.traits_any.is_empty()
                || self
                    .traits_any
                    .iter()
                    .any(|t| information.traits.contains(t)))
    }
}

fn parse_branch(
    event: Binding<PlayEvent>,
    card: Binding<ObjectRef>,
    fields: SubjectFields,
    predicate: &Expr<Bool>,
) -> Option<(SubjectFilter, Option<(Comparison, i64)>)> {
    let Expr::Compute(operation) = predicate else {
        return None;
    };
    let Predicate::And { predicates } = operation.as_ref() else {
        return None;
    };
    let mut common = predicates.clone();
    let original = if let Some(Expr::Compute(last)) = common.last()
        && let Predicate::CompareInt {
            left: Expr::Compute(number),
            comparison,
            right: Expr::Literal(Int(n)),
        } = last.as_ref()
        && matches!(number.as_ref(),IntOperation::InformationOrZero {information} if *information==cost(event,card))
    {
        let result = Some((*comparison, *n));
        common.pop();
        let guard = Expr::Compute(Box::new(Predicate::HasInfo {
            information: cost(event, card),
        }));
        if common.pop().as_ref() != Some(&guard) {
            return None;
        }
        result
    } else {
        None
    };
    let subject = SubjectFilter::parse(
        fields,
        &Expr::Compute(Box::new(Predicate::And { predicates: common })),
    )?;
    Some((subject, original))
}

pub(crate) fn filter(ability: &Ability) -> Option<PlayedFilter> {
    if !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        || !matches!(
            ability.active_zones.as_slice(),
            [ZoneKind::Field | ZoneKind::Cemetery]
        )
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
    (trigger.active_zones == ability.active_zones).then_some(())?;
    PlayedFilter::parse(*binding, predicate)
}
