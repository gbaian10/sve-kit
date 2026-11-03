//! Closed printed evolution destinations share the existing face-selection predicate.
use super::{
    Binding, Bool, CardKind, CardName, CharacteristicView, Expr, KindRead, NamesRead,
    ObjectOperand, ObjectRef, ObjectView, Predicate,
};

pub(crate) enum Destination {
    SameName,
    Names(Vec<CardName>),
    Contains(CardName),
}

fn view(object: ObjectOperand) -> ObjectView {
    ObjectView::Current {
        object: Box::new(object),
    }
}

fn names(object: ObjectOperand) -> Expr<Vec<CardName>> {
    Expr::Live(Box::new(NamesRead::Names {
        object: view(object),
    }))
}

impl Destination {
    pub(crate) fn matches(&self, source: &str, form: &str, is_evolve_follower: bool) -> bool {
        match self {
            Self::SameName => source == form,
            Self::Names(names) => names.iter().any(|name| name.0 == form),
            Self::Contains(name) => is_evolve_follower && form.contains(&name.0),
        }
    }

    pub(crate) fn predicate(&self, candidate: Binding<ObjectRef>) -> Expr<Bool> {
        let object = ObjectOperand::Bound { binding: candidate };
        Expr::Compute(Box::new(match self {
            Self::SameName => Predicate::NamesIntersect {
                left: names(object),
                right: names(ObjectOperand::SourceHolder {}),
            },
            Self::Names(values) => Predicate::NamesIntersect {
                left: names(object),
                right: Expr::Literal(values.clone()),
            },
            Self::Contains(name) => Predicate::And {
                predicates: vec![
                    Expr::Compute(Box::new(Predicate::KindEquals {
                        left: Expr::Live(Box::new(KindRead::Kind {
                            object: view(object.clone()),
                            view: CharacteristicView::Effective,
                        })),
                        right: Expr::Literal(CardKind::EvolveFollower),
                    })),
                    Expr::Compute(Box::new(Predicate::NameContains {
                        object: view(object),
                        name: Expr::Literal(name.clone()),
                    })),
                ],
            },
        }))
    }

    pub(crate) fn from_predicate(
        predicate: &Expr<Bool>,
        candidate: Binding<ObjectRef>,
    ) -> Option<Self> {
        let Expr::Compute(operation) = predicate else {
            return None;
        };
        let destination = if let Predicate::NamesIntersect {
            right: Expr::Literal(values),
            ..
        } = operation.as_ref()
        {
            if values.is_empty() || values.iter().any(|name| name.0.is_empty()) {
                return None;
            }
            Self::Names(values.clone())
        } else if let Predicate::NamesIntersect { .. } = operation.as_ref() {
            Self::SameName
        } else if let Predicate::And { predicates } = operation.as_ref() {
            let [_, Expr::Compute(name_predicate)] = predicates.as_slice() else {
                return None;
            };
            let Predicate::NameContains {
                name: Expr::Literal(name),
                ..
            } = name_predicate.as_ref()
            else {
                return None;
            };
            if name.0.is_empty() {
                return None;
            }
            Self::Contains(name.clone())
        } else {
            return None;
        };
        (destination.predicate(candidate) == *predicate).then_some(destination)
    }
}
