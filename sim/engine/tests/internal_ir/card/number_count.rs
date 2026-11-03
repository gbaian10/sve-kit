use super::*;

fn tree(payload: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut candidate = tree["cards"]["BP08-038"].take();
    let mut entry = candidate["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["line"] = json!(1);
    entry["activate"] = payload.clone();
    candidate["abilities"] = json!([entry]);
    tree["cards"] = json!({"BP08-038":candidate});
    tree
}

fn lowered(payload: &Value) -> WellFormedCardProgram {
    remapped(&tree(payload))
}

fn remapped(tree: &Value) -> WellFormedCardProgram {
    let programs = lowerer().lower(tree).unwrap();
    let mut a = programs["BP08-038"].card().abilities[0].clone();
    a.source = source("unit-follower");
    a.source.fragment = None;
    a.node.id = AbilityId(1);
    a.node.source = AbilitySource {
        holder: ObjectOperand::SourceHolder {},
        controller: PlayerRef::Controller {},
        origin: AbilityOrigin::Printed {
            face: FaceId("0".into()),
            line: Nat(1),
        },
    };
    if let Some(body) = &mut a.node.body {
        locate_steps(&mut body.steps, &a.source, &a.node.source);
    }
    if let Some(play) = &mut a.node.play
        && let CostSpec::Ordered { steps } = &mut play.costs
    {
        for step in steps {
            step.instruction.source = a.node.source.clone();
        }
    }
    for effect in &mut a.node.declarations {
        if let Effect::Continuous { source, .. } = effect {
            *source = a.node.source.clone();
        }
    }
    card(vec![a])
}

#[test]
fn own_numeric_dsl_reads_original_and_effective_values_and_seals_departing_source_information() {
    let program = lowered(&json!({"cost":{"pp":0},"do":[
        {"damage":{"to":{"leaders":"opponents"},"n":{"orig_cost_of":"this"}}},
        {"buff":{"on":"this","power":{"cost_of":"this"},"hp":{"hp_of":"this"}}},
        {"damage":{"to":{"leaders":"opponents"},"n":{"power_of":"this"}}},
        {"move":{"ref":"this","to":"cemetery"}},
        {"damage":{"to":{"leaders":"opponents"},"n":{"hp_of":"this"}}}
    ]}));
    let mut game = position_with_state(1, &program, &json!({"cost":2}));
    let result = activate(&mut game, 1);
    let amounts = result
        .progress
        .facts
        .iter()
        .filter_map(|fact| {
            if let Fact::Damage(hit) = fact {
                Some(hit.amount.0.get())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(amounts, [1_u64, 4, 6]);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["s"])
    );
}

#[test]
fn class_and_negative_class_count_queries_intersect_kind_and_controller() {
    let query = |class: &str, negative: bool| {
        let mut count = json!({"who":"self","zone":"field","kind":"follower"});
        count[if negative { "not_class" } else { "class" }] = json!(class);
        json!({"count":count})
    };
    let program = lowered(&json!({"cost":{"pp":0},"do":[
        {"if":{"cond":{"cmp":{"left":query("ニュートラル",false),"op":"eq","right":2}},"then":[{"life":{"who":"self","delta":1}}]}},
        {"if":{"cond":{"cmp":{"left":query("ニュートラル",true),"op":"eq","right":0}},"then":[{"life":{"who":"self","delta":2}}]}},
        {"if":{"cond":{"cmp":{"left":query("エルフ",true),"op":"eq","right":2}},"then":[{"life":{"who":"self","delta":4}}]}}
    ]}));
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"s","card":"unit-follower"},{"id":"ally","card":"unit-follower"},{"id":"amulet","card":"unit-amulet"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"opposing","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "count-class",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    activate(&mut game, 1);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        27
    );
}

#[test]
fn scalar_zone_counts_reject_hidden_characteristic_filters_and_unknown_keywords() {
    for query in [
        json!({"who":"self","zone":"banish","class":"エルフ"}),
        json!({"who":"self","zone":"deck","keyword":"ward"}),
        json!({"who":"self","zone":"hand","other":true}),
        json!({"who":"self","zone":"field","keyword":"unknown"}),
    ] {
        lowerer()
            .lower(&tree(
                &json!({"cost":{"pp":0},"do":[{"draw":{"n":{"count":query}}}]}),
            ))
            .unwrap_err();
    }
}

#[test]
fn banish_count_reads_only_own_population_and_refreshes_after_source_leaves() {
    let count = json!({"count":{"who":"self","zone":"banish"}});
    let program = lowered(&json!({"cost":{"pp":0},"do":[
        {"damage":{"to":{"leaders":"opponents"},"n":count}},
        {"banish":"this"},
        {"damage":{"to":{"leaders":"opponents"},"n":count}}
    ]}));
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["banish"] = json!([
        {"id":"own1","card":"unit-spell"},{"id":"own2","card":"unit-spell"}
    ]);
    initial["players"]["P2"]["zones"]["banish"] = json!([
        {"id":"opp1","card":"unit-spell"},{"id":"opp2","card":"unit-spell"},
        {"id":"opp3","card":"unit-spell"}
    ]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "count-banish",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    let result = activate(&mut game, 1);
    let amounts = result
        .progress
        .facts
        .iter()
        .filter_map(|fact| {
            if let Fact::Damage(hit) = fact {
                Some(hit.amount.0.get())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(amounts, [2_u64, 3]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["banish"], json!(["own1", "own2", "s"]));
    assert_eq!(state["P2"]["leader"]["life"], 15_u8);
}

#[test]
fn static_keyword_count_dependencies_are_rejected_before_they_can_recurse() {
    let mut candidate = tree(&json!({"cost":{"pp":0},"do":[]}));
    let entry = &mut candidate["cards"]["BP08-038"]["abilities"][0];
    entry.as_object_mut().unwrap().remove("activate");
    entry["static"] = json!({
        "while":{"cmp":{"left":{"count":{"who":"self","zone":"field","keyword":"ward"}},"op":"ge","right":1}},
        "effects":[{"grant":{"keywords":["ward"]}}]
    });
    let program = remapped(&candidate);
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "count-recursion",
    )
    .unwrap();
    let error = game
        .load_internal_card("unit-follower", &program)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("recursive static keyword dependency")
    );
}

#[test]
fn excluding_source_from_a_count_never_authorizes_hidden_keyword_reads() {
    let program = lowered(&json!({"cost":{"pp":0},"do":[
        {"damage":{"to":{"leaders":"opponents"},"n":{"count":{"who":"self","zone":"field","keyword":"ward","other":true}}}}
    ]}));
    for zone in [ZoneKind::Hand, ZoneKind::Deck, ZoneKind::Banish] {
        let mut a = program.card().abilities[0].clone();
        let Node::Instruction { instruction } = &mut a.node.body.as_mut().unwrap().steps[0].node
        else {
            panic!("lowered damage instruction")
        };
        let Action::Damage {
            amount:
                DamageAmount::Uniform {
                    amount: Expr::Compute(amount),
                },
            ..
        } = &mut instruction.action
        else {
            panic!("lowered counted damage")
        };
        let IntOperation::FromNat {
            value: Expr::Live(count),
        } = amount.as_mut()
        else {
            panic!("natural count")
        };
        let NatRead::ZoneCount {
            selector: Selector::Difference { input, .. },
        } = count.as_mut()
        else {
            panic!("other count")
        };
        let Selector::Zone {
            zone: input_zone, ..
        } = input.as_mut()
        else {
            panic!("zone input")
        };
        *input_zone = zone;
        let mut game = Game::new(
            catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "count-private",
        )
        .unwrap();
        let before = exact_authority(&game);
        let error = game
            .load_internal_card("unit-follower", &card(vec![a]))
            .unwrap_err();
        assert!(error.to_string().contains("private observation"));
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn scalar_hand_and_deck_counts_use_each_players_current_population() {
    let count = |who: &str, zone: &str| json!({"count":{"who":who,"zone":zone}});
    let program = lowered(&json!({"cost":{"pp":0},"do":[
        {"damage":{"to":{"leaders":"opponents"},"n":count("self","hand")}},
        {"damage":{"to":{"leaders":"opponents"},"n":count("self","deck")}},
        {"damage":{"to":{"leaders":"opponents"},"n":count("opp","hand")}},
        {"damage":{"to":{"leaders":"opponents"},"n":count("opp","deck")}},
        {"draw":{"n":1_u8}},
        {"damage":{"to":{"leaders":"opponents"},"n":count("self","hand")}},
        {"damage":{"to":{"leaders":"opponents"},"n":count("self","deck")}}
    ]}));
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    for (player, zone, n) in [
        ("P1", "hand", 2_usize),
        ("P1", "deck", 3),
        ("P2", "hand", 4),
        ("P2", "deck", 5),
    ] {
        initial["players"][player]["zones"][zone] = json!(
            (0..n)
                .map(|i| json!({"id":format!("{player}-{zone}-{i}"),"card":"unit-spell"}))
                .collect::<Vec<_>>()
        );
    }
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "scalar-hand-deck",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    let result = activate(&mut game, 1);
    let amounts = result
        .progress
        .facts
        .iter()
        .filter_map(|fact| {
            if let Fact::Damage(hit) = fact {
                Some(hit.amount.0.get())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(amounts, [2_u64, 3, 4, 5, 3, 2]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 1_u8);
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 3_usize);
}

fn arithmetic_game(program: &WellFormedCardProgram, banished: usize) -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["banish"] = json!(
        (0..banished)
            .map(|n| json!({"id":format!("b{n}"),"card":"unit-spell"}))
            .collect::<Vec<_>>()
    );
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "arith").unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

fn life_after(payload: &Value, banished: usize) -> i64 {
    let mut game = arithmetic_game(&lowered(payload), banished);
    activate(&mut game, 1);
    game.projection(View::Referee).unwrap()["P1"]["leader"]["life"]
        .as_i64()
        .unwrap()
}

#[test]
fn plus_times_and_min_combine_public_counts_with_literals() {
    let count = json!({"count":{"who":"self","zone":"banish"}});
    let gain = |delta: Value| json!({"cost":{"pp":0},"do":[{"life":{"who":"self","delta":delta}}]});
    for (banished, plus, times, min) in [(0_usize, 3, 0, 0), (3, 6, 6, 2), (7, 10, 14, 2)] {
        assert_eq!(
            life_after(&gain(json!({"plus":[3,count]})), banished),
            20 + plus
        );
        assert_eq!(
            life_after(&gain(json!({"times":[count,2]})), banished),
            20 + times,
            "「の2倍」 doubles the current count"
        );
        assert_eq!(
            life_after(&gain(json!({"min":[count,2,5]})), banished),
            20 + min,
            "min takes the smallest operand"
        );
    }
}

#[test]
fn per_rounds_down_on_both_sides_of_each_unit() {
    let count = json!({"count":{"who":"self","zone":"banish"}});
    let per = json!({"per":{"of":count,"unit":5}});
    let payload = json!({"cost":{"pp":0},"do":[{"life":{"who":"self","delta":{"plus":[1,per]}}}]});
    for (banished, gained) in [(0_usize, 1_i64), (4, 1), (5, 2), (9, 2), (10, 3)] {
        assert_eq!(
            life_after(&payload, banished),
            20 + gained,
            "{banished} cards hold {} whole fives",
            gained - 1
        );
    }
    // A negative dividend floors toward negative infinity instead of truncating toward zero.
    let negative = json!({"per":{"of":{"minus":[0,count]},"unit":5}});
    let check = json!({"cost":{"pp":0},"do":[{"if":{
        "cond":{"cmp":{"left":negative,"op":"eq","right":-2}},
        "then":[{"life":{"who":"self","delta":1}}]
    }}]});
    assert_eq!(life_after(&check, 7), 21, "floor(-7 / 5) is -2");
    assert_eq!(life_after(&check, 5), 20, "floor(-5 / 5) is -1");
}

#[test]
fn arithmetic_lowering_rejects_a_zero_unit_an_empty_min_and_unopened_max() {
    for n in [
        json!({"per":{"of":3,"unit":0}}),
        json!({"min":[]}),
        json!({"max":[1,2]}),
        json!({"half_up":[1,2]}),
        json!({"times":[1,2,3]}),
    ] {
        lowerer()
            .lower(&tree(&json!({"cost":{"pp":0},"do":[{"draw":{"n":n}}]})))
            .unwrap_err();
    }
}

#[test]
fn half_up_rounds_an_odd_constant_in_a_body_expression() {
    let payload = json!({"cost":{"pp":0},"do":[{"life":{"who":"self","delta":{"half_up":3}}}]});
    assert_eq!(
        life_after(&payload, 0),
        22,
        "half of three rounds upward to two"
    );
}

#[test]
fn half_up_rounds_signed_halves_towards_positive_infinity() {
    const MAX_SAFE: i64 = 0x001F_FFFF_FFFF_FFFF;
    for (amount, rounded) in [
        (0_i64, 0_i64),
        (1, 1),
        (2, 1),
        (3, 2),
        (-1, 0),
        (-2, -1),
        (-3, -1),
        (-4, -2),
    ] {
        let payload =
            json!({"cost":{"pp":0},"do":[{"life":{"who":"self","delta":{"half_up":amount}}}]});
        assert_eq!(life_after(&payload, 0), 20 + rounded);
    }
    let composed = json!({"cost":{"pp":0},"do":[{"life":{"who":"self","delta":{"half_up":{"plus":[{"power_of":"this"},1]}}}}]});
    assert_eq!(life_after(&composed, 0), 22);

    for (amount, rounded) in [
        (
            json!({"plus":[{"times":[MAX_SAFE,1024]},1023]}),
            json!({"times":[1024,{"times":[1_i64 << 31,1_i64 << 21]}]}),
        ),
        (
            json!({"minus":[{"times":[-MAX_SAFE,1024]},1024]}),
            json!({"times":[-1024,{"times":[1_i64 << 31,1_i64 << 21]}]}),
        ),
    ] {
        let payload = json!({"cost":{"pp":0},"do":[{"if":{
            "cond":{"cmp":{"left":{"half_up":amount},"op":"eq","right":rounded}},
            "then":[{"life":{"who":"self","delta":1}}]
        }}]});
        assert_eq!(life_after(&payload, 0), 21);
    }
}

#[test]
fn signed_overflow_fails_the_activation_without_changing_the_game() {
    // The author grammar caps literals at 2^53 - 1; their product still exceeds i64.
    const MAX_SAFE: i64 = 0x001F_FFFF_FFFF_FFFF;
    let program = lowered(&json!({"cost":{"pp":0},"do":[
        {"life":{"who":"self","delta":{"times":[{"times":[MAX_SAFE,MAX_SAFE]},2]}}}
    ]}));
    let mut game = arithmetic_game(&program, 0);
    let before = exact_authority(&game);
    game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(exact_authority(&game), before);
}

#[test]
fn twice_the_field_count_includes_source_and_excludes_amulets() {
    let program = lowered(&json!({"cost":{"pp":0},"do":[
        {"damage":{"to":{"leaders":"opponents"},"n":{"times":[
            {"count":{"who":"self","zone":"field","kind":"follower"}},2
        ]}}}
    ]}));
    for (allies, expected) in [(0_usize, 2_u64), (2, 6)] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        let mut field = vec![
            json!({"id":"s","card":"unit-follower"}),
            json!({"id":"amulet","card":"unit-amulet"}),
        ];
        field.extend((0..allies).map(|n| json!({"id":format!("ally{n}"),"card":"unit-follower"})));
        initial["players"]["P1"]["zones"]["field"] = json!(field);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "twice-field",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program).unwrap();
        game.load_internal_card("unit-amulet", &card(vec![]))
            .unwrap();
        activate(&mut game, 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            20 - expected
        );
    }
}

#[test]
fn per_damage_is_a_separate_hit_and_omits_a_zero_hit() {
    let program = lowered(&json!({"cost":{"pp":0},"do":[
        {"damage":{"to":{"leaders":"opponents"},"n":3}},
        {"damage":{"to":{"leaders":"opponents"},"n":{"per":{
            "of":{"count":{"who":"self","zone":"cemetery","kind":"follower"}},"unit":5
        }}}}
    ]}));
    for (followers, expected) in [
        (4_usize, vec![3_u64]),
        (5, vec![3, 1]),
        (9, vec![3, 1]),
        (10, vec![3, 2]),
    ] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        let mut grave = vec![json!({"id":"other","card":"unit-spell"})];
        grave.extend((0..followers).map(|n| json!({"id":format!("f{n}"),"card":"unit-follower"})));
        initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
        let mut game =
            Game::new(catalog(), &initial, &Value::Null, &Value::Null, "per-hits").unwrap();
        game.load_internal_card("unit-follower", &program).unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        let result = activate(&mut game, 1);
        let hits: Vec<_> = result
            .progress
            .facts
            .iter()
            .filter_map(|fact| {
                if let Fact::Damage(hit) = fact {
                    Some(hit.amount.0.get())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(hits, expected);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            20 - expected.iter().sum::<u64>()
        );
    }
}

#[test]
fn minimum_pair_discount_requires_the_reduced_cost_in_full() {
    let mut input = tree(&json!({"cost":{"pp":0},"do":[]}));
    let mut seed = input["cards"]["BP08-038"].take();
    let entry = &mut seed["abilities"][0];
    entry.as_object_mut().unwrap().remove("activate");
    let count = |kind| json!({"count":{"who":"self","zone":"cemetery","kind":kind}});
    entry["play"] = json!({"cost_delta":{"minus":[0,{"min":[count("follower"),count("spell")]}]}});
    input["cards"] = json!({"unit-follower":seed});
    let programs = lowerer().lower(&input).unwrap();
    for (followers, spells, pp, remaining) in [
        (0_usize, 3_usize, 10_u8, Some(0_u8)),
        (2, 3, 8, Some(0)),
        (3, 3, 7, Some(0)),
        (1, 3, 8, None),
    ] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["pp"] = json!({"current":pp,"max":10});
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
        let mut grave: Vec<_> = (0..followers)
            .map(|n| json!({"id":format!("f{n}"),"card":"unit-follower"}))
            .collect();
        grave.extend((0..spells).map(|n| json!({"id":format!("spell{n}"),"card":"unit-spell"})));
        initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
        let mut game = Game::new(
            catalog_with_cost(&json!("10")),
            &initial,
            &Value::Null,
            &Value::Null,
            "pair-cost",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &programs["unit-follower"])
            .unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        let before = exact_authority(&game);
        let result = game.play_internal_card("s", PlayerId { seat: 1 });
        if let Some(pp_left) = remaining {
            assert!(matches!(result.unwrap(), CardPlay::Entered(_)));
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["pp"]["current"], pp_left);
            assert_eq!(state["P1"]["field"], json!(["s"]));
        } else {
            result.unwrap_err();
            assert_eq!(exact_authority(&game), before);
        }
    }
}
