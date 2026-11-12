//! Author formulas retain the existing counter instruction and payment authority.
use super::*;
use sve_engine::game::internal::InputDomain;

fn program(steps: &Value) -> WellFormedCardProgram {
    lowerer()
        .lower(&equipment_played::tree(
            &json!({"activate":{"cost":{"pp":0},"do":steps}}),
            "unit-follower",
        ))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn counter(delta: &Value) -> Value {
    json!({"counter":{"on":"this","name":"test","delta":delta}})
}

fn position(seat: u32, count: i64) -> Value {
    let actor = if seat == 1 { "P1" } else { "P2" };
    let mut initial = setup(seat, json!([{"id":"d","card":"unit-spell"}]));
    initial["turn"]["active"] = json!(actor);
    initial["players"][actor]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower","state":{"counters":{"test":count}}}]);
    initial["players"][actor]["zones"]["hand"] = json!([]);
    initial
}

fn board(program: &WellFormedCardProgram, initial: &Value) -> Game {
    let mut game = Game::new(
        catalog(),
        initial,
        &Value::Null,
        &Value::Null,
        "counter-author",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

fn changes(result: &Activation) -> Vec<(Nat, Nat, Int)> {
    result
        .progress
        .receipts
        .values()
        .filter_map(|outcome| {
            if let InstructionOutcome::Counter(event) = outcome {
                Some((event.before, event.after, event.delta))
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn signed_formulas_read_after_draw_and_preserve_zero_and_saturating_removal() {
    let count = json!({"count":{"who":"self","zone":"hand"}});
    for (delta, draw, expected) in [
        (count.clone(), false, 2),
        (json!({"plus":[count,1]}), true, 4),
        (json!({"min":[count,2]}), true, 3),
        (json!({"times":[count,2]}), true, 4),
        (json!({"minus":[0,count]}), true, 1),
        (json!({"times":[count,-3]}), true, 0),
    ] {
        let mut steps = vec![];
        if draw {
            steps.push(json!({"draw":{"n":1}}));
        }
        steps.push(counter(&delta));
        let program = program(&json!(steps));
        for seat in [1, 2] {
            let mut game = board(&program, &position(seat, 2));
            let result = resolved(
                game.activate_internal_card(
                    "s",
                    program.card().abilities[0].node.id,
                    PlayerId { seat },
                )
                .unwrap(),
            );
            assert_eq!(
                changes(&result),
                vec![(
                    Nat(2),
                    Nat(expected),
                    Int(i64::try_from(expected).unwrap() - 2)
                )]
            );
        }
    }
}

#[test]
fn a_multi_holder_formula_uses_one_start_view_of_the_source_counter() {
    let program = program(&json!([
        {"counter":{"on":{"who":"self","zone":"field","kind":"follower"},"name":"test","delta":{"plus":[{"counter":{"on":"this","name":"test"}},1]}}}
    ]));
    let mut initial = position(1, 2);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"ally","card":"unit-follower","state":{"counters":{"test":7}}}));
    let mut game = board(&program, &initial);
    let result = resolved(
        game.activate_internal_card(
            "s",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    assert_eq!(changes(&result), vec![(Nat(7), Nat(10), Int(3))]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["s"]["counters"]["test"], 5);
    assert_eq!(state["objects"]["ally"]["counters"]["test"], 10);
}

#[test]
fn subsequent_counter_formulas_read_the_latest_value() {
    let read = json!({"counter":{"on":"this","name":"test"}});
    let program = program(&json!([
        counter(&json!({"plus":[read,1]})),
        counter(&json!({"minus":[0,read]}))
    ]));
    let mut game = board(&program, &position(1, 2));
    let result = resolved(
        game.activate_internal_card(
            "s",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    assert_eq!(
        changes(&result),
        vec![(Nat(2), Nat(5), Int(3)), (Nat(5), Nat(0), Int(-5))]
    );
}

#[test]
fn paid_pp_x_reaches_the_counter_without_another_decision() {
    let program = lowerer()
        .lower(&equipment_played::tree(
            &json!({"activate":{"x":{},"cost":{"pp":"x"},"do":[counter(&json!("x"))]}}),
            "unit-follower",
        ))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for x in [0_u64, 4] {
        let mut initial = position(1, 0);
        initial["players"]["P1"]["pp"] = json!({"current":4,"max":4});
        let mut game = board(&program, &initial);
        let CardPlay::Awaiting(question) = game
            .activate_internal_card(
                "s",
                program.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap()
        else {
            panic!("paid X")
        };
        assert!(matches!(question.domain, InputDomain::Integer(_)));
        let result = resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Integer(Nat(x)))
                .unwrap(),
        );
        assert_eq!(
            changes(&result),
            vec![(Nat(0), Nat(x), Int(i64::try_from(x).unwrap()))]
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            4 - x
        );
        assert!(game.internal_decision(View::P1).is_none());
    }
}

#[test]
fn fusion_paid_members_feed_the_author_counter_after_ex_carry() {
    let program = lowerer().lower(&evolution_linked_movement::tree(&[("activate",json!({
        "fusion":{"sel":{"who":"self","zone":["hand","ex"],"kind":"spell"},"upto":2,"result":"paid"},
        "do":[counter(&json!({"count":{"in":{"objects_of":"paid","view":"before"},"kind":"spell"}}))]
    }))])).unwrap().remove("unit-follower").unwrap();
    let mut initial = position(1, 0);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"s","card":"unit-follower"},{"id":"h","card":"unit-spell"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"e","card":"unit-spell"}]);
    let mut game = board(&program, &initial);
    let CardPlay::Awaiting(question) = game
        .activate_internal_card(
            "s",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap()
    else {
        panic!("Fusion materials")
    };
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("Fusion materials")
    };
    let result = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(domain.members.iter().map(|m| m.handle).collect()),
        )
        .unwrap(),
    );
    assert_eq!(changes(&result), vec![(Nat(0), Nat(2), Int(2))]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["s"]));
    assert_eq!(state["objects"]["s"]["counters"]["test"], 2);
    assert!(game.internal_decision(View::P1).is_none());
}

#[test]
fn a_formula_overflow_restores_the_draw_and_all_counter_authority() {
    let program = program(&json!([
        {"draw":{"n":1}},counter(&json!(1)),
        counter(&json!({"times":[0x001F_FFFF_FFFF_FFFF_u64,{"plus":[1024,{"count":{"who":"self","zone":"hand"}}]}]}))
    ]));
    let mut game = board(&program, &position(1, 2));
    let before = exact_authority(&game);
    assert!(matches!(
        game.activate_internal_card(
            "s",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 }
        ),
        Err(sve_engine::EngineFailure::Invalid(_))
    ));
    assert_eq!(exact_authority(&game), before);
}

#[test]
fn unknown_signed_formulas_and_minimum_integer_keep_the_author_path_in_rejection() {
    for delta in [
        json!({"minus":[{"count":{"who":"self","zone":"hand"}},1]}),
        json!({"minus":[{"count":{"who":"self","zone":"hand"}},{"count":{"who":"self","zone":"ex"}}]}),
        json!({"power_of":"this"}),
    ] {
        let tree = equipment_played::tree(
            &json!({"activate":{"cost":{"pp":0},"do":[counter(&delta)]}}),
            "unit-follower",
        );
        let error = lowerer().lower(&tree).unwrap_err();
        assert_eq!(error.code, "unsupported_construct");
        assert!(error.path.contains("/do/0"), "{}", error.path);
    }
    let minimum = equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0},"do":[counter(&json!(i64::MIN))]}}),
        "unit-follower",
    );
    let error = lowerer().lower(&minimum).unwrap_err();
    assert_eq!(error.code, "invalid_expanded");
    assert!(
        error.path.starts_with("/cards/unit-follower"),
        "{}",
        error.path
    );
    let tree = equipment_played::tree(
        &json!({"activate":{"cost":{"counter":{"on":"this","name":"test","n":{"plus":[1,1]}}},"do":[]}}),
        "unit-follower",
    );
    let error = lowerer().lower(&tree).unwrap_err();
    assert_eq!(
        error.code, "unsupported_construct",
        "counter payment remains a literal quantity"
    );
}

#[test]
fn a_formal_counter_holder_accepts_a_formula_without_changing_its_source() {
    let program = lowerer().lower(&equipment_played::tree(&json!({"activate":{
        "cost":{"pp":0},
        "targets":[{"bind":"chosen","from":{"who":"self","zone":"ex","kind":"follower"},"n":1}],
        "do":[{"counter":{"on":"chosen","name":"test","delta":{"plus":[{"count":{"who":"self","zone":"hand"}},1]}}}]
    }}),"unit-follower")).unwrap().remove("unit-follower").unwrap();
    let mut initial = position(1, 2);
    initial["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"ally","card":"unit-follower","state":{"counters":{"test":7}}}]);
    let mut game = board(&program, &initial);
    let CardPlay::Awaiting(question) = game
        .activate_internal_card(
            "s",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap()
    else {
        panic!("formal counter holder")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("formal counter holder")
    };
    assert_eq!(domain.members.len(), 1);
    let result = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    assert_eq!(changes(&result), vec![(Nat(7), Nat(8), Int(1))]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["s"]["counters"]["test"], 2);
    assert_eq!(state["objects"]["ally"]["counters"]["test"], 8);
}

#[test]
fn a_formula_cannot_write_to_a_source_that_left_field_and_rolls_back_its_move() {
    let program = program(&json!([
        {"move":{"ref":"this","to":"cemetery"}},
        counter(&json!({"plus":[{"count":{"who":"self","zone":"hand"}},1]}))
    ]));
    let mut game = board(&program, &position(1, 2));
    let before = exact_authority(&game);
    assert!(matches!(
        game.activate_internal_card(
            "s",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 }
        ),
        Err(sve_engine::EngineFailure::Unsupported(_))
    ));
    assert_eq!(exact_authority(&game), before);
}
