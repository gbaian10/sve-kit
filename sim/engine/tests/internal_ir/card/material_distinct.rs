//! Original-cost distinctness remains a constraint on the ordinary material route.
#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate these material scenarios."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, InputDomain};
use sve_engine::game::player::{
    Command, DecisionShape, Input, QuestionShape, SubmissionKind, Viewer,
};

fn tree(count: u64, body: &Value, quick: bool) -> Value {
    let mut activation = json!({
        "cost":{"steps":[{"act_self":true},{"banish":{
            "sel":{"who":"self","zone":"ex"},"n":count,"distinct_by":"orig_cost"
        }}]},"do":body
    });
    if quick {
        activation["quick"] = json!(true);
    }
    equipment_played::tree(&json!({"activate":activation}), "unit-follower")
}

fn program(count: u64) -> WellFormedCardProgram {
    lowerer()
        .lower(&tree(
            count,
            &json!([
                {"counter":{"on":"this","name":"completed","delta":1}},
                {"life":{"who":"self","delta":1}}
            ]),
            false,
        ))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn catalogue() -> Arc<Catalog> {
    let (snapshot, registry, document) = catalog_documents(&json!("1"));
    let mut rows = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let template = rows
        .iter()
        .find(|row| row["number"] == "unit-follower")
        .unwrap()
        .clone();
    let mut document: Value = serde_json::from_str(&document).unwrap();
    for (number, cost, kind) in [
        ("m0", "0", "フォロワー"),
        ("m1", "1", "フォロワー"),
        ("m2a", "2", "フォロワー"),
        ("m2b", "2", "フォロワー"),
        ("m3", "3", "フォロワー"),
        ("m4", "4", "フォロワー"),
        ("missing", "-", "フォロワー"),
        ("missing-token", "-", "フォロワー・トークン"),
    ] {
        let mut row = template.clone();
        row["number"] = json!(number);
        row["faces"][0]["name"] = json!(number);
        row["faces"][0]["cost"] = json!(cost);
        row["faces"][0]["card_type"] = json!(kind);
        rows.push(row);
        document["cards"][number] = document["cards"]["unit-follower"].clone();
    }
    Arc::new(
        Catalog::from_documents(
            &rows
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
            &registry,
            &[("synthetic.yaml".into(), document.to_string())],
        )
        .unwrap(),
    )
}

fn initial(seat: u32, materials: &[&str]) -> Value {
    let actor = if seat == 1 { "P1" } else { "P2" };
    let mut initial = setup(seat, json!([{"id":"drawn","card":"unit-spell"}]));
    initial["turn"]["active"] = json!(actor);
    initial["players"][actor]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    initial["players"][actor]["zones"]["hand"] = json!([]);
    initial["players"][actor]["zones"]["ex"] = json!(
        materials
            .iter()
            .enumerate()
            .map(|(i, number)| json!({"id":format!("e{i}"),"card":number,"state":{"cost":9}}))
            .collect::<Vec<_>>()
    );
    initial
}

fn board(program: &WellFormedCardProgram, initial: &Value) -> Game {
    let mut game = Game::new(
        catalogue(),
        initial,
        &Value::Null,
        &Value::Null,
        "material-distinct",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    for number in [
        "unit-spell",
        "unit-amulet",
        "m0",
        "m1",
        "m2a",
        "m2b",
        "m3",
        "m4",
    ] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("ordinary cost materials")
    };
    question
}

fn start(game: &mut Game, program: &WellFormedCardProgram, seat: u32) -> DecisionView {
    awaiting(
        game.activate_internal_card(
            "source",
            program.card().abilities[0].node.id,
            PlayerId { seat },
        )
        .unwrap(),
    )
}

#[test]
fn bp10_003_fragment_banishes_five_distinct_original_costs_from_ex_both_seats() {
    let program = program(5);
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        let mut initial = initial(seat, &["m0", "m1", "m2a", "m3", "m4"]);
        initial["players"][actor]["zones"]["hand"] = json!([{"id":"hand-decoy","card":"m4"}]);
        initial["players"][opponent]["zones"]["ex"] = json!([{"id":"foreign","card":"m4"}]);
        let mut game = board(&program, &initial);
        let question = start(&mut game, &program, seat);
        let InputDomain::CostMaterials(domain) = &question.domain else {
            panic!("ordinary materials")
        };
        assert_eq!(domain.members.len(), 5);
        assert_eq!(
            domain
                .original_costs
                .as_ref()
                .unwrap()
                .values()
                .copied()
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4]
        );
        let result = resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(domain.members.iter().map(|m| m.handle).collect()),
            )
            .unwrap(),
        );
        assert_eq!(result.progress.facts.iter().filter(|fact|matches!(fact,Fact::Movement(m) if m.method==MoveMethod::Banish && m.from.kind==ZoneKind::Ex)).count(),5);
        assert!(result.progress.receipts.values().any(|receipt|matches!(receipt,InstructionOutcome::Payment{quantity,subjects,..} if quantity.amount==Int(5) && quantity.unit==QuantityUnit::Cards && subjects.len()==5)));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[actor]["ex"], json!([]));
        assert_eq!(state[actor]["banish"].as_array().unwrap().len(), 5);
        assert_eq!(state[actor]["hand"], json!(["hand-decoy"]));
        assert_eq!(state[opponent]["ex"], json!(["foreign"]));
        assert_eq!(state["objects"]["source"]["acted"], true);
        assert_eq!(state["objects"]["source"]["counters"]["completed"], 1);
    }
}

#[test]
fn duplicate_costs_bad_counts_and_foreign_handles_preserve_the_pending_payment() {
    let program = program(2);
    let mut game = board(&program, &initial(1, &["m2a", "m2b", "m3"]));
    let question = start(&mut game, &program, 1);
    let before = exact_authority(&game);
    for handles in [
        vec![],
        vec![0],
        vec![0, 1],
        vec![0, 0],
        vec![0, 99],
        vec![0, 1, 2],
    ] {
        assert!(matches!(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(handles)
            ),
            Err(sve_engine::EngineFailure::Invalid(_))
        ));
        assert_eq!(exact_authority(&game), before);
    }
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![0, 2]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["e1"]));
    assert_eq!(state["P1"]["banish"], json!(["e0", "e2"]));
}

#[test]
fn the_player_question_exposes_frozen_original_costs_and_rejects_duplicate_costs() {
    let program = program(2);
    let mut game = board(&program, &initial(1, &["m2a", "m2b", "m3"]));
    let question = start(&mut game, &program, 1);
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("ordinary materials")
    };
    assert_eq!(domain.members.len(), 3);
    assert_eq!(
        domain
            .original_costs
            .as_ref()
            .unwrap()
            .values()
            .copied()
            .collect::<Vec<_>>(),
        vec![2, 2, 3]
    );
    game.advance_player_input();
    let Input::Question(question) = game.input(Viewer::P1) else {
        panic!("player question")
    };
    let QuestionShape::Decision(DecisionShape::CostMaterials(selection)) = &question.shape else {
        panic!("player materials")
    };
    assert!(selection.distinct_original_costs);
    assert_eq!(
        selection
            .original_costs
            .as_ref()
            .unwrap()
            .values()
            .copied()
            .collect::<Vec<_>>(),
        vec![2, 2, 3]
    );
    assert!(
        selection
            .members
            .iter()
            .all(|member| member.original_cost != Some(9))
    );
    let before = exact_authority(&game);
    let rejected = game.submit_player(
        Viewer::P1,
        Command::Answer {
            token: question.token,
            answer: Answer::CostMaterials(vec![0, 1]),
        },
    );
    assert_eq!(rejected.kind, SubmissionKind::Rejected);
    assert_eq!(exact_authority(&game), before);
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Answer {
                token: question.token,
                answer: Answer::CostMaterials(vec![0, 2])
            }
        )
        .kind,
        SubmissionKind::Applied
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["e1"]));
}

#[test]
fn insufficient_distinct_values_restore_the_entire_activation_before_asking() {
    let program = program(3);
    let mut game = board(&program, &initial(1, &["m2a", "m2b", "m3"]));
    let before = exact_authority(&game);
    assert!(matches!(
        game.activate_internal_card(
            "source",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 }
        ),
        Err(sve_engine::EngineFailure::IllegalPlay(_))
    ));
    assert_eq!(exact_authority(&game), before);
}

#[test]
fn failure_after_materials_restores_posture_banishment_draw_counter_and_random_authority() {
    let program=lowerer().lower(&tree(2,&json!([
        {"draw":{"n":1}},{"counter":{"on":"this","name":"completed","delta":1}},
        {"pp":{"who":"self","delta":{"times":[0x001F_FFFF_FFFF_FFFF_u64,{"plus":[1024,{"count":{"who":"self","zone":"hand"}}]}]}}}
    ]),false)).unwrap().remove("unit-follower").unwrap();
    let mut game = board(&program, &initial(1, &["m2a", "m3"]));
    let origin = game.clone();
    let question = start(&mut game, &program, 1);
    assert!(matches!(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![0, 1])
        ),
        Err(sve_engine::EngineFailure::Invalid(_))
    ));
    transactions::assert_restored(&game, &origin);
    let restored = exact_authority(&game);
    assert!(matches!(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![0, 1])
        ),
        Err(sve_engine::EngineFailure::Invalid(_))
    ));
    assert_eq!(exact_authority(&game), restored);
}

#[test]
fn a_mode_quote_requires_a_payable_distinct_subset_and_can_skip_a_duplicate_pair() {
    let program=lowerer().lower(&tree(2,&json!([
        {"choice":{"n":1,"options":[[{"life":{"who":"self","delta":1}}],[{"life":{"who":"self","delta":2}}]]}}
    ]),false)).unwrap().remove("unit-follower").unwrap();
    let mut absent = board(&program, &initial(1, &["m2a", "m2b"]));
    let before = exact_authority(&absent);
    assert!(matches!(
        absent.activate_internal_card(
            "source",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 }
        ),
        Err(sve_engine::EngineFailure::IllegalPlay(_))
    ));
    assert_eq!(exact_authority(&absent), before);
    let mut game = board(&program, &initial(1, &["m2a", "m2b", "m3"]));
    let modes = start(&mut game, &program, 1);
    assert!(matches!(modes.domain, InputDomain::Modes { .. }));
    let materials = awaiting(
        game.answer_internal_card(modes.actor, modes.token, Answer::Modes(vec![ModeId(1)]))
            .unwrap(),
    );
    assert!(matches!(materials.domain, InputDomain::CostMaterials(_)));
    resolved(
        game.answer_internal_card(
            materials.actor,
            materials.token,
            Answer::CostMaterials(vec![0, 2]),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        21
    );
}

#[test]
fn quick_quotes_enumerate_only_payable_distinct_subsets() {
    let program = lowerer()
        .lower(&tree(2, &json!([{"life":{"who":"self","delta":1}}]), true))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for available in [false, true] {
        let materials = if available {
            vec!["m2a", "m2b", "m3"]
        } else {
            vec!["m2a", "m2b"]
        };
        let mut initial = initial(2, &materials);
        initial["turn"]["active"] = json!("P1");
        let mut game = board(&program, &initial);
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let question = game.internal_quick_choice().unwrap().unwrap();
        let InputDomain::Quick { options, .. } = &question.domain else {
            panic!("Quick window")
        };
        assert_eq!(options.len(), usize::from(available));
        if available {
            let materials = awaiting(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Quick(Some(options[0].handle)),
                )
                .unwrap(),
            );
            resolved(
                game.answer_internal_card(
                    materials.actor,
                    materials.token,
                    Answer::CostMaterials(vec![0, 2]),
                )
                .unwrap(),
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["P2"]["banish"],
                json!(["e0", "e2"])
            );
        }
    }
}

#[test]
fn raw_ir_cannot_add_names_variable_distinctness_aggregates_or_hidden_materials() {
    let program = program(2);
    for modifier in 0..4 {
        let mut raw = program.card().clone();
        let ChoiceDomain::Objects { constraint, .. } =
            &mut raw.abilities[0].node.play.as_mut().unwrap().parameters[0].domain
        else {
            panic!("material constraint")
        };
        match modifier {
            0 => constraint.distinct_by = Some(ObjectProperty::Names),
            1 => {
                constraint.cardinality = Cardinality {
                    minimum: Nat(0),
                    maximum: None,
                };
                constraint.groups[0].cardinality = constraint.cardinality;
            }
            2 => constraint.aggregate_bounds.push(AggregateBound {
                property: NumericProperty::OriginalCost,
                comparison: Comparison::GreaterEqual,
                value: Expr::Literal(Int(1)),
            }),
            _ => {
                let Selector::Zone { zone, .. } = &mut constraint.groups[0].selector else {
                    panic!("material zone")
                };
                *zone = ZoneKind::Hand;
            }
        }
        if modifier == 3 {
            let error = WellFormedCardProgram::check(raw).unwrap_err();
            assert_eq!(error.reason, WellFormedReason::UnsupportedShape);
            assert!(error.ir_path.ends_with("/domain/groups/0"));
            continue;
        }
        let raw = WellFormedCardProgram::check(raw)
            .unwrap_or_else(|error| panic!("modifier {modifier}: {error:?}"));
        let mut game = Game::new(
            catalogue(),
            &initial(1, &["m2a", "m3"]),
            &Value::Null,
            &Value::Null,
            "material-distinct-raw",
        )
        .unwrap();
        let before = exact_authority(&game);
        let result = game.load_internal_card("unit-follower", &raw);
        assert!(
            matches!(result, Err(sve_engine::EngineFailure::Unsupported(_))),
            "modifier {modifier}: {result:?}"
        );
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn distinctness_composes_with_the_existing_public_sources_and_payment_actions() {
    for (action, zone) in [
        ("banish", json!("field")),
        ("banish", json!("ex")),
        ("banish", json!("cemetery")),
        ("banish", json!(["field", "ex"])),
        ("to_cemetery", json!("field")),
        ("to_cemetery", json!("ex")),
        ("return_to_hand", json!("field")),
        ("return_to_hand", json!("ex")),
        ("act", json!("field")),
        ("to_ex", json!("field")),
    ] {
        let mut document = tree(2, &json!([{"life":{"who":"self","delta":1}}]), false);
        let step =
            &mut document["cards"]["unit-follower"]["abilities"][0]["activate"]["cost"]["steps"][1];
        let mut material = step["banish"].take();
        material["sel"]["zone"] = zone.clone();
        material["sel"]["other"] = json!(true);
        *step = json!({action:material});
        let program = lowerer()
            .lower(&document)
            .unwrap()
            .remove("unit-follower")
            .unwrap();
        let mut initial = initial(1, &[]);
        let first = json!({"id":"first","card":"m2a"});
        let second = json!({"id":"second","card":"m3"});
        if zone.is_array() {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(first);
            initial["players"]["P1"]["zones"]["ex"] = json!([second]);
        } else if zone == "field" {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .extend([first, second]);
        } else {
            initial["players"]["P1"]["zones"][zone.as_str().unwrap()] = json!([first, second]);
        }
        let mut game = board(&program, &initial);
        let question = start(&mut game, &program, 1);
        let InputDomain::CostMaterials(domain) = &question.domain else {
            panic!("public materials")
        };
        assert_eq!(domain.members.len(), 2, "{action} {zone}");
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(vec![0, 1]),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        let destination = match action {
            "banish" => "banish",
            "return_to_hand" => "hand",
            "to_ex" => "ex",
            "act" => "field",
            _ => "cemetery",
        };
        for id in ["first", "second"] {
            assert!(
                state["P1"][destination]
                    .as_array()
                    .unwrap()
                    .contains(&json!(id)),
                "{action} {zone} {id}"
            );
            if action == "act" {
                assert_eq!(state["objects"][id]["acted"], true);
            }
        }
        assert_eq!(state["P1"]["leader"]["life"], 21);
    }
}

#[test]
fn missing_printed_costs_and_costless_tokens_never_enter_the_material_domain() {
    let program = program(2);
    let mut game = board(
        &program,
        &initial(1, &["missing", "missing-token", "m2a", "m2b", "m3"]),
    );
    let question = start(&mut game, &program, 1);
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("ordinary materials")
    };
    assert_eq!(
        domain
            .members
            .iter()
            .map(|member| member.card.as_deref().unwrap())
            .collect::<Vec<_>>(),
        vec!["m2a", "m2b", "m3"]
    );
    assert_eq!(
        domain
            .original_costs
            .as_ref()
            .unwrap()
            .values()
            .copied()
            .collect::<Vec<_>>(),
        vec![2, 2, 3]
    );
    let before = exact_authority(&game);
    assert!(matches!(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![0, 3]),
        ),
        Err(sve_engine::EngineFailure::Invalid(_))
    ));
    assert_eq!(exact_authority(&game), before);
}
