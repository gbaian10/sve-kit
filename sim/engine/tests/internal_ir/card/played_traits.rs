#![expect(
    clippy::panic,
    reason = "Unexpected program and choice shapes invalidate these history probes."
)]
use super::*;
use sve_engine::game::internal::{InputDomain, ResumeStage};
use sve_engine::game::player::Viewer;

fn tree(filter: Option<Value>) -> Value {
    let mut query = json!({"event":"played","window":"this_turn","who":"self"});
    if let Some(filter) = filter {
        query["sel"] = filter;
    }
    equipment_played::tree(
        &json!({"activate":{
            "cost":{"pp":0},"do":[{"life":{"who":"self","delta":{"history_count":query}}}]
        }}),
        "unit-follower",
    )
}
fn lower_and_load(input: &Value) {
    let programs = lowerer().lower(input).unwrap();
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "played-traits-lowering",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
}

#[test]
fn unfiltered_current_turn_played_history_is_a_valid_lower_load_baseline() {
    lower_and_load(&tree(None));
}

#[test]
fn one_trait_current_turn_played_history_must_lower_and_load() {
    lower_and_load(&tree(Some(json!({"trait":"宴楽"}))));
}

#[test]
fn union_of_two_traits_current_turn_played_history_must_lower_and_load() {
    lower_and_load(&tree(Some(json!({"traits_any":["宴楽","魔法使い"]}))));
}

#[test]
fn complete_normal_and_evolved_shapes_must_compose_entry_method_and_history_reads() {
    let count = json!({"event":"played","window":"this_turn","who":"self"});
    let choice = json!({"either":{"options":[
        [{"grant":{"on":"this","keywords":["storm"]}}],
        [{"grant":{"on":"this","keywords":["drain"]}}]
    ]}});
    let threshold = |n: u8| {
        json!({"if":{"cond":{"history":{
        "event":"played","window":"this_turn","who":"self","at_least":n
    }},"then":[choice.clone()]}})
    };
    let mut input = equipment_played::tree(
        &json!({"line":2,"fanfare":{"do":[
            {"if":{"cond":{"entered_by":{"ref":"this","by":"play"}},"then":[threshold(2)],"else":[threshold(1)]}}
        ]}}),
        "unit-follower",
    );
    let intrinsic = equipment_played::tree(
        &json!({"line":1,"evolve":{"cost":{"pp":1}}}),
        "unit-follower",
    )["cards"]["unit-follower"]["abilities"][0]
        .clone();
    input["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .insert(0, intrinsic);
    let form = equipment_played::tree(
        &json!({"line":1,"evolve_time":{
            "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
            "do":[{"damage":{"to":"enemy","n":{"times":[2,{"history_count":count}]}}}]
        }}),
        "unit-form",
    );
    input["cards"]["unit-form"] = form["cards"]["unit-form"].clone();
    let programs = lowerer().lower(&input).unwrap();
    evolution::position(1, &programs["unit-follower"], &programs["unit-form"]);
}

#[test]
fn entry_method_without_history_is_a_valid_lower_load_baseline() {
    let input = equipment_played::tree(
        &json!({"fanfare":{"do":[
            {"if":{"cond":{"entered_by":{"ref":"this","by":"play"}},"then":[
                {"grant":{"on":"this","keywords":["storm"]}}
            ],"else":[{"grant":{"on":"this","keywords":["drain"]}}]}}
        ]}}),
        "unit-follower",
    );
    lower_and_load(&input);
}

#[test]
fn spell_kind_current_turn_played_history_must_lower_and_load() {
    lower_and_load(&tree(Some(json!({"kind":"spell"}))));
}

#[test]
fn union_burst_evolution_body_and_selected_controller_load_with_unfiltered_history() {
    let mut input = equipment_played::tree(
        &json!({"line":1,"evolve":{"cost":{"pp":1}}}),
        "unit-follower",
    );
    let form = equipment_played::tree(
        &json!({"line":1,"evolve_time":{
            "ub":true,
            "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
            "do":[
                {"damage":{"to":"enemy","n":2}},
                {"if":{"cond":{"history":{"event":"played","window":"this_turn","who":"self"}},"then":[
                    {"damage":{"to":{"leader_of":"enemy"},"n":2}}
                ]}}
            ]
        }}),
        "unit-form",
    );
    input["cards"]["unit-form"] = form["cards"]["unit-form"].clone();
    let programs = lowerer().lower(&input).unwrap();
    evolution::position(1, &programs["unit-follower"], &programs["unit-form"]);
}

fn history_reader(filter: Option<Value>) -> WellFormedCardProgram {
    lowerer()
        .lower(&tree(filter))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}
fn traits_catalog(missing: bool, token: bool) -> Arc<Catalog> {
    let (snapshot, keywords, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            row["faces"][0]["traits"] = match row["number"].as_str().unwrap() {
                "unit-follower" => json!(["宴楽", "魔法使い"]),
                "unit-amulet" => json!(["宴楽"]),
                _ => json!(["魔法使い"]),
            };
            if token && row["number"] == "unit-amulet" {
                row["faces"][0]["card_type"] = json!("フォロワー・トークン");
                row["faces"][0]["power"] = json!("1");
                row["faces"][0]["hp"] = json!("1");
            }
            if missing {
                row["faces"][0].as_object_mut().unwrap().remove("traits");
                row["faces"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("card_class");
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    Arc::new(
        Catalog::from_documents(&snapshot, &keywords, &[("unit.yaml".into(), document)]).unwrap(),
    )
}
fn initial() -> Value {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":10,"max":10});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"reader","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"spell","card":"unit-spell"},{"id":"amulet","card":"unit-amulet"},
        {"id":"dual","card":"unit-follower"}
    ]);
    initial
}
fn position(initial: &Value, reader: &WellFormedCardProgram, missing: bool) -> Game {
    position_with_tokens(initial, reader, missing, false)
}
fn position_with_tokens(
    initial: &Value,
    reader: &WellFormedCardProgram,
    missing: bool,
    token: bool,
) -> Game {
    let mut game = Game::new(
        traits_catalog(missing, token),
        initial,
        &Value::Null,
        &Value::Null,
        "played-trait-history",
    )
    .unwrap();
    game.load_internal_card("unit-follower", reader).unwrap();
    game.load_internal_card("unit-spell", &card(vec![play::spell(vec![])]))
        .unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    game
}
fn play_card(game: &mut Game, id: &str, seat: u32) {
    assert!(
        matches!(
            game.play_internal_card(id, PlayerId { seat }).unwrap(),
            CardPlay::Entered(_) | CardPlay::Resolved(_)
        ),
        "a successful proper Play commits before returning"
    );
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn read_count(game: &mut Game, program: &WellFormedCardProgram, expected: u64) {
    let before = game.projection(View::Referee).unwrap()["P1"]["leader"]["life"]
        .as_u64()
        .unwrap();
    resolved(
        game.activate_internal_card(
            "reader",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        before.checked_add(expected).unwrap(),
        "the activated reader gains exactly the committed filtered count"
    );
}

#[test]
fn committed_play_facts_filter_kind_and_traits_and_count_a_dual_trait_card_once() {
    for (filter, expected) in [
        (json!({"traits_any":["宴楽","魔法使い"]}), 3),
        (json!({"trait":"宴楽"}), 2),
        (json!({"kind":"spell"}), 1),
        (json!({"name":"unit-spell"}), 1),
        (json!({"kind":"spell","name":"unit-spell"}), 1),
        (
            json!({"name":"unit-amulet","trait":"宴楽","not_token":true}),
            1,
        ),
    ] {
        let program = history_reader(Some(filter));
        let mut game = position(&initial(), &program, false);
        for id in ["spell", "amulet", "dual"] {
            play_card(&mut game, id, 1);
        }
        read_count(&mut game, &program, expected);
        read_count(&mut game, &program, expected);
    }
}

#[test]
fn late_entering_reader_sees_plays_without_observers_and_departed_spell() {
    let reader = history_reader(Some(json!({"traits_any":["宴楽","魔法使い"]})));
    let mut initial = initial();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"reader","card":"unit-follower"}));
    let mut game = position(&initial, &reader, false);
    for id in ["spell", "amulet", "dual"] {
        play_card(&mut game, id, 1);
    }
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["spell"])
    );
    play_card(&mut game, "reader", 1);
    read_count(&mut game, &reader, 4);
}

#[test]
fn incomplete_explicit_history_refuses_filtered_queries_but_preserves_unfiltered_counts() {
    for filter in [
        None,
        Some(json!({"trait":"宴楽"})),
        Some(json!({"kind":"follower","trait":"宴楽","token":true})),
    ] {
        let filtered = filter.is_some();
        let reader = history_reader(filter);
        let mut initial = initial();
        initial["semantic_state"]["counters_this_turn"]["P1"]["cards_played"] = json!(3);
        let mut game = position(&initial, &reader, false);
        if filtered {
            let before = game.projection(View::Referee).unwrap();
            let error = game
                .activate_internal_card(
                    "reader",
                    reader.card().abilities[0].node.id,
                    PlayerId { seat: 1 },
                )
                .unwrap_err();
            assert!(format!("{error:?}").contains("incomplete filtered played-card history"));
            assert_eq!(game.projection(View::Referee).unwrap(), before);
        } else {
            read_count(&mut game, &reader, 3);
        }
    }
}

#[test]
fn missing_optional_characteristics_do_not_break_an_unfiltered_ordinary_spell_play() {
    let mut game = position(&initial(), &card(vec![]), true);
    play_card(&mut game, "spell", 1);
    assert_eq!(
        game.projection(View::Referee).unwrap()["semantic_state"]["counters_this_turn"]["P1"]["cards_played"],
        1
    );
}

#[test]
fn wider_played_selectors_and_nonplayed_trait_unions_stay_rejected() {
    for filter in [
        json!({"traits_any":["宴楽","宴楽"]}),
        json!({"traits_any":["宴楽"]}),
        json!({"traits_any":["宴楽","魔法使い","プリコネ"]}),
        json!({"trait":""}),
        json!({"class":"ウィッチ"}),
        json!({"name":""}),
        json!({"other":true}),
        json!({"trait":"宴楽","token":true,"class":"ウィッチ"}),
        json!({"kind":"follower","orig_cost_eq":1}),
    ] {
        lowerer().lower(&tree(Some(filter))).unwrap_err();
    }
    let mut input = tree(Some(json!({"trait":"宴楽"})));
    input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0]["life"]["delta"]["history_count"]
        ["who"] = json!("opp");
    lowerer().lower(&input).unwrap_err();
    let mut input = tree(Some(json!({"traits_any":["宴楽","魔法使い"]})));
    input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0]["life"]["delta"]["history_count"]
        ["event"] = json!("evolved");
    lowerer().lower(&input).unwrap_err();
}

fn history_read(program: &mut CardProgram) -> (&mut HistoryQuery, &mut HistoryCount) {
    let Node::Instruction { instruction } =
        &mut program.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("life instruction");
    };
    let Action::Life {
        change: NumericChange::Add {
            amount: Expr::Compute(operation),
        },
        ..
    } = &mut instruction.action
    else {
        panic!("natural life amount");
    };
    let IntOperation::FromNat {
        value: Expr::Live(read),
    } = operation.as_mut()
    else {
        panic!("history count");
    };
    let NatRead::HistoryCount { query, count } = read.as_mut() else {
        panic!("history read");
    };
    (query, count)
}

#[test]
fn raw_history_variants_are_rejected_before_runtime() {
    for variant in 0..5 {
        let mut candidate = history_reader(Some(json!({"kind":"spell"}))).card().clone();
        let (query, count) = history_read(&mut candidate);
        match variant {
            0 => *count = HistoryCount::Subjects,
            1 => query.window.relative_player = Some(PlayerRef::Controller {}),
            2 => {
                query.include_execution = InclusionPolicy::ExcludeCurrentExecution {
                    execution: ExecutionRef::Current {},
                }
            }
            3 => query.window.range = HistoryRange::Game {},
            _ => {
                let EventPattern::Play {
                    predicate: Expr::Compute(outer),
                    ..
                } = &mut query.pattern
                else {
                    panic!("played pattern");
                };
                let Predicate::PlayedCard {
                    predicate: Expr::Compute(inner),
                    ..
                } = outer.as_mut()
                else {
                    panic!("card predicate");
                };
                let Predicate::And { predicates } = inner.as_mut() else {
                    panic!("controller and kind");
                };
                predicates.push(predicates.last().unwrap().clone());
            }
        }
        WellFormedCardProgram::check(candidate).unwrap_err();
    }
}

fn child_position(full: bool, opponent: bool) -> Value {
    let mut initial = initial();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"parent","card":"unit-amulet"}]);
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"incoming","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["cemetery"] = json!([]);
    if opponent {
        initial["players"]["P2"]["zones"]["cemetery"] =
            initial["players"]["P1"]["zones"]["cemetery"].take();
        initial["players"]["P1"]["zones"]["cemetery"] = json!([]);
    }
    if full {
        for n in 0..4 {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("filler{n}"),"card":"unit-follower"}));
        }
    }
    initial
}

#[test]
fn nested_play_and_ability_put_share_the_exact_play_fact_boundary() {
    for (nested, full, opponent) in [
        (false, false, false),
        (true, false, false),
        (true, true, false),
        (true, false, true),
    ] {
        let mut input = equipment_played::tree(
            &json!({"fanfare":{"do":[
                {"if":{"cond":{"entered_by":{"ref":"this","by":"play"}},"then":[{"life":{"who":"self","delta":10}}]}},
                {"life":{"who":"self","delta":{"history_count":{"event":"played","window":"this_turn","who":"self","sel":{"trait":"宴楽"}}}}}
            ]}}),
            "unit-follower",
        );
        let action = if nested {
            json!({"play_card":{"ref":"incoming","cost":0}})
        } else {
            json!({"move":{"ref":"incoming","to":"field","result":"entered"}})
        };
        let parent = equipment_played::tree(
            &json!({"activate":{"cost":{"pp":0},"targets":[{"bind":"incoming","from":{"who":if opponent {"opp"}else{"self"},"zone":"cemetery","kind":"follower"},"n":1}],"do":[action,{"life":{"who":"self","delta":{"history_count":{"event":"played","window":"this_turn","who":"self","sel":{"trait":"宴楽"}}}}}]}}),
            "unit-amulet",
        );
        input["cards"]["unit-amulet"] = parent["cards"]["unit-amulet"].clone();
        let programs = lowerer().lower(&input).unwrap();
        let initial = child_position(full, opponent);
        let mut game = Game::new(
            traits_catalog(false, false),
            &initial,
            &Value::Null,
            &Value::Null,
            "nested-play-history",
        )
        .unwrap();
        for (number, program) in &programs {
            game.load_internal_card(number, program).unwrap();
        }
        let CardPlay::Awaiting(q) = game
            .activate_internal_card(
                "parent",
                programs["unit-amulet"].card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap()
        else {
            panic!("child target");
        };
        assert!(matches!(q.domain, InputDomain::Targets(_)));
        resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        if full {
            assert!(game.internal_pending_choice().unwrap().is_none());
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                20
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["semantic_state"]["counters_this_turn"]["P1"]
                    ["cards_played"],
                0
            );
            continue;
        }
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = q.domain else {
            panic!("child fanfare");
        };
        resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            if nested { 32 } else { 20 },
            "nested proper Play contributes exactly one before its Fanfare; ability Put contributes none"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["semantic_state"]["counters_this_turn"]["P1"]["cards_played"],
            if nested { json!(1) } else { json!(0) }
        );
    }
}

#[test]
fn failed_full_field_and_unpaid_plays_leave_no_filtered_fact() {
    for full in [false, true] {
        let reader = history_reader(Some(json!({"traits_any":["宴楽","魔法使い"]})));
        let mut initial = initial();
        if full {
            for n in 0..4 {
                initial["players"]["P1"]["zones"]["field"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":format!("filler{n}"),"card":"unit-follower"}));
            }
        } else {
            initial["players"]["P1"]["pp"] = json!({"current":0,"max":0});
        }
        let mut game = position(&initial, &reader, false);
        let before = game.projection(View::Referee).unwrap();
        game.play_internal_card(if full { "dual" } else { "spell" }, PlayerId { seat: 1 })
            .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
        read_count(&mut game, &reader, 0);
    }
}

#[test]
fn played_facts_and_counters_clear_together_at_real_turn_changes() {
    let reader = history_reader(Some(json!({"trait":"宴楽"})));
    let mut initial = initial();
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] =
            json!([{"id":format!("next-{seat}"),"card":"unit-spell"}]);
    }
    let mut game = position(&initial, &reader, false);
    play_card(&mut game, "dual", 1);
    read_count(&mut game, &reader, 1);
    for _ in 0..2 {
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        quick::pass(&mut game);
    }
    read_count(&mut game, &reader, 0);
}

#[test]
fn an_old_generation_play_counts_as_another_card_after_the_same_physical_card_returns_to_hand() {
    let mut input = equipment_played::tree(
        &json!({"fanfare":{"do":[{"if":{"cond":{"history":{"event":"played","window":"this_turn","who":"self","sel":{"trait":"宴楽"},"at_least":2}},"then":[{"life":{"who":"self","delta":1}}]}}]}}),
        "unit-follower",
    );
    let parent = equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0},"targets":[{"bind":"child","from":{"who":"self","zone":"field","kind":"follower"},"n":1}],"do":[{"move":{"ref":"child","to":"hand"}}]}}),
        "unit-amulet",
    );
    input["cards"]["unit-amulet"] = parent["cards"]["unit-amulet"].clone();
    let programs = lowerer().lower(&input).unwrap();
    let mut initial = initial();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"parent","card":"unit-amulet"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"child","card":"unit-follower"}]);
    let mut game = Game::new(
        traits_catalog(false, false),
        &initial,
        &Value::Null,
        &Value::Null,
        "played-new-generation",
    )
    .unwrap();
    for (number, program) in &programs {
        game.load_internal_card(number, program).unwrap();
    }
    for expected in [20, 21] {
        play_card(&mut game, "child", 1);
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = q.domain else {
            panic!("child Fanfare");
        };
        resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            expected
        );
        if expected == 20 {
            let CardPlay::Awaiting(q) = game
                .activate_internal_card(
                    "parent",
                    programs["unit-amulet"].card().abilities[0].node.id,
                    PlayerId { seat: 1 },
                )
                .unwrap()
            else {
                panic!("return target");
            };
            resolved(
                game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![0]))
                    .unwrap(),
            );
            game.resume_internal_card().unwrap();
        }
    }
}

#[test]
fn raw_duplicate_and_three_way_trait_unions_are_not_canonical_played_queries() {
    for three_way in [false, true] {
        let mut candidate = history_reader(Some(json!({"traits_any":["宴楽","魔法使い"]})))
            .card()
            .clone();
        let (query, _) = history_read(&mut candidate);
        let EventPattern::Play {
            predicate: Expr::Compute(outer),
            ..
        } = &mut query.pattern
        else {
            panic!("played event");
        };
        let Predicate::PlayedCard {
            predicate: Expr::Compute(inner),
            ..
        } = outer.as_mut()
        else {
            panic!("one card binding");
        };
        let Predicate::Or { predicates } = inner.as_mut() else {
            panic!("trait union");
        };
        if three_way {
            predicates.push(predicates[0].clone());
        } else {
            predicates[1] = predicates[0].clone();
        }
        WellFormedCardProgram::check(candidate).unwrap_err();
    }
}

#[test]
fn traits_added_after_play_do_not_rewrite_committed_play_information() {
    let mut input = tree(Some(json!({"trait":"後付"})));
    input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"].as_array_mut().unwrap().insert(0,json!({"apply":{"on":{"who":"self","zone":"field","kind":"follower"},"effect":{"add_trait":{"trait":"後付"}}}}));
    let reader = lowerer()
        .lower(&input)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut game = position(&initial(), &reader, false);
    play_card(&mut game, "dual", 1);
    read_count(&mut game, &reader, 0);
    assert!(
        game.projection(View::Referee).unwrap()["objects"]["dual"]["traits"]
            .as_array()
            .unwrap()
            .contains(&json!("後付")),
        "the current card gained the trait while its old Play fact did not"
    );
}

fn union_subscription(filter: &Value) -> Value {
    equipment_played::tree(
        &json!({"auto":{"on":{"played":{"who":"self","sel":filter}},
            "do":[{"damage":{"to":{"leaders":"opponents"},"n":1}}]}}),
        "unit-follower",
    )
}

#[test]
fn played_union_matches_each_branch_once_including_an_overlapping_spell() {
    let input = union_subscription(&json!({"any_of":[
        {"trait":"宴楽"},{"kind":"spell","trait":"魔法使い"}
    ]}));
    let reader = lowerer()
        .lower(&input)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for (kind, traits, seat, active, expected) in [
        ("spell", json!(["宴楽", "魔法使い"]), 1, 1, 1),
        ("spell", json!(["宴楽", "魔法使い"]), 1, 2, 1),
        ("spell", json!(["宴楽"]), 1, 1, 1),
        ("spell", json!(["魔法使い"]), 1, 1, 1),
        ("spell", json!([]), 1, 1, 0),
        ("follower", json!(["宴楽"]), 1, 1, 1),
        ("follower", json!(["魔法使い"]), 1, 1, 0),
        ("spell", json!(["宴楽", "魔法使い"]), 2, 2, 0),
    ] {
        let (snapshot, keywords, document) = catalog_documents(&json!("1"));
        let snapshot = snapshot
            .lines()
            .map(|line| {
                let mut row: Value = serde_json::from_str(line).unwrap();
                row["faces"][0]["traits"] = traits.clone();
                row.to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
        let catalog = Arc::new(
            Catalog::from_documents(&snapshot, &keywords, &[("unit.yaml".into(), document)])
                .unwrap(),
        );
        let mut initial = initial();
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        initial["turn"]["active"] = json!(format!("P{active}"));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"][format!("P{seat}")]["zones"]["hand"] = json!([{"id":"played","card":if kind == "spell" {"unit-spell"} else {"unit-follower"}}]);
        initial["players"][format!("P{seat}")]["pp"] = json!({"current":10,"max":10});
        let mut game = Game::new(
            catalog,
            &initial,
            &Value::Null,
            &Value::Null,
            "played-union",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &reader).unwrap();
        game.load_internal_card(
            "unit-spell",
            &card(vec![play::spell(vec![]), quick::permit()]),
        )
        .unwrap();
        if active == seat {
            play_card(&mut game, "played", seat);
        } else {
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            let q = game.internal_quick_choice().unwrap().unwrap();
            assert_eq!(q.actor, PlayerId { seat });
            let InputDomain::Quick { options, .. } = q.domain else {
                panic!("Quick window")
            };
            assert_eq!(options.len(), 1_usize);
            resolved(
                game.answer_internal_card(q.actor, q.token, Answer::Quick(Some(options[0].handle)))
                    .unwrap(),
            );
            if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                game.resume_internal_card().unwrap();
            }
        }
        let mut count = 0_u8;
        while let Some(q) = game.internal_pending_choice().unwrap() {
            let InputDomain::Pending(options) = q.domain else {
                panic!("pending union")
            };
            resolved(
                game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                    .unwrap(),
            );
            game.resume_internal_card().unwrap();
            count = count.checked_add(1).unwrap();
            assert!(count <= 1, "overlapping branches create one occurrence");
        }
        assert_eq!(
            count, expected,
            "kind={kind}, traits={traits}, seat={seat}, active={active}"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            20_u8.checked_sub(expected).unwrap()
        );
    }
}

#[test]
fn played_union_rejects_unneeded_branch_filters_and_malformed_unions() {
    for filter in [
        json!({"any_of":[{"trait":"宴楽"},{"trait":"魔法使い","kind":"follower"}]}),
        json!({"any_of":[{"trait":"宴楽"},{"trait":"魔法使い","kind":"spell","orig_cost_le":3}]}),
        json!({"any_of":[{"trait":"宴楽"},{"trait":"宴楽","kind":"spell"}]}),
        json!({"any_of":[{"trait":"宴楽"}]}),
        json!({"any_of":[{"trait":"宴楽"},{"trait":"魔法使い","kind":"spell"},{"trait":"プリコネ"}]}),
    ] {
        lowerer().lower(&union_subscription(&filter)).unwrap_err();
    }
}

#[test]
fn played_union_rejects_wider_player_or_turn_qualifiers() {
    let baseline = union_subscription(&json!({"any_of":[
        {"trait":"宴楽"},{"kind":"spell","trait":"魔法使い"}
    ]}));
    for who in [None, Some("opp")] {
        let mut input = baseline.clone();
        let event = input["cards"]["unit-follower"]["abilities"][0]["auto"]["on"]["played"]
            .as_object_mut()
            .unwrap();
        if let Some(who) = who {
            event.insert("who".into(), json!(who));
        } else {
            event.remove("who");
        }
        lowerer().lower(&input).unwrap_err();
    }
    let mut input = baseline;
    input["cards"]["unit-follower"]["abilities"][0]["auto"]["on"]["played"]["during"] =
        json!({"turn":"self"});
    lowerer().lower(&input).unwrap_err();
}

#[test]
fn played_union_raw_ir_rejects_reversed_or_extra_branches() {
    let input = union_subscription(&json!({"any_of":[
        {"trait":"宴楽"},{"kind":"spell","trait":"魔法使い"}
    ]}));
    let program = lowerer()
        .lower(&input)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for extra in [false, true] {
        let mut candidate = program.card().clone();
        let TriggerCondition::Event {
            pattern:
                EventPattern::Play {
                    predicate: Expr::Compute(outer),
                    ..
                },
        } = &mut candidate.abilities[0].node.trigger.as_mut().unwrap().on
        else {
            panic!("played event")
        };
        let Predicate::PlayedCard {
            predicate: Expr::Compute(inner),
            ..
        } = outer.as_mut()
        else {
            panic!("one card binding")
        };
        let Predicate::Or { predicates } = inner.as_mut() else {
            panic!("union")
        };
        if extra {
            predicates.push(predicates[0].clone());
        } else {
            predicates.swap(0, 1);
        }
        WellFormedCardProgram::check(candidate).unwrap_err();
    }
}

#[test]
fn played_kind_trait_and_token_filters_lower_and_load() {
    for filter in [
        json!({"kind":"follower"}),
        json!({"kind":"amulet"}),
        json!({"trait":"宴楽","kind":"spell"}),
        json!({"kind":"follower","trait":"宴楽","token":true}),
        json!({"kind":"spell","not_token":true}),
    ] {
        lower_and_load(&tree(Some(filter)));
    }
}

#[test]
fn played_intersections_use_saved_facts_after_token_erasure_without_hidden_hand_identity() {
    for (filter, expected) in [
        (json!({"kind":"follower","trait":"宴楽"}), 2),
        (json!({"trait":"宴楽","token":true}), 1),
        (json!({"kind":"follower","trait":"宴楽","token":true}), 1),
        (
            json!({"kind":"follower","trait":"魔法使い","token":true}),
            0,
        ),
        (
            json!({"kind":"spell","trait":"魔法使い","not_token":true}),
            1,
        ),
    ] {
        let mut candidate = history_reader(Some(filter)).card().clone();
        let others = Selector::Difference {
            input: Box::new(Selector::Zone {
                players: players(PlayerRelation::SelfPlayer),
                zone: ZoneKind::Field,
                view: SelectionView::Current {},
                binding: Binding::new(901, ScopeId(901)),
                predicate: Box::new(Expr::Literal(Bool(true))),
            }),
            except: Box::new(Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            }),
        };
        candidate.abilities.push(ability(
            90,
            vec![movement::put(90, others, OwnerZone::Cemetery)],
        ));
        let reader = WellFormedCardProgram::check(candidate).unwrap();
        let mut observations = Vec::new();
        for hidden in ["unit-spell", "unit-follower"] {
            let mut initial = initial();
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .retain(|object| object["id"] != "amulet");
            initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"amulet","card":"unit-amulet"}]);
            initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"hidden","card":hidden}]);
            let mut game = position_with_tokens(&initial, &reader, false, true);
            for id in ["spell", "amulet", "dual"] {
                play_card(&mut game, id, 1);
            }
            read_count(&mut game, &reader, expected);
            resolved(
                game.activate_internal_card("reader", AbilityId(90), PlayerId { seat: 1 })
                    .unwrap(),
            );
            game.resume_internal_card().unwrap();
            assert!(
                !game.projection(View::Referee).unwrap()["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("amulet")),
                "a departing token is erased"
            );
            read_count(&mut game, &reader, expected);
            observations.push(game.observe(Viewer::P1).unwrap());
        }
        assert_eq!(
            observations[0], observations[1],
            "a private opposing hand changes no public history read"
        );
    }
}
