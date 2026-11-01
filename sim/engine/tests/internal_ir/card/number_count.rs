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
