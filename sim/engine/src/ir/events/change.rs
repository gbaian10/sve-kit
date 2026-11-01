//! Committed changes use the same typed subject and per-fact subscription plans.
use core::iter::once;

use super::subject::{SubjectFields, SubjectFilter};
use crate::ir::{
    Binding, Bool, Comparison, Expr, FacetFields, HolderFields, Int, IntFrozen, ObjectFrozen,
    ObjectRef, Posture, PostureEvent, PostureFields, Predicate, RuntimeHolder, Stat,
    StatChangeEvent, StatDelta,
};

pub(crate) struct PostureFilter {
    pub(crate) subject: SubjectFilter,
    pub(crate) after: Posture,
}
impl PostureFilter {
    pub(crate) fn predicate(&self, event: Binding<PostureEvent>) -> Expr<Bool> {
        Expr::Compute(Box::new(Predicate::And {
            predicates: vec![
                self.subject.predicate(SubjectFields::Posture(event)),
                Expr::Compute(Box::new(Predicate::PostureEquals {
                    left: Expr::Frozen(Box::new(PostureFields::PostureAfter { event })),
                    right: Expr::Literal(self.after),
                })),
            ],
        }))
    }
    pub(crate) fn parse(event: Binding<PostureEvent>, expression: &Expr<Bool>) -> Option<Self> {
        let Expr::Compute(root) = expression else {
            return None;
        };
        let Predicate::And { predicates } = root.as_ref() else {
            return None;
        };
        let [subject, Expr::Compute(posture)] = predicates.as_slice() else {
            return None;
        };
        let Predicate::PostureEquals {
            right: Expr::Literal(after),
            ..
        } = posture.as_ref()
        else {
            return None;
        };
        let plan = Self {
            subject: SubjectFilter::parse(SubjectFields::Posture(event), subject)?,
            after: *after,
        };
        (plan.predicate(event) == *expression).then_some(plan)
    }
}

pub(crate) struct StatFilter {
    pub(crate) itself: bool,
    pub(crate) properties: Vec<Stat>,
    pub(crate) up: Option<bool>,
    pub(crate) subject: Binding<ObjectRef>,
    pub(crate) facet: Binding<StatDelta>,
}
impl StatFilter {
    pub(crate) fn predicate(&self, event: Binding<StatChangeEvent>) -> Expr<Bool> {
        let mut predicates = Vec::new();
        if self.itself {
            predicates.push(Expr::Compute(Box::new(Predicate::ObjectEquals {
                left: Expr::Bound(self.subject),
                right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
            })));
        }
        predicates.push(Expr::Compute(Box::new(Predicate::Or {
            predicates: self
                .properties
                .iter()
                .map(|property| {
                    Expr::Compute(Box::new(Predicate::AnyStatFacet {
                        facets: Expr::Frozen(Box::new(FacetFields::StatChangeFacets { event })),
                        facet: self.facet,
                        property: *property,
                        predicate: self.up.map_or(Expr::Literal(Bool(true)), |up| {
                            Expr::Compute(Box::new(Predicate::CompareInt {
                                left: Expr::Frozen(Box::new(IntFrozen::StatDelta {
                                    event: self.facet,
                                })),
                                comparison: if up {
                                    Comparison::Greater
                                } else {
                                    Comparison::Less
                                },
                                right: Expr::Literal(Int(0)),
                            }))
                        }),
                    }))
                })
                .collect(),
        })));
        Expr::Compute(Box::new(Predicate::HolderIsObject {
            holder: Expr::Frozen(Box::new(HolderFields::StatChangeSubject { event })),
            object: self.subject,
            predicate: Expr::Compute(Box::new(Predicate::And { predicates })),
        }))
    }
    pub(crate) fn parse(event: Binding<StatChangeEvent>, expression: &Expr<Bool>) -> Option<Self> {
        let Expr::Compute(root) = expression else {
            return None;
        };
        let Predicate::HolderIsObject {
            object,
            predicate: Expr::Compute(inner),
            ..
        } = root.as_ref()
        else {
            return None;
        };
        let Predicate::And { predicates } = inner.as_ref() else {
            return None;
        };
        let Expr::Compute(last) = predicates.last()? else {
            return None;
        };
        let Predicate::Or { predicates: facets } = last.as_ref() else {
            return None;
        };
        let Expr::Compute(first) = facets.first()? else {
            return None;
        };
        let Predicate::AnyStatFacet { facet, .. } = first.as_ref() else {
            return None;
        };
        let properties = facets
            .iter()
            .map(|part| {
                let Expr::Compute(part) = part else {
                    return None;
                };
                let Predicate::AnyStatFacet { property, .. } = part.as_ref() else {
                    return None;
                };
                Some(*property)
            })
            .collect::<Option<Vec<_>>>()?;
        [None, Some(false), Some(true)]
            .into_iter()
            .flat_map(|up| {
                let properties = properties.clone();
                [false, true].into_iter().map(move |itself| Self {
                    itself,
                    properties: properties.clone(),
                    up,
                    subject: *object,
                    facet: *facet,
                })
            })
            .find(|plan| plan.predicate(event) == *expression)
    }
    pub(crate) fn matches(&self, event: &StatChangeEvent, source: ObjectRef) -> bool {
        let RuntimeHolder::Object { object } = event.subject else {
            return false;
        };
        (!self.itself || source == object)
            && once(&event.facets.first)
                .chain(&event.facets.rest)
                .any(|facet| {
                    facet.delta.0 != 0
                        && self.properties.contains(&facet.property)
                        && self.up.is_none_or(|up| up == (facet.delta.0 > 0))
                })
    }
}
