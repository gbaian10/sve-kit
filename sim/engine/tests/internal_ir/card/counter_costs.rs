use super::*;
use sve_engine::EngineFailure;
use sve_engine::game::internal::InputDomain;

fn expanded(counter: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["activate"] = json!({"cost":{"steps":[{"pp":1_u8},{"counter":counter}]},
        "do":[{"draw":{"who":"self","n":1_u8}}]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    tree
}

fn counter() -> Value {
    json!({"on":{"who":"self","zone":"field","name":"unit-amulet"},
        "holders":1_u8,"name":"test","n":2_u8})
}

fn program() -> WellFormedCardProgram {
    lowerer()
        .lower(&expanded(&counter()))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn position(available: bool) -> Game {
    let mut initial = setup(1, json!([{"id":"d","card":"unit-spell"}]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"s","card":"unit-follower","state":{"counters":{"test":99_u8}}},
        {"id":"poor","card":"unit-amulet","state":{"counters":{"test":1_u8}}}
    ]);
    if available {
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"pool","card":"unit-amulet","state":{"counters":{"test":3_u8}}}));
    }
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"ex","card":"unit-amulet","state":{"counters":{"test":99_u8}}}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"foreign","card":"unit-amulet","state":{"counters":{"test":99_u8}}}
    ]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "selected-counter-cost",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program())
        .unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

#[test]
fn named_counter_material_filters_balance_and_spends_only_the_selected_holder() {
    let mut game = position(true);
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("counter material choice");
    };
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("material domain");
    };
    assert_eq!(
        domain.members.len(),
        1,
        "wrong name, zone, controller and insufficient balance are excluded"
    );
    assert_eq!(domain.members[0].card.as_deref(), Some("unit-amulet"));
    let result = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![domain.members[0].handle]),
        )
        .unwrap();
    assert!(
        matches!(result, CardPlay::Resolved(_)),
        "payment precedes the draw body"
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["pool"]["counters"]["test"], 1_u8);
    assert_eq!(view["objects"]["poor"]["counters"]["test"], 1_u8);
    for id in ["s", "ex", "foreign"] {
        assert_eq!(view["objects"][id]["counters"]["test"], 99_u8);
    }
    assert_eq!(view["P1"]["pp"]["current"], 1_u8);
    assert_eq!(view["P1"]["hand"], json!(["d"]));
}

#[test]
fn insufficient_named_counter_material_rolls_back_pp_and_does_not_use_other_pools() {
    let mut game = position(false);
    let before = exact_authority(&game);
    assert!(
        matches!(
            game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 }),
            Err(EngineFailure::IllegalPlay(_))
        ),
        "the whole named amount is required before payment"
    );
    assert!(
        exact_authority(&game) == before,
        "failed material admission preserves the whole play"
    );
}

#[test]
fn counter_cost_lowering_rejects_other_players_non_public_zones_and_non_singleton_holders() {
    let mut wrong_player = counter();
    wrong_player["on"]["who"] = json!("opp");
    let mut wrong_zone = counter();
    wrong_zone["on"]["zone"] = json!("cemetery");
    let mut many = counter();
    many["holders"] = json!(2_u8);
    let mut absent = counter();
    absent.as_object_mut().unwrap().remove("holders");
    let mut self_with_holders = counter();
    self_with_holders["on"] = json!("this");
    for invalid in [wrong_player, wrong_zone, many, absent, self_with_holders] {
        lowerer()
            .lower(&expanded(&invalid))
            .expect_err("unsupported cost holder shape must fail closed");
    }
}

#[test]
fn hand_written_counter_costs_cannot_reuse_an_opponent_material_parameter() {
    let mut raw = program().card().clone();
    let play = raw
        .abilities
        .first_mut()
        .unwrap()
        .node
        .play
        .as_mut()
        .unwrap();
    let ChoiceDomain::Objects { constraint, .. } = &mut play.parameters[0].domain else {
        panic!("material");
    };
    let Selector::Zone { players, .. } = &mut constraint.groups[0].selector else {
        panic!("zone");
    };
    players.relation = PlayerRelation::All;
    players.cardinality.maximum = Some(Nat(2));
    let checked = WellFormedCardProgram::check(raw).unwrap();
    let initial = setup(1, json!([]));
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "invalid-counter-cost",
    )
    .unwrap();
    let error = game
        .load_internal_card("unit-follower", &checked)
        .expect_err("opposing counters cannot pay this cost slice");
    assert!(
        matches!(error, EngineFailure::Unsupported(message) if message.contains("whole-card cost step")),
        "the cost holder guard rejects the declaration"
    );
}
