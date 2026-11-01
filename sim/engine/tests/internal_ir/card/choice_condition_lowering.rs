use super::*;
use sve_engine::game::internal::{InputDomain, InstructionOutcome};

fn tree(cost: Value, body: Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let ability = &mut tree["cards"]["BP08-038"]["abilities"][0];
    ability.as_object_mut().unwrap().remove("fanfare");
    ability["activate"] = json!({});
    ability["activate"]["cost"] = cost;
    ability["activate"]["do"] = body;
    tree
}

fn payment(n: Value) -> Value {
    let mut cost = json!({"discard":{"sel":{"who":"self","zone":"hand"},"result":"paid_cards"}});
    cost["discard"]["n"] = n;
    cost
}

fn filtered(comparison: &str, amount: u64) -> Value {
    json!({"if":{"cond":{"cmp":{"left":{"count":{"in":{"objects_of":"paid_cards","view":"before"},"kind":"spell","class":"エルフ"}},"op":comparison,"right":amount}},"then":[{"life":{"who":"self","delta":1_u8}}]}})
}

#[test]
fn receipt_conditions_require_a_preceding_typed_discard_producer() {
    let body = json!([filtered("ge", 1)]);
    lowerer()
        .lower(&tree(payment(json!(1)), body.clone()))
        .unwrap();
    lowerer().lower(&tree(json!({"pp":1}), body)).unwrap_err();
    let mut before = tree(payment(json!(1)), json!([]));
    before["cards"]["BP08-038"]["abilities"][0]["activate"]["usable_if"] =
        filtered("ge", 1)["if"]["cond"].clone();
    lowerer().lower(&before).unwrap_err();
    let mut wrong_view = tree(payment(json!(1)), json!([filtered("ge", 1)]));
    wrong_view["cards"]["BP08-038"]["abilities"][0]["activate"]["do"][0]["if"]["cond"]["cmp"]["left"]
        ["count"]["in"]["view"] = json!("after");
    lowerer().lower(&wrong_view).unwrap_err();
}

#[test]
fn multicard_discard_existence_does_not_pretend_to_be_a_numeric_count() {
    for (operation, threshold) in [
        ("ge", 1_u64),
        ("gt", 0),
        ("ne", 0),
        ("lt", 1),
        ("le", 0),
        ("eq", 0),
    ] {
        lowerer()
            .lower(&tree(
                payment(json!(3)),
                json!([filtered(operation, threshold)]),
            ))
            .unwrap();
    }
    lowerer()
        .lower(&tree(payment(json!(3)), json!([filtered("ge", 2)])))
        .unwrap_err();
    lowerer()
        .lower(&tree(payment(json!(1)), json!([filtered("ge", 2)])))
        .unwrap();
}

#[test]
fn public_class_counts_reject_hidden_hand_and_deck_filters() {
    for zone in ["field", "cemetery", "ex", "hand", "deck"] {
        let body = json!([{"draw":{"who":"self","n":{"count":{"who":"self","zone":zone,"class":"ロイヤル","kind":"follower"}}}}]);
        assert_eq!(
            lowerer().lower(&tree(json!({"pp":1}), body)).is_ok(),
            matches!(zone, "field" | "cemetery" | "ex")
        );
    }
}

#[test]
fn turn_number_reads_the_chosen_players_started_turns() {
    let mut document = tree(
        json!({"pp":1}),
        json!([{"if":{
            "cond":{"cmp":{"left":{"turn_number":{"who":"self"}},"op":"ge","right":8}},
            "then":[{"life":{"who":"self","delta":2}}]
        }}]),
    );
    let card = document["cards"]["BP08-038"].take();
    document["cards"] = json!({"unit-follower":card});
    let programs = lowerer().lower(&document).unwrap();
    for (own, other, life) in [(7_u8, 20_u8, 10_u8), (8, 1, 12)] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":"unit-follower"}]);
        initial["turn"]["elapsed_turns"] = json!({"P1":own,"P2":other});
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "turn-condition",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &programs["unit-follower"])
            .unwrap();
        assert!(matches!(
            game.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Resolved(_)
        ));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            life
        );
    }
}

#[test]
fn count_of_draws_only_the_actual_number_of_paid_hand_cards() {
    let mut document = tree(
        json!({"discard":{"sel":{"who":"self","zone":"hand"},"upto":2,"result":"paid_cards"}}),
        json!([{"draw":{"who":"self","n":{"count_of":"paid_cards"}}}]),
    );
    let entry = document["cards"]["BP08-038"].take();
    document["cards"] = json!({"unit-follower":entry});
    let program = lowerer()
        .lower(&document)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for selected in [0_usize, 1, 2] {
        let mut initial = setup(
            1,
            json!([
                {"id":"draw-one","card":"unit-spell"},
                {"id":"draw-two","card":"unit-spell"},
                {"id":"draw-three","card":"unit-spell"}
            ]),
        );
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":"unit-follower"}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"paid-one","card":"unit-spell"}, {"id":"paid-two","card":"unit-spell"}
        ]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "actual-cost-count",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program).unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        let CardPlay::Awaiting(choice) = game
            .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("required hand cost")
        };
        let InputDomain::CostMaterials(domain) = &choice.domain else {
            panic!("hand cost materials")
        };
        let handles = domain
            .members
            .iter()
            .take(selected)
            .map(|member| member.handle)
            .collect();
        let progress = resolved(
            game.answer_internal_card(choice.actor, choice.token, Answer::CostMaterials(handles))
                .unwrap(),
        )
        .progress;
        assert!(progress.receipts.values().any(|outcome| matches!(outcome, InstructionOutcome::Cards(cards) if cards.actual == Nat(u64::try_from(selected).unwrap()))));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .len(),
            selected
        );
    }
}

#[test]
fn receipt_member_conditions_reject_otherwise_supported_but_unadmitted_predicates() {
    let mut document = tree(payment(json!(1)), json!([filtered("ge", 1)]));
    let entry = document["cards"]["BP08-038"].take();
    document["cards"] = json!({"unit-follower":entry});
    let program = lowerer()
        .lower(&document)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let initial = setup(1, json!([]));
    let mut allowed = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "receipt-member-positive",
    )
    .unwrap();
    allowed
        .load_internal_card("unit-follower", &program)
        .unwrap();
    let unsupported_member = Expr::Compute(Box::new(Predicate::CompareInt {
        left: Expr::Literal(Int(1)),
        comparison: Comparison::Equal,
        right: Expr::Literal(Int(1)),
    }));
    for expression in [
        unsupported_member.clone(),
        Expr::Compute(Box::new(Predicate::And {
            predicates: vec![Expr::Literal(Bool(false)), unsupported_member],
        })),
    ] {
        let mut raw = program.card().clone();
        let body = raw.abilities[0].node.body.as_mut().unwrap();
        let Node::Control {
            control: Control::If {
                condition: guard, ..
            },
        } = &mut body.steps[0].node
        else {
            panic!("conditional guard")
        };
        let Expr::Compute(operation) = guard else {
            panic!("receipt predicate")
        };
        let Predicate::ReceiptAny {
            predicate: member_predicate,
            ..
        } = operation.as_mut()
        else {
            panic!("receipt member predicate")
        };
        *member_predicate = expression;
        let checked = WellFormedCardProgram::check(raw).unwrap();
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "receipt-member-negative",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &checked)
            .unwrap_err();
    }
}

#[test]
fn optional_play_parameter_payment_cannot_export_discard_receipts() {
    let mut document = fanfare::expanded::tree().clone();
    document["cards"]["BP08-038"]["abilities"][0]["fanfare"] = json!({
        "cost":payment(json!(1)), "do":[filtered("ge", 1)]
    });
    let program = lowerer()
        .lower(&document)
        .unwrap()
        .remove("BP08-038")
        .unwrap();
    let mut optional = program.card().clone();
    let body = optional.abilities[0].node.body.as_mut().unwrap();
    let Node::Control {
        control: Control::Pay {
            optional: optionality,
            ..
        },
    } = &mut body.steps[0].node
    else {
        panic!("play-parameter payment")
    };
    *optionality = PaymentOptionality::Optional;
    let mut producer_only = optional.clone();
    producer_only.abilities[0]
        .node
        .body
        .as_mut()
        .unwrap()
        .steps
        .pop();
    WellFormedCardProgram::check(producer_only).unwrap();
    WellFormedCardProgram::check(optional).unwrap_err();
}

#[test]
fn other_public_class_count_excludes_holder_and_opposing_or_wrong_kind_cards() {
    let mut document = tree(
        json!({"pp":1}),
        json!([{"draw":{"who":"self","n":{"count":{"who":"self","zone":"field","kind":"follower","class":"ニュートラル","other":true}}}}]),
    );
    let entry = document["cards"]["BP08-038"].take();
    document["cards"] = json!({"unit-follower":entry});
    let program = lowerer()
        .lower(&document)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for count in [0_usize, 2] {
        let mut initial = setup(
            1,
            json!([
                {"id":"d1","card":"unit-spell"},{"id":"d2","card":"unit-spell"},{"id":"d3","card":"unit-spell"}
            ]),
        );
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        let mut field = vec![
            json!({"id":"source","card":"unit-follower"}),
            json!({"id":"wrong-kind","card":"unit-amulet"}),
        ];
        field.extend(
            (0..count).map(|index| json!({"id":format!("ally-{index}"),"card":"unit-follower"})),
        );
        initial["players"]["P1"]["zones"]["field"] = json!(field);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "other-class-count",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program).unwrap();
        for number in ["unit-spell", "unit-amulet"] {
            game.load_internal_card(number, &card(vec![])).unwrap();
        }
        resolved(
            game.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"]
                .as_array()
                .unwrap()
                .len(),
            count
        );
    }
}
