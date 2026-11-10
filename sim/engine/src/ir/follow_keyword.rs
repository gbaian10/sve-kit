//! A current Ward query is confined to successful explicit movement members.
use super::{
    AbilitiesRead, AbilityPredicate, Binding, CardKind, CharacteristicView, Expr, FollowPermission,
    KeywordId, KindRead, MovementReceipt, ObjectOperand, ObjectRef, ObjectView, Predicate,
    Selector,
};

pub(crate) fn selector(receipt: MovementReceipt, binding: Binding<ObjectRef>) -> Selector {
    let object = ObjectView::Current {
        object: Box::new(ObjectOperand::Bound { binding }),
    };
    Selector::Filter {
        input: Box::new(Selector::MoveAfter {
            receipt,
            permission: FollowPermission::SameEffectExplicitReference {},
        }),
        binding,
        predicate: Box::new(Expr::Compute(Box::new(Predicate::And {
            predicates: vec![
                Expr::Compute(Box::new(Predicate::And {
                    predicates: vec![Expr::Compute(Box::new(Predicate::KindEquals {
                        left: Expr::Live(Box::new(KindRead::Kind {
                            object: object.clone(),
                            view: CharacteristicView::Effective,
                        })),
                        right: Expr::Literal(CardKind::Follower),
                    }))],
                })),
                Expr::Compute(Box::new(Predicate::HasAbility {
                    abilities: Expr::Live(Box::new(AbilitiesRead::Abilities {
                        object,
                        view: CharacteristicView::Effective,
                    })),
                    predicate: Box::new(AbilityPredicate {
                        kind: None,
                        keyword: Some(KeywordId::Ward),
                        except: vec![],
                        holder: None,
                        origin: None,
                    }),
                })),
            ],
        }))),
    }
}

pub(crate) fn supported(value: &Selector) -> bool {
    let Selector::Filter { input, binding, .. } = value else {
        return false;
    };
    let Selector::MoveAfter {
        receipt: receipt @ (MovementReceipt::Move { .. } | MovementReceipt::Search { .. }),
        permission: FollowPermission::SameEffectExplicitReference {},
    } = input.as_ref()
    else {
        return false;
    };
    *value == selector(receipt.clone(), *binding)
}
