use super::*;
use crate::rule_fixtures;
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::InputDomain;

fn program(action: Action, count: u64, zone: ZoneKind) -> WellFormedCardProgram {
    let mut ability = payment::paid_ability(vec![payment::act(), action], vec![draw(1, 0)]);
    let cardinality = Cardinality {
        minimum: Nat(count),
        maximum: Some(Nat(count)),
    };
    ability.node.play.as_mut().unwrap().parameters = vec![Decision {
        id: DecisionId(50),
        captures: vec![],
        actor: PlayerRef::Controller {},
        domain: ChoiceDomain::Objects {
            constraint: SelectionConstraint {
                groups: vec![SelectionGroup {
                    selector: Selector::Zone {
                        players: PlayerSet {
                            relation: PlayerRelation::SelfPlayer,
                            relative_to: PlayerRef::Controller {},
                            cardinality: Cardinality {
                                minimum: Nat(1),
                                maximum: Some(Nat(1)),
                            },
                            text_quantifier: TextQuantifier::Implicit,
                        },
                        zone,
                        view: SelectionView::Current {},
                        binding: Binding::new(52, ScopeId(2)),
                        predicate: Box::new(Expr::Literal(Bool(true))),
                    },
                    cardinality,
                }],
                cardinality,
                distinct_by: None,
                disjoint: Disjointness::Disjoint,
                aggregate_bounds: vec![],
            },
            binding: Binding::new(51, ScopeId(1)),
            target_roles: None,
        },
        kind: DecisionKind::PlayParameter {
            slot: PlaySlot::CostMaterials,
            selection_snapshot: SelectionSnapshot::AtDecision,
            play_scope: ExecutionRef::Current {},
            input: InputMode::Inline,
        },
        visibility: Visibility::Public {},
    }];
    card(vec![ability])
}

#[test]
fn material_payment_is_exact_and_later_failure_restores_prior_posture() {
    let action = Action::Posture {
        objects: Selector::Bound {
            reference: Binding::new(51, ScopeId(1)),
        },
        posture: Posture::Acted,
    };
    let mut game = position_with_card(1, &program(action.clone(), 1, ZoneKind::Field));
    let before = game.projection(View::Referee).unwrap();
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("materials");
    };
    assert!(matches!(question.domain, InputDomain::CostMaterials(_)));
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::CostMaterials(vec![0]),
    )
    .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    assert!(game.internal_decision(View::Referee).is_none());
    let mut game = position_with_card(1, &program(action, 2, ZoneKind::Field));
    let before = game.projection(View::Referee).unwrap();
    game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    assert!(game.internal_decision(View::Referee).is_none());
}

#[test]
fn material_costs_move_the_exact_selected_field_or_ex_object_to_its_owner() {
    for zone in [ZoneKind::Field, ZoneKind::Ex] {
        for destination in [OwnerZone::Cemetery, OwnerZone::Hand] {
            let action = Action::Move {
                objects: Selector::Bound {
                    reference: Binding::new(51, ScopeId(1)),
                },
                movement: MoveSpec {
                    to: MoveDestination::Owners { zone: destination },
                    position: None,
                    order: None,
                    initial_posture: None,
                    entry_effects: vec![],
                },
                reason: MoveReason::Put,
            };
            move_material(
                action,
                zone,
                if destination == OwnerZone::Hand {
                    "hand"
                } else {
                    "cemetery"
                },
            );
        }
        move_material(
            Action::Banish {
                objects: Selector::Bound {
                    reference: Binding::new(51, ScopeId(1)),
                },
            },
            zone,
            "banish",
        );
    }
}

#[expect(clippy::panic, reason = "This helper requires a material decision.")]
fn move_material(action: Action, zone: ZoneKind, destination: &str) {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    let source_zone = if zone == ZoneKind::Field {
        "field"
    } else {
        "ex"
    };
    if zone == ZoneKind::Field {
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"m","card":"unit-follower","owner":"P2"}));
    } else {
        initial["players"]["P1"]["zones"]["ex"] =
            json!([{"id":"m","card":"unit-follower","owner":"P2"}]);
    }
    let (snapshot, registry, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot.replace("スペル", "スペル・トークン");
    let rules = rule_fixtures::rules(
        &snapshot,
        json!([]),
        json!([rule_fixtures::resource(
            "lesson_item",
            "unit-spell",
            "printed",
            true
        )]),
    );
    let catalog = Arc::new(
        Catalog::from_documents_with_rules(
            &snapshot,
            &registry,
            &[("unit.yaml".into(), document)],
            &rules.to_string(),
            &EngineIdentityInput::LegacyJp,
        )
        .unwrap(),
    );
    let mut game = Game::new(catalog, &initial, &Value::Null, &Value::Null, "materials").unwrap();
    game.load_internal_card("unit-follower", &program(action, 1, zone))
        .unwrap();
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("expected material decision")
    };
    let handle = u64::from(source_zone == "field");
    let result = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![handle]),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P2"][destination],
        json!(["m"]),
        "selected materials move to the owner destination"
    );
    assert_eq!(
        view["objects"]["s"]["acted"], true,
        "source posture is also paid"
    );
    assert!(
        matches!(result.progress.receipts.get(&ReceiptId(101)), Some(InstructionOutcome::Payment { quantity: Quantity { amount: Int(1), unit: QuantityUnit::Cards }, subjects }) if subjects.len() == 1),
        "one material payment receipt"
    );
}
