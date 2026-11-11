use super::*;
use crate::ir::{ClassId, IntRead, PlayerRef, PlayerResource, ScopeId};

struct Saved {
    class: String,
    cost: Option<i64>,
}

// No live authority is available to fill holes in this saved view.
struct Frozen<'saved>(Option<&'saved Saved>);

impl Reader for Frozen<'_> {
    fn boolean(&self, _property: &BooleanProperty) -> Result<bool> {
        Err(unsupported("snapshot has no state or keyword information"))
    }

    fn number(&self, property: &NumberProperty) -> Result<Option<i64>> {
        let saved = self.0.ok_or_else(|| unsupported("missing snapshot"))?;
        match property {
            NumberProperty::Cost(CharacteristicView::Effective) => saved
                .cost
                .map(Some)
                .ok_or_else(|| unsupported("snapshot has no effective cost")),
            _ => Err(unsupported("snapshot has no requested number")),
        }
    }

    fn matches_labels(
        &self,
        property: &LabelsProperty,
        matches: &dyn Fn(&str) -> bool,
    ) -> Result<bool> {
        let saved = self.0.ok_or_else(|| unsupported("missing snapshot"))?;
        match property {
            LabelsProperty::Class => Ok(matches(&saved.class)),
            _ => Err(unsupported("snapshot has no requested labels")),
        }
    }
}

fn class(binding: Binding<ObjectRef>, expected: &str) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::ClassEquals {
        left: Expr::Live(Box::new(ClassRead::Class {
            object: ObjectView::Current {
                object: Box::new(ObjectOperand::Bound { binding }),
            },
            view: CharacteristicView::Effective,
        })),
        right: Expr::Literal(Info::Present(ClassId(expected.into()))),
    }))
}

fn cost(binding: Binding<ObjectRef>, right: Expr<Int>) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::CompareInt {
        left: Expr::Compute(Box::new(IntOperation::InformationOrZero {
            information: NumberRead::Cost {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::Bound { binding }),
                },
                view: CharacteristicView::Effective,
            },
        })),
        comparison: Comparison::LessEqual,
        right,
    }))
}

#[test]
fn validation_rejects_foreign_bindings_even_in_an_unselected_branch() {
    let binding = Binding::new(1, ScopeId(1));
    let expression = Expr::Compute(Box::new(Predicate::And {
        predicates: vec![
            class(binding, "Nightmare"),
            class(Binding::new(2, ScopeId(1)), "Elf"),
        ],
    }));
    for policy in [Policy::Selection, Policy::PrivateSelection, Policy::Search] {
        assert!(matches!(
            ConditionPlan::compile(&expression, binding, policy),
            Err(crate::EngineFailure::Unsupported(_))
        ));
    }
}

#[test]
fn literal_only_keeps_search_short_circuit_and_selection_eager_errors() {
    let binding = Binding::new(1, ScopeId(1));
    let unavailable = cost(
        binding,
        Expr::Live(Box::new(IntRead::PlayerResource {
            player: PlayerRef::Controller {},
            property: PlayerResource::PpMaximum,
        })),
    );
    let saved = Saved {
        class: "Elf".into(),
        cost: Some(1),
    };
    for (expression, expected) in [
        (
            Expr::Compute(Box::new(Predicate::And {
                predicates: vec![class(binding, "Nightmare"), unavailable.clone()],
            })),
            false,
        ),
        (
            Expr::Compute(Box::new(Predicate::Or {
                predicates: vec![class(binding, "Elf"), unavailable],
            })),
            true,
        ),
    ] {
        let search = ConditionPlan::compile(&expression, binding, Policy::Search)
            .unwrap()
            .prepare(None)
            .unwrap();
        assert_eq!(search.evaluate(&Frozen(Some(&saved))).unwrap(), expected);
        let selection = ConditionPlan::compile(&expression, binding, Policy::Selection)
            .unwrap()
            .prepare(None)
            .unwrap();
        assert!(matches!(
            selection.evaluate(&Frozen(Some(&saved))),
            Err(crate::EngineFailure::Unsupported(_))
        ));
    }
}

#[test]
fn a_frozen_reader_uses_saved_values_and_cannot_supply_missing_information() {
    let binding = Binding::new(1, ScopeId(1));
    let condition = ConditionPlan::compile(
        &cost(binding, Expr::Literal(Int(2))),
        binding,
        Policy::Selection,
    )
    .unwrap()
    .prepare(None)
    .unwrap();
    for (amount, expected) in [(1, true), (3, false)] {
        let saved = Saved {
            class: "Elf".into(),
            cost: Some(amount),
        };
        assert_eq!(condition.evaluate(&Frozen(Some(&saved))).unwrap(), expected);
    }
    let incomplete = Saved {
        class: "Elf".into(),
        cost: None,
    };
    for reader in [Frozen(None), Frozen(Some(&incomplete))] {
        assert!(matches!(
            condition.evaluate(&reader),
            Err(crate::EngineFailure::Unsupported(_))
        ));
    }
}
