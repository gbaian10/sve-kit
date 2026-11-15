use super::*;

fn ordinal_tree() -> Value {
    let mut tree = equipment_played::tree(
        &json!({"auto":{
            "on":{"played":{"who":"self","sel":{"kind":"spell","where":{"cmp":{
                "left":{"history_count":{"window":"this_turn","event":"played","who":"self","sel":{"kind":"spell"}}},
                "op":"eq","right":3
            }}}}},
            "do":[{"life":{"who":"self","delta":1}}]
        }}),
        "unit-amulet",
    );
    tree["cards"]["unit-amulet"]["abilities"][0]["line"] = json!(1);
    tree
}

#[test]
fn a_filtered_play_ordinal_condition_lowers_and_loads() {
    let programs = lowerer().lower(&ordinal_tree()).unwrap();
    game(1, 0)
        .load_internal_card("unit-amulet", &programs["unit-amulet"])
        .unwrap();
}

fn catalog_with_leaf() -> Arc<Catalog> {
    let (mut snapshot, keywords, document) = catalog_documents(&json!("1"));
    let mut row: Value = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|row| row["number"] == "unit-spell")
        .unwrap();
    row["number"] = json!("leaf");
    row["faces"][0]["name"] = json!("leaf");
    snapshot.push('\n');
    snapshot.push_str(&row.to_string());
    let mut document: Value = serde_json::from_str(&document).unwrap();
    document["cards"]["leaf"] = document["cards"]["unit-spell"].clone();
    Arc::new(
        Catalog::from_documents(
            &snapshot,
            &keywords,
            &[("unit.yaml".into(), document.to_string())],
        )
        .unwrap(),
    )
}

fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn nested_board() -> (Game, AbilityId) {
    use alloc::collections::BTreeMap;
    let mut tree = ordinal_tree();
    let extra = equipment_played::tree(
        &json!({"line":2,"activate":{"cost":{"pp":0},"do":[{
            "life":{"who":"self","delta":{"history_count":{"window":"this_turn","event":"played","who":"self","sel":{"kind":"spell"}}}}
        }]}}),
        "unit-amulet",
    );
    tree["cards"]["unit-amulet"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(extra["cards"]["unit-amulet"]["abilities"][0].clone());
    let observer = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let reader = observer
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let parent_tree = equipment_played::tree(
        &json!({"spell":{
            "targets":[{"bind":"child","from":{"who":"self","zone":"cemetery","kind":"spell","name":"leaf"},"n":1}],
            "do":[{"play_card":{"ref":"child","cost":0}}]
        }}),
        "unit-spell",
    );
    let parent = lowerer()
        .lower_with_printed_costs(
            &parent_tree,
            &BTreeMap::from([("unit-spell".into(), Nat(1))]),
        )
        .unwrap()
        .remove("unit-spell")
        .unwrap();
    let leaf_tree = equipment_played::tree(&json!({"spell":{"do":[]}}), "leaf");
    let leaf = lowerer()
        .lower_with_printed_costs(&leaf_tree, &BTreeMap::from([("leaf".into(), Nat(1))]))
        .unwrap()
        .remove("leaf")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["leader"]["life"] = json!(10);
    initial["players"]["P1"]["pp"] = json!({"current":10,"max":10});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"watcher","card":"unit-amulet"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"first","card":"leaf"},{"id":"second","card":"leaf"},{"id":"parent","card":"unit-spell"}
    ]);
    let mut game = Game::new(
        catalog_with_leaf(),
        &initial,
        &Value::Null,
        &Value::Null,
        "ordinal-nested",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &observer).unwrap();
    game.load_internal_card("unit-spell", &parent).unwrap();
    game.load_internal_card("leaf", &leaf).unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    (game, reader)
}

#[test]
fn the_third_play_stays_pending_after_its_body_plays_a_fourth_spell() {
    use sve_engine::game::internal::InputDomain;
    let (mut game, reader) = nested_board();
    for id in ["first", "second"] {
        resolved(game.play_internal_card(id, PlayerId { seat: 1 }).unwrap());
        checks(&mut game);
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
    let CardPlay::Awaiting(target) = game
        .play_internal_card("parent", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("child target");
    };
    resolved(
        game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    checks(&mut game);
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("ordinal pending");
    };
    assert_eq!(options.len(), 1);
    resolved(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    checks(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        11
    );
    assert!(game.internal_pending_choice().unwrap().is_none());
    resolved(
        game.activate_internal_card("watcher", reader, PlayerId { seat: 1 })
            .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        15
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        7
    );
}

#[test]
fn ordinal_conditions_keep_zero_other_players_and_other_history_events_closed() {
    let valid = ordinal_tree();
    let base = "/cards/unit-amulet/abilities/0/auto/on/played/sel/where/cmp";
    for (suffix, value) in [
        ("/right", json!(0)),
        ("/left/history_count/who", json!("opp")),
        ("/left/history_count/event", json!("evolved")),
        ("/op", json!("ge")),
    ] {
        let mut candidate = valid.clone();
        *candidate.pointer_mut(&format!("{base}{suffix}")).unwrap() = value;
        lowerer().lower(&candidate).unwrap_err();
    }
    let programs = lowerer().lower(&valid).unwrap();
    let mut raw = programs["unit-amulet"].card().clone();
    let TriggerCondition::Event {
        pattern: EventPattern::Play { predicate, .. },
    } = &mut raw.abilities[0].node.trigger.as_mut().unwrap().on
    else {
        panic!("play trigger");
    };
    let Expr::Compute(outer) = predicate else {
        panic!("guard");
    };
    let Predicate::And { predicates } = outer.as_mut() else {
        panic!("guard conjunction");
    };
    let Expr::Compute(guard) = &mut predicates[1] else {
        panic!("history guard");
    };
    let Predicate::CompareNat { right, .. } = guard.as_mut() else {
        panic!("ordinal");
    };
    *right = Expr::Literal(Nat(0));
    let rejected = WellFormedCardProgram::check(raw).map_or(true, |program| {
        game(1, 0)
            .load_internal_card("unit-amulet", &program)
            .is_err()
    });
    assert!(rejected);
}

fn cost_program() -> WellFormedCardProgram {
    let tree = equipment_played::tree(
        &json!({"static":{
            "while":{"cmp":{"left":{"history_count":{"window":"this_turn","event":"played","who":"self","sel":{"kind":"spell","trait":"runner"}}},"op":"eq","right":0}},
            "effects":[{"cost_mod":{"when_playing":{"who":"self","kind":"spell","trait":"runner"},"delta":-1}}]
        }}),
        "unit-amulet",
    );
    lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-amulet")
        .unwrap()
}

fn quick_spell(cost: u64) -> WellFormedCardProgram {
    use alloc::collections::BTreeMap;
    let mut tree = equipment_played::tree(&json!({"line":1,"keywords":["quick"]}), "unit-spell");
    tree["cards"]["unit-spell"]["abilities"][0]["sources"][0]
        .as_object_mut()
        .unwrap()
        .remove("ability_scope");
    let body = equipment_played::tree(&json!({"line":2,"spell":{"do":[]}}), "unit-spell");
    tree["cards"]["unit-spell"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(body["cards"]["unit-spell"]["abilities"][0].clone());
    lowerer()
        .lower_with_printed_costs(&tree, &BTreeMap::from([("unit-spell".into(), Nat(cost))]))
        .unwrap()
        .remove("unit-spell")
        .unwrap()
}

fn cost_board(seat: u32, late: bool, copies: usize, cost: u64) -> Game {
    let (snapshot, keywords, document) = catalog_documents(&json!(cost.to_string()));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            row["faces"][0]["traits"] = json!(["runner"]);
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &keywords, &[("unit.yaml".into(), document)]).unwrap(),
    );
    let own = if seat == 1 { "P1" } else { "P2" };
    let active = if late {
        own
    } else if seat == 1 {
        "P2"
    } else {
        "P1"
    };
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(active);
    initial["players"][own]["pp"] = json!({"current":if late {3} else {0},"max":10});
    initial["players"][own]["zones"]["hand"] = json!([
        {"id":"first","card":"unit-spell"},{"id":"second","card":"unit-spell"}
    ]);
    initial["players"][own]["zones"]["field"] = json!([]);
    for index in 0..copies {
        initial["players"][own]["zones"][if late { "hand" } else { "field" }]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":format!("provider-{index}"),"card":"unit-amulet"}));
    }
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "ordinal-cost",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &cost_program())
        .unwrap();
    game.load_internal_card("unit-spell", &quick_spell(cost))
        .unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    game
}

#[test]
fn first_matching_discounts_stack_in_opponent_quick_at_zero_pp_and_stop_after_one_play() {
    use sve_engine::game::internal::InputDomain;
    for seat in [1, 2] {
        for (copies, cost) in [(1, 1), (2, 2)] {
            let mut game = cost_board(seat, false, copies, cost);
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            let quick = game.internal_quick_choice().unwrap().unwrap();
            assert_eq!(quick.actor, PlayerId { seat });
            let InputDomain::Quick { options, .. } = &quick.domain else {
                panic!("Quick");
            };
            assert_eq!(options.len(), 2);
            resolved(
                game.answer_internal_card(
                    quick.actor,
                    quick.token,
                    Answer::Quick(Some(options[0].handle)),
                )
                .unwrap(),
            );
            checks(&mut game);
            let next = game.internal_quick_choice().unwrap().unwrap();
            let InputDomain::Quick { options, .. } = &next.domain else {
                panic!("next Quick");
            };
            assert_eq!(options.as_slice(), []);
            let own = if seat == 1 { "P1" } else { "P2" };
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after[own]["pp"]["current"], 0);
            assert_eq!(after[own]["hand"].as_array().unwrap().len(), 1);
        }
    }
}

#[test]
fn an_earlier_matching_play_is_counted_before_the_provider_enters() {
    let mut game = cost_board(1, true, 1, 1);
    resolved(
        game.play_internal_card("first", PlayerId { seat: 1 })
            .unwrap(),
    );
    checks(&mut game);
    assert!(matches!(
        game.play_internal_card("provider-0", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    checks(&mut game);
    resolved(
        game.play_internal_card("second", PlayerId { seat: 1 })
            .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        0
    );
}
