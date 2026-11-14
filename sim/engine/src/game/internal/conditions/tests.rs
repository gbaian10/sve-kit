use super::*;
use crate::ir::{ClassId, IntRead, PlayerRef, PlayerResource, ScopeId};
use core::cell::Cell;

struct Saved {
    names: Vec<String>,
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
            LabelsProperty::Class(CharacteristicView::Effective) => Ok(matches(&saved.class)),
            LabelsProperty::Names => Ok(saved.names.iter().any(|name| matches(name))),
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
        names: vec!["candidate".into()],
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
            names: vec!["candidate".into()],
            class: "Elf".into(),
            cost: Some(amount),
        };
        assert_eq!(condition.evaluate(&Frozen(Some(&saved))).unwrap(), expected);
    }
    let incomplete = Saved {
        names: vec!["candidate".into()],
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

struct Printed<'saved>(Frozen<'saved>);

impl Reader for Printed<'_> {
    fn boolean(&self, property: &BooleanProperty) -> Result<bool> {
        self.0.boolean(property)
    }

    fn number(&self, property: &NumberProperty) -> Result<Option<i64>> {
        match property {
            NumberProperty::Cost(CharacteristicView::Original | CharacteristicView::Effective) => {
                self.0
                    .number(&NumberProperty::Cost(CharacteristicView::Effective))
            }
            _ => Err(unsupported("printed view has no requested number")),
        }
    }

    fn matches_labels(
        &self,
        property: &LabelsProperty,
        matches: &dyn Fn(&str) -> bool,
    ) -> Result<bool> {
        match property {
            LabelsProperty::Class(CharacteristicView::Original | CharacteristicView::Effective) => {
                self.0.matches_labels(
                    &LabelsProperty::Class(CharacteristicView::Effective),
                    matches,
                )
            }
            _ => self.0.matches_labels(property, matches),
        }
    }
}

#[test]
fn the_same_condition_reads_public_private_printed_and_frozen_candidate_information() {
    use crate::catalog::Catalog;
    use crate::game::Game;
    use crate::ir::{CardProgram, WellFormedCardProgram};
    use alloc::sync::Arc;
    use serde_json::{Value, json};

    let snapshot = json!({"number":"unit","faces":[{
        "name":"unit","card_class":"Elf","card_type":"フォロワー","cost":"2",
        "power":"2","hp":"3","traits":[],"text":null,"sections":[]
    }]})
    .to_string();
    let registry = json!({"format":1_u8,"kind":"keyword_registry","keywords":{}}).to_string();
    let document = json!({"format":1_u8,"kind":"effect_set","cards":{
        "unit":{"status":"complete","abilities":[]}
    }})
    .to_string();
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("synthetic.yaml".into(), document)])
            .unwrap(),
    );
    let position = json!({"history":"explicit","turn":{"active":"P1","first_player":"P1","phase":"main","elapsed_turns":{"P1":3_u8,"P2":2_u8}},"players":{
        "P1":{"leader":{"class":"Elf","life":20_u8},"pp":{"current":2_u8,"max":2_u8},"ep":0_u8,"sep":0_u8,"construction":"class","zones":{"field":[{"id":"visible","card":"unit"}],"deck":[{"id":"private","card":"unit"}]}},
        "P2":{"leader":{"class":"Elf","life":20_u8},"pp":{"current":2_u8,"max":2_u8},"ep":0_u8,"sep":0_u8,"construction":"class","zones":{}}
    }});
    let mut game = Game::new(
        catalog,
        &position,
        &Value::Null,
        &Value::Null,
        "candidate-views",
    )
    .unwrap();
    game.load_internal_card(
        "unit",
        &WellFormedCardProgram::check(CardProgram {
            abilities: vec![],
            card_play_costs: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    let binding = Binding::new(1, ScopeId(1));
    let expression = cost(binding, Expr::Literal(Int(2)));
    for (policy, id) in [(Policy::Selection, "visible"), (Policy::Search, "private")] {
        let plan = ConditionPlan::compile(&expression, binding, policy)
            .unwrap()
            .prepare(None)
            .unwrap();
        assert!(candidate::matches(&plan, &game, id).unwrap());
    }
    let printed = Saved {
        names: vec!["candidate".into()],
        class: "Elf".into(),
        cost: Some(2),
    };
    let frozen = Saved {
        names: vec!["candidate".into()],
        class: "Elf".into(),
        cost: Some(5),
    };
    let plan = ConditionPlan::compile(&expression, binding, Policy::Selection)
        .unwrap()
        .prepare(None)
        .unwrap();
    assert!(plan.evaluate(&Printed(Frozen(Some(&printed)))).unwrap());
    assert!(
        !plan.evaluate(&Frozen(Some(&frozen))).unwrap(),
        "saved information cannot be replaced by the live cost"
    );
}

#[test]
fn a_snapshot_cannot_substitute_effective_information_for_missing_original_information() {
    let binding = Binding::new(1, ScopeId(1));
    let mut expression = class(binding, "Elf");
    if let Expr::Compute(operation) = &mut expression
        && let Predicate::ClassEquals {
            left: Expr::Live(read),
            ..
        } = operation.as_mut()
    {
        let ClassRead::Class { view, .. } = read.as_mut();
        *view = CharacteristicView::Original;
    }
    let plan = ConditionPlan::compile(&expression, binding, Policy::Search)
        .unwrap()
        .prepare(None)
        .unwrap();
    let saved = Saved {
        names: vec!["candidate".into()],
        class: "Elf".into(),
        cost: Some(1),
    };
    assert!(plan.evaluate(&Printed(Frozen(Some(&saved)))).unwrap());
    assert!(matches!(
        plan.evaluate(&Frozen(Some(&saved))),
        Err(crate::EngineFailure::Unsupported(_))
    ));
}

#[test]
fn an_unprepared_public_filter_does_not_gain_the_holders_names() {
    let binding = Binding::new(1, ScopeId(1));
    let expression = Expr::Compute(Box::new(Predicate::NamesIntersect {
        left: Expr::Live(Box::new(NamesRead::Names {
            object: ObjectView::Current {
                object: Box::new(ObjectOperand::Bound { binding }),
            },
        })),
        right: Expr::Live(Box::new(NamesRead::Names {
            object: ObjectView::Current {
                object: Box::new(ObjectOperand::SourceHolder {}),
            },
        })),
    }));
    let plan = ConditionPlan::compile(&expression, binding, Policy::Selection)
        .unwrap()
        .prepare(None)
        .unwrap();
    let saved = Saved {
        names: vec!["candidate".into()],
        class: "Elf".into(),
        cost: Some(1),
    };
    assert!(matches!(
        plan.evaluate(&Frozen(Some(&saved))),
        Err(crate::EngineFailure::Unsupported(_))
    ));
    for policy in [Policy::Search, Policy::PrivateSelection] {
        assert!(matches!(
            ConditionPlan::compile(&expression, binding, policy),
            Err(crate::EngineFailure::Unsupported(_))
        ));
    }
}

fn holder_names(binding: Binding<ObjectRef>) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::NamesIntersect {
        left: Expr::Live(Box::new(NamesRead::Names {
            object: ObjectView::Current {
                object: Box::new(ObjectOperand::Bound { binding }),
            },
        })),
        right: Expr::Live(Box::new(NamesRead::Names {
            object: ObjectView::Current {
                object: Box::new(ObjectOperand::SourceHolder {}),
            },
        })),
    }))
}

fn holder_exclusion(binding: Binding<ObjectRef>) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::Not {
        predicate: holder_names(binding),
    }))
}

#[test]
fn public_materials_prepare_the_holder_exclusion_once_and_exclude_every_shared_name() {
    struct HolderNames(Cell<usize>);
    impl Resolver for HolderNames {
        fn number(&self, _expression: &Expr<Int>) -> Result<i64> {
            Err(unsupported("no number source"))
        }
        fn names(&self, source: SourceNames) -> Result<Vec<CardName>> {
            match source {
                SourceNames::Holder => {
                    self.0.set(self.0.get() + 1);
                    Ok(vec![CardName("holder".into()), CardName("alias".into())])
                }
                SourceNames::Information => Err(unsupported("no private source")),
            }
        }
    }
    let binding = Binding::new(1, ScopeId(1));
    for expression in [
        holder_exclusion(binding),
        Expr::Compute(Box::new(Predicate::And {
            predicates: vec![class(binding, "Elf"), holder_exclusion(binding)],
        })),
    ] {
        let source = HolderNames(Cell::new(0));
        let plan = ConditionPlan::compile_public_material(&expression, binding)
            .unwrap()
            .prepare(Some(&source))
            .unwrap();
        for (names, expected) in [
            (vec!["holder"], false),
            (vec!["other", "alias"], false),
            (vec!["different"], true),
        ] {
            let saved = Saved {
                names: names.into_iter().map(str::to_owned).collect(),
                class: "Elf".into(),
                cost: None,
            };
            assert_eq!(plan.evaluate(&Frozen(Some(&saved))).unwrap(), expected);
        }
        assert_eq!(source.0.get(), 1);
        for policy in [Policy::Search, Policy::PrivateSelection] {
            assert!(matches!(
                ConditionPlan::compile(&expression, binding, policy),
                Err(crate::EngineFailure::Unsupported(_))
            ));
        }
    }
}

#[test]
fn material_holder_authority_does_not_extend_to_other_positions_or_foreign_bindings() {
    let binding = Binding::new(1, ScopeId(1));
    let exclusion = holder_exclusion(binding);
    for expression in [
        holder_names(binding),
        holder_exclusion(Binding::new(2, ScopeId(1))),
        Expr::Compute(Box::new(Predicate::And {
            predicates: vec![exclusion.clone(), class(binding, "Elf")],
        })),
        Expr::Compute(Box::new(Predicate::Or {
            predicates: vec![class(binding, "Elf"), exclusion.clone()],
        })),
        Expr::Compute(Box::new(Predicate::And {
            predicates: vec![
                class(binding, "Elf"),
                Expr::Compute(Box::new(Predicate::And {
                    predicates: vec![class(binding, "Elf"), exclusion],
                })),
            ],
        })),
    ] {
        assert!(matches!(
            ConditionPlan::compile_public_material(&expression, binding),
            Err(crate::EngineFailure::Unsupported(_))
        ));
    }
}

struct EvolvedKind;

impl Reader for EvolvedKind {
    fn boolean(&self, _property: &BooleanProperty) -> Result<bool> {
        Err(unsupported("kind-only view"))
    }

    fn number(&self, _property: &NumberProperty) -> Result<Option<i64>> {
        Err(unsupported("kind-only view"))
    }

    fn matches_labels(
        &self,
        property: &LabelsProperty,
        matches: &dyn Fn(&str) -> bool,
    ) -> Result<bool> {
        if matches!(
            property,
            LabelsProperty::Kind(CharacteristicView::Effective)
        ) {
            Ok(matches("フォロワー・エボルヴ"))
        } else {
            Err(unsupported("kind-only view"))
        }
    }
}

#[test]
fn captured_kinds_remain_exact_while_candidate_kind_inclusion_stays_unchanged() {
    use crate::ir::events::subject::{SubjectFilter, SubjectInformation};
    use crate::ir::{PlayerId, TraitId};
    let observer = PlayerId { seat: 1 };
    let traits = [TraitId("Golem".into())];
    let mut information = SubjectInformation {
        original_cost: None,
        keywords: &[],
        kind: Some(CardKind::EvolveFollower),
        traits: &traits,
        names: &[],
        class: None,
    };
    let filter = SubjectFilter {
        kind: Some(CardKind::Follower),
        ..SubjectFilter::default()
    };
    assert!(
        EvolvedKind
            .matches_kind(CharacteristicView::Effective, CardKind::Follower)
            .unwrap()
    );
    assert!(
        !subject::matches(
            &filter,
            &information,
            false,
            observer,
            observer,
            observer,
            false
        )
        .unwrap()
    );
    let mut source = super::super::source::PublicCharacteristics {
        kind: information.kind,
        token: false,
        class: None,
        traits: traits.to_vec(),
        names: vec![],
    };
    assert!(!subject::source(&source, CardKind::Follower, &traits[0]).unwrap());
    assert!(subject::source(&source, CardKind::EvolveFollower, &traits[0]).unwrap());
    information.kind = Some(CardKind::Follower);
    assert!(
        subject::matches(
            &filter,
            &information,
            false,
            observer,
            observer,
            observer,
            false
        )
        .unwrap()
    );
    source.kind = None;
    assert!(!subject::source(&source, CardKind::Follower, &traits[0]).unwrap());
}

struct KindAndTraits {
    kind: &'static str,
    traits: &'static [&'static str],
}

impl Reader for KindAndTraits {
    fn boolean(&self, _property: &BooleanProperty) -> Result<bool> {
        Err(unsupported("kind and trait view"))
    }

    fn number(&self, _property: &NumberProperty) -> Result<Option<i64>> {
        Err(unsupported("kind and trait view"))
    }

    fn matches_labels(
        &self,
        property: &LabelsProperty,
        matches: &dyn Fn(&str) -> bool,
    ) -> Result<bool> {
        match property {
            LabelsProperty::Kind(CharacteristicView::Effective) => Ok(matches(self.kind)),
            LabelsProperty::Traits(CharacteristicView::Effective) => {
                Ok(self.traits.iter().any(|value| matches(value)))
            }
            _ => Err(unsupported("kind and trait view")),
        }
    }
}

/// KE-14: no printed Onmyoji amulet exists, so BP20-054's selector shape
/// (an Onmyoji follower, or any spell) is checked against a synthetic one.
#[test]
fn an_onmyoji_amulet_matches_neither_the_onmyoji_follower_nor_the_spell_branch() {
    use crate::ir::TraitId;
    let binding = Binding::new(1, ScopeId(1));
    let object = || ObjectView::Current {
        object: Box::new(ObjectOperand::Bound { binding }),
    };
    let kind = |expected: CardKind| {
        Expr::Compute(Box::new(Predicate::KindEquals {
            left: Expr::Live(Box::new(KindRead::Kind {
                object: object(),
                view: CharacteristicView::Effective,
            })),
            right: Expr::Literal(expected),
        }))
    };
    let expression = Expr::Compute(Box::new(Predicate::Or {
        predicates: vec![
            Expr::Compute(Box::new(Predicate::And {
                predicates: vec![
                    kind(CardKind::Follower),
                    Expr::Compute(Box::new(Predicate::HasTrait {
                        traits: Expr::Live(Box::new(TraitsRead::Traits {
                            object: object(),
                            view: CharacteristicView::Effective,
                        })),
                        trait_id: TraitId("陰陽師".into()),
                    })),
                ],
            })),
            kind(CardKind::Spell),
        ],
    }));
    let condition = ConditionPlan::compile(&expression, binding, Policy::Selection)
        .unwrap()
        .prepare(None)
        .unwrap();
    for (card, expected) in [
        (
            KindAndTraits {
                kind: "アミュレット",
                traits: &["陰陽師"],
            },
            false,
        ),
        (
            KindAndTraits {
                kind: "フォロワー",
                traits: &["陰陽師"],
            },
            true,
        ),
        (
            KindAndTraits {
                kind: "フォロワー",
                traits: &["兵士"],
            },
            false,
        ),
        (
            KindAndTraits {
                kind: "スペル",
                traits: &[],
            },
            true,
        ),
    ] {
        assert_eq!(
            condition.evaluate(&card).unwrap(),
            expected,
            "{}",
            card.kind
        );
    }
}
