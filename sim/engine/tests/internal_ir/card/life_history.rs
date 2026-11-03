use super::*;
use alloc::collections::BTreeMap;

fn lower_bodies(bodies: &[Value]) -> WellFormedCardProgram {
    let mut tree = fanfare::expanded::tree().clone();
    let entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    let entries = bodies
        .iter()
        .enumerate()
        .map(|(index, body)| {
            let mut entry = entry.clone();
            entry.as_object_mut().unwrap().remove("fanfare");
            entry["sources"][0]["ability_scope"] = json!(format!("life_{index}"));
            entry["activate"] = body.clone();
            entry
        })
        .collect::<Vec<_>>();
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":entries}});
    lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}
fn initial() -> Value {
    let mut initial = setup(
        1,
        json!([
            {"id":"d0","card":"unit-spell"},{"id":"d1","card":"unit-spell"},
            {"id":"d2","card":"unit-spell"},{"id":"d3","card":"unit-spell"},
            {"id":"d4","card":"unit-spell"},{"id":"d5","card":"unit-spell"}
        ]),
    );
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"opp-draw","card":"unit-spell"}]);
    initial
}
fn fixture(initial: &Value) -> Game {
    let mut game = Game::new(
        catalog(),
        initial,
        &Value::Null,
        &Value::Null,
        "life-history",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}
fn activate_body(game: &mut Game, ability: AbilityId) {
    resolved(
        game.activate_internal_card("s", ability, PlayerId { seat: 1 })
            .unwrap(),
    );
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn history(who: &str, direction: &str) -> Value {
    json!({"window":"this_turn","event":"life_changed","who":who,"where":{"direction":direction}})
}

#[test]
fn life_history_records_zero_neither_as_healing_nor_decrease_and_resets_each_turn() {
    let mut game = fixture(&initial());
    let first = json!({"cost":{"pp":0},"do":[
        {"life":{"who":"self","delta":1}}, {"life":{"who":"self","delta":3}},
        {"life":{"who":"self","delta":-4}}, {"life":{"who":"self","delta":0}},
        {"draw":{"n":{"history_sum": {"window":"this_turn","event":"life_changed","who":"self","where":{"direction":"up"},"field":"delta"}}}},
        {"draw":{"n":{"history_count":history("self","down")}}}
    ]});
    let second =
        json!({"cost":{"pp":0},"do":[{"draw":{"n":{"history_count":history("opp","down")}}}]});
    let program = lower_bodies(&[first, second]);
    let first = program.card().abilities[0].node.id;
    let second = program.card().abilities[1].node.id;
    game.load_internal_card("unit-follower", &program).unwrap();
    activate_body(&mut game, first);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 20);
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 5);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    quick::pass(&mut game);
    resolved(
        game.activate_internal_card("f", second, PlayerId { seat: 2 })
            .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["hand"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn life_history_reads_explicit_current_turn_counts_from_the_authoritative_position() {
    let mut initial = initial();
    initial["semantic_state"]["counters_this_turn"] =
        json!({"P1":{"leader_hp_decreased":2,"leader_hp_increased":4}});
    let mut game = fixture(&initial);
    let program = lower_bodies(&[json!({"cost":{"pp":0},"do":[
        {"draw":{"n":{"history_count":history("self","down")}}},
        {"draw":{"n":{"history_sum":{"window":"this_turn","event":"life_changed","who":"self","where":{"direction":"up"},"field":"delta"}}}}
    ]})]);
    let ability = program.card().abilities[0].node.id;
    game.load_internal_card("unit-follower", &program).unwrap();
    activate_body(&mut game, ability);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
}

#[test]
fn failed_life_payment_rolls_back_the_life_history_alongside_resources() {
    let mut initial = initial();
    initial["players"]["P1"]["leader"]["life"] = json!(3);
    initial["players"]["P1"]["pp"] = json!({"current":2,"max":2});
    let mut game = fixture(&initial);
    let program = lower_bodies(&[
        json!({"cost":{"steps":[{"leader_life":2},{"pp":3}]},"do":[]}),
        json!({"cost":{"pp":0},"do":[{"draw":{"n":{"history_count":history("self","down")}}}]}),
    ]);
    let failed = program.card().abilities[0].node.id;
    let reader = program.card().abilities[1].node.id;
    game.load_internal_card("unit-follower", &program).unwrap();
    let before = exact_authority(&game);
    game.activate_internal_card("s", failed, PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(exact_authority(&game), before);
    activate_body(&mut game, reader);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn life_history_rejects_unhandled_filters_windows_and_directionless_sums() {
    let mut tree = fanfare::expanded::spell_tree().clone();
    let baseline = history("self", "down");
    let mut queries = Vec::new();
    for (key, value) in [
        ("window", json!("game")),
        ("event", json!("played")),
        ("distinct", json!("subjects")),
        ("sel", json!({"name":"hidden"})),
        ("exclude", json!("this_execution")),
        ("where", json!({"direction":"up"})),
        ("where", json!({"direction":"down","by":{"kind":"ability"}})),
        (
            "where",
            json!({"direction":"down","during":{"turn":"self"}}),
        ),
    ] {
        let mut query = baseline.clone();
        query[key] = value;
        queries.push(query);
    }
    for missing in ["who", "where"] {
        let mut query = baseline.clone();
        query.as_object_mut().unwrap().remove(missing);
        queries.push(query);
    }
    for query in queries {
        tree["cards"]["SD03-005"]["abilities"][0]["spell"]["do"] =
            json!([{"draw":{"n":{"history_count":query}}}]);
        lowerer()
            .lower_with_printed_costs(&tree, &BTreeMap::from([("SD03-005".to_owned(), Nat(1))]))
            .unwrap_err();
    }
    let mut sum = history("self", "up");
    sum["field"] = json!("delta");
    let mut down = sum.clone();
    down["where"]["direction"] = json!("down");
    let mut no_player = sum.clone();
    no_player.as_object_mut().unwrap().remove("who");
    sum.as_object_mut().unwrap().remove("where");
    for query in [down, no_player, sum] {
        tree["cards"]["SD03-005"]["abilities"][0]["spell"]["do"] =
            json!([{"life":{"who":"self","delta":{"history_sum":query}}}]);
        lowerer()
            .lower_with_printed_costs(&tree, &BTreeMap::from([("SD03-005".to_owned(), Nat(1))]))
            .unwrap_err();
    }
}

#[test]
fn life_payments_outside_a_required_trigger_head_remain_rejected() {
    for (head, mode) in [("spell", false), ("fanfare", false), ("spell", true)] {
        for (cost, allowed) in [(json!({"pp":0}), true), (json!({"leader_life":1}), false)] {
            let body = if mode {
                json!({"do":[{"choice":{"upto":1,"options":[
                    [{"pay":{"cost":cost,"do":[{"draw":{"n":1}}]}}],
                    [{"draw":{"n":1}}]
                ]}}]})
            } else {
                let mut cost = cost;
                cost["result"] = json!("paid");
                json!({"optional_cost":cost,"do":[]})
            };
            let mut tree = fanfare::expanded::tree().clone();
            let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
            entry.as_object_mut().unwrap().remove("fanfare");
            entry[head] = body;
            let meta = tree["cards"]["BP08-038"]["meta"].clone();
            let number = if head == "spell" {
                "unit-spell"
            } else {
                "unit-follower"
            };
            tree["cards"] = json!({number:{"meta":meta,"abilities":[entry]}});
            let program = lowerer()
                .lower_with_printed_costs(&tree, &BTreeMap::from([(number.to_owned(), Nat(1))]))
                .unwrap()
                .remove(number)
                .unwrap();
            let mut game = Game::new(
                catalog(),
                &initial(),
                &Value::Null,
                &Value::Null,
                "life-head-boundary",
            )
            .unwrap();
            let result = game.load_internal_card(number, &program);
            if allowed {
                result.unwrap();
            } else {
                result.unwrap_err();
            }
        }
    }
}
