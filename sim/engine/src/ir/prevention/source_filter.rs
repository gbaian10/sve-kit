use crate::ir::events::timing::opponent;
use crate::ir::{
    Binding, Bool, CardKind, CharacteristicView, Expr, KindRead, LkiReason, ObjectOperand,
    ObjectRef, ObjectView, PlayerRef, Predicate, SourcePredicate, TraitId, TraitsRead,
};

#[derive(Clone, Copy)]
pub(crate) struct Filter<'filter> {
    pub(crate) own: bool,
    pub(crate) kind: CardKind,
    pub(crate) trait_id: &'filter TraitId,
}

fn view(binding: Binding<ObjectRef>) -> ObjectView {
    ObjectView::LastKnown {
        object: Box::new(ObjectOperand::Bound { binding }),
        reason: LkiReason::SourceDeparted,
    }
}

impl Filter<'_> {
    pub(crate) fn predicate(self, binding: Binding<ObjectRef>) -> SourcePredicate {
        SourcePredicate {
            binding,
            condition: Expr::Compute(Box::new(Predicate::And {
                predicates: vec![
                    Expr::Compute(Box::new(Predicate::PlayerEquals {
                        left: Expr::Literal(PlayerRef::ProposedDamageController {}),
                        right: Expr::Literal(if self.own {
                            PlayerRef::Controller {}
                        } else {
                            opponent()
                        }),
                    })),
                    Expr::Compute(Box::new(Predicate::KindEquals {
                        left: Expr::Live(Box::new(KindRead::Kind {
                            object: view(binding),
                            view: CharacteristicView::Effective,
                        })),
                        right: Expr::Literal(self.kind),
                    })),
                    Expr::Compute(Box::new(Predicate::HasTrait {
                        traits: Expr::Live(Box::new(TraitsRead::Traits {
                            object: view(binding),
                            view: CharacteristicView::Effective,
                        })),
                        trait_id: self.trait_id.clone(),
                    })),
                ],
            })),
        }
    }
}

fn predicate(expression: &Expr<Bool>) -> Option<&Predicate> {
    if let Expr::Compute(predicate) = expression {
        Some(predicate.as_ref())
    } else {
        None
    }
}

pub(crate) fn parse(source: &SourcePredicate) -> Option<Filter<'_>> {
    let Predicate::And { predicates } = predicate(&source.condition)? else {
        return None;
    };
    let [controller, kind, trait_id] = predicates.as_slice() else {
        return None;
    };
    let Predicate::PlayerEquals {
        left: Expr::Literal(PlayerRef::ProposedDamageController {}),
        right: Expr::Literal(player),
    } = predicate(controller)?
    else {
        return None;
    };
    let own = if *player == (PlayerRef::Controller {}) {
        true
    } else if *player == opponent() {
        false
    } else {
        return None;
    };
    let Predicate::KindEquals {
        left: Expr::Live(kind_read),
        right: Expr::Literal(kind),
    } = predicate(kind)?
    else {
        return None;
    };
    let KindRead::Kind {
        object: kind_object,
        view: CharacteristicView::Effective,
    } = kind_read.as_ref()
    else {
        return None;
    };
    if *kind_object != view(source.binding) {
        return None;
    }
    let Predicate::HasTrait {
        traits: Expr::Live(traits_read),
        trait_id,
    } = predicate(trait_id)?
    else {
        return None;
    };
    let TraitsRead::Traits {
        object: traits_object,
        view: CharacteristicView::Effective,
    } = traits_read.as_ref()
    else {
        return None;
    };
    if *traits_object != view(source.binding) || trait_id.0.is_empty() {
        return None;
    }
    Some(Filter {
        own,
        kind: *kind,
        trait_id,
    })
}
