#![expect(
    clippy::panic,
    reason = "Unexpected shapes or questions invalidate the ordinal authority fixtures."
)]
use super::*;

fn query() -> Value {
    json!({"event":"evolved","window":"this_turn","who":"self","sel":{"kind":"follower"}})
}
fn read() -> Value {
    json!({"at":"event","value":{"history_count":query()}})
}
fn tree(value: &Value) -> Value {
    equipment_played::tree(
        &json!({"auto":{
            "on":{"evolved":{"who":"self","sel":{"kind":"follower"}}},
            "do":[{"life":{"who":"self","delta":value}}]
        }}),
        "unit-follower",
    )
}
fn lower(input: &Value) -> WellFormedCardProgram {
    lowerer()
        .lower(input)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}
fn load(program: &WellFormedCardProgram) -> Result<(), sve_engine::EngineFailure> {
    Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "event-ordinal-shape",
    )?
    .load_internal_card("unit-follower", program)
}

#[test]
fn event_time_own_follower_evolution_count_lowers_and_loads_in_nested_conditions() {
    load(&lower(&tree(&read()))).unwrap();
    let mut nested = tree(&read());
    nested["cards"]["unit-follower"]["abilities"][0]["auto"]["do"] = json!([
        {"if":{"cond":{"cmp":{"left":read(),"op":"eq","right":1}},"then":[
            {"if":{"cond":{"cmp":{"left":read(),"op":"eq","right":1}},"then":[
                {"life":{"who":"self","delta":read()}}
            ]}}
        ]}}
    ]);
    load(&lower(&nested)).unwrap();
}

#[test]
fn event_time_read_rejects_other_queries_and_live_domains() {
    for value in [
        json!({"history_count":{"event":"attacked","window":"this_turn","who":"self","sel":{"kind":"follower"}}}),
        json!({"history_count":{"event":"evolved","window":"this_turn","who":"opp","sel":{"kind":"follower"}}}),
        json!({"history_count":{"event":"evolved","window":"this_turn","sel":{"kind":"follower"}}}),
        json!({"history_count":{"event":"evolved","window":"this_turn","who":"self","sel":{"kind":"follower","other":true}}}),
        json!({"history_count":{"event":"evolved","window":"this_turn","who":"self","sel":{"kind":"follower"},"distinct":"subjects"}}),
        json!({"history_count":{"event":"evolved","window":"last_turn","who":"self","sel":{"kind":"follower"}}}),
        json!({"leader_life":"self"}),
        json!({"count":{"who":"self","zone":"cemetery"}}),
        json!({"plus":[{"history_count":query()},1]}),
    ] {
        lowerer()
            .lower(&tree(&json!({"at":"event","value":value})))
            .unwrap_err();
    }
}

#[test]
fn event_time_read_rejects_other_headers_and_non_auto_entries() {
    for header in [
        json!({"attacked":{"who":"self","sel":{"kind":"follower"}}}),
        json!({"evolved":{"who":"opp","sel":{"kind":"follower"}}}),
        json!({"evolved":{"who":"self","sel":{"kind":"follower","other":true}}}),
        json!({"evolved":{"who":"self","during":"your_turn","sel":{"kind":"follower"}}}),
        json!({"evolved":{"ref":"this"}}),
    ] {
        let mut input = tree(&read());
        input["cards"]["unit-follower"]["abilities"][0]["auto"]["on"] = header;
        lowerer().lower(&input).unwrap_err();
    }
    for entry in [
        json!({"activate":{"cost":{"pp":0},"do":[{"life":{"who":"self","delta":read()}}]}}),
        json!({"fanfare":{"do":[{"life":{"who":"self","delta":read()}}]}}),
        json!({"spell":{"do":[{"life":{"who":"self","delta":read()}}]}}),
    ] {
        let input = equipment_played::tree(&entry, "unit-follower");
        lowerer().lower(&input).unwrap_err();
    }
}

#[test]
fn structurally_valid_literal_event_capture_is_still_rejected_by_native_admission() {
    let program = lower(&tree(&read()));
    let mut raw = program.card().clone();
    let Capture::Integer { value, .. } = &mut raw.abilities[0].node.event_captures[0] else {
        panic!("numeric capture fixture")
    };
    *value = Expr::Literal(Int(1));
    let checked = WellFormedCardProgram::check(raw).unwrap();
    load(&checked).unwrap_err();
    load(&program).unwrap();
}

#[test]
fn wellformed_event_history_rejects_policy_query_and_same_batch_dependencies() {
    let program = lower(&tree(&read()));
    for change in 0..4 {
        let mut raw = program.card().clone();
        let Capture::Integer { at, value, .. } = &mut raw.abilities[0].node.event_captures[0]
        else {
            panic!("numeric capture fixture")
        };
        if change == 0 {
            *at = CaptureTime::AtEvent {
                event: EventContext::Trigger {},
                policy: EventSnapshotPolicy::BeforeExit,
            };
        } else {
            let Expr::Compute(operation) = value else {
                panic!("FromNat")
            };
            let IntOperation::FromNat {
                value: Expr::Live(read),
            } = operation.as_mut()
            else {
                panic!("history read")
            };
            let NatRead::HistoryCount { query, count } = read.as_mut() else {
                panic!("history query")
            };
            match change {
                1 => *count = HistoryCount::Subjects,
                2 => query.window.range = HistoryRange::Game {},
                _ => {
                    query.include_execution = InclusionPolicy::ExcludeCurrentExecution {
                        execution: ExecutionRef::Current {},
                    }
                }
            }
        }
        WellFormedCardProgram::check(raw).unwrap_err();
    }
    let mut raw = program.card().clone();
    let first = raw.abilities[0].node.event_captures[0].clone();
    let Capture::Integer { at, into, .. } = first else {
        panic!("integer capture")
    };
    raw.abilities[0].node.event_captures.push(Capture::Integer {
        at,
        value: Expr::Captured(into),
        into: CaptureRef {
            binding: Binding::new(999, ScopeId(1)),
        },
    });
    WellFormedCardProgram::check(raw).unwrap_err();
}

#[derive(Clone, Copy)]
enum SourceReturn {
    Stay,
    Hand,
    Field,
}
fn fixture(observer_in_hand: bool, opponent_turn: bool) -> Game {
    fixture_with_departure(observer_in_hand, opponent_turn, SourceReturn::Stay)
}
fn fixture_with_departure(
    observer_in_hand: bool,
    opponent_turn: bool,
    source_return: SourceReturn,
) -> Game {
    let depart = !matches!(source_return, SourceReturn::Stay);
    let mut input = tree(&read());
    let mut observer = input["cards"]["unit-follower"].clone();
    if depart {
        observer["abilities"][0]["auto"]["do"] = json!([
            {"life":{"who":"self","delta":read()}},
            {"if":{"cond":{"cmp":{"left":read(),"op":"ge","right":2}},"then":[
                {"buff":{"on":"this","power":2,"hp":2}},
                {"grant":{"on":"this","keywords":["storm"]}}
            ]}}
        ]);
    }
    let total = if observer_in_hand || opponent_turn {
        1
    } else if depart {
        3
    } else {
        4
    };
    let producer = producer_tree(total, source_return, opponent_turn);
    input["cards"] = producer["cards"].clone();
    input["cards"]["unit-amulet"] = observer;
    let catalog = fixture_catalog(depart);
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["leader"]["life"] = json!(1);
    initial["players"]["P1"]["pp"] = json!({"current":10,"max":10});
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!(
        (0..4)
            .map(|i| json!({"id":format!("recipient{i}"),"card":"unit-follower"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!(
        (0..4)
            .map(|i| json!({"id":format!("form{i}"),"card":"unit-form"}))
            .collect::<Vec<_>>()
    );
    let zone = if observer_in_hand { "hand" } else { "field" };
    initial["players"]["P1"]["zones"][zone]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"observer","card":"unit-amulet"}));
    if opponent_turn {
        initial["turn"]["active"] = json!("P2");
        initial["turn"]["elapsed_turns"]["P2"] = json!(7);
        initial["players"]["P2"]["pp"] = json!({"current":10,"max":10});
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"opponent","card":"unit-follower"}]);
        initial["players"]["P2"]["zones"]["evolve_deck"] =
            json!([{"id":"opponent-form","card":"unit-form"}]);
    }
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "event-ordinal-runtime",
    )
    .unwrap();
    for (number, program) in lowerer().lower(&input).unwrap() {
        game.load_internal_card(&number, &program).unwrap();
    }
    game.load_internal_card("unit-form", &card(vec![])).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}
fn fixture_catalog(depart: bool) -> Arc<Catalog> {
    let (mut snapshot, registry, document) = catalog_documents(&json!("1"));
    if depart {
        snapshot = snapshot
            .lines()
            .map(|line| {
                let mut row: Value = serde_json::from_str(line).unwrap();
                if row["number"] == "unit-amulet" {
                    row["faces"][0]["card_type"] = json!("フォロワー");
                    row["faces"][0]["power"] = json!("2");
                    row["faces"][0]["hp"] = json!("3");
                }
                row.to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    snapshot.push('\n');
    snapshot.push_str(&json!({"number":"unit-form","faces":[{"name":"unit-follower","card_class":"ニュートラル","card_type":"フォロワー・エボルヴ","cost":"-","power":"4","hp":"5","traits":[],"text":null,"sections":[]}]}).to_string());
    let mut definitions: Value = serde_json::from_str(&document).unwrap();
    definitions["cards"]["unit-form"] = json!({"status":"complete","abilities":[]});
    Arc::new(
        Catalog::from_documents(
            &snapshot,
            &registry,
            &[("unit.yaml".into(), definitions.to_string())],
        )
        .unwrap(),
    )
}
fn producer_tree(total: usize, source_return: SourceReturn, opponent_turn: bool) -> Value {
    let depart = !matches!(source_return, SourceReturn::Stay);
    let reenter = matches!(source_return, SourceReturn::Field);
    let targets = (0..total).map(|index|json!({"bind":format!("recipient{index}"),"from":{"who":"self","zone":"field","kind":"follower","has_evolve":true},"n":1})).collect::<Vec<_>>();
    let steps = (0..total)
        .map(|index| json!({"evolve":{"ref":format!("recipient{index}")}}))
        .collect::<Vec<_>>();
    let mut producer = equipment_played::tree(
        &json!({"activate":{
            "cost":{"pp":0},"targets":targets,"do":steps
        }}),
        "unit-follower",
    );
    if depart {
        let body = &mut producer["cards"]["unit-follower"]["abilities"][0]["activate"];
        body["targets"].as_array_mut().unwrap().push(json!({"bind":"observer","from":{"who":"self","zone":"field","kind":"follower","name":"unit-amulet"},"n":1}));
        let departure = if reenter {
            json!([
                {"move":{"ref":"observer","to":"cemetery"}},
                {"choose":{"bind":"returned","from":{"who":"self","zone":"cemetery","kind":"follower","name":"unit-amulet"},"n":1}},
                {"move":{"ref":"returned","to":"field","result":"reentered"}}
            ])
        } else {
            json!([{ "move":{"ref":"observer","to":"hand"}}])
        };
        body["do"].as_array_mut().unwrap().push(json!({"if":{"cond":{"cmp":{"left":{"history_count":query()},"op":"ge","right":3}},"then":departure}}));
    }
    let mut activation = producer["cards"]["unit-follower"]["abilities"][0].clone();
    let mut intrinsic = activation.clone();
    intrinsic.as_object_mut().unwrap().remove("activate");
    intrinsic["line"] = json!(1);
    intrinsic["evolve"] = json!({"cost":{"pp":1}});
    activation["line"] = json!(2);
    if opponent_turn {
        let mut head = activation
            .as_object_mut()
            .unwrap()
            .remove("activate")
            .unwrap();
        head.as_object_mut().unwrap().remove("cost");
        head["on"] = json!({"phase":{"which":"end","whose":"opp"}});
        activation["auto"] = head;
    }
    producer["cards"]["unit-follower"]["abilities"] = json!([intrinsic, activation]);
    producer
}
fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn effect_evolve(game: &mut Game, recipient: usize) {
    effect_batch(game, 1, false);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"][format!("recipient{recipient}")]["evolved"],
        true,
        "the authority actually evolves the requested recipient"
    );
}
fn effect_batch(game: &mut Game, total: usize, phase: bool) {
    use sve_engine::game::internal::InputDomain;
    checks(game);
    let mut play = if phase {
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        checks(game);
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("phase producer")
        };
        let handle = options
            .iter()
            .find(|option| option.card == "unit-follower")
            .unwrap()
            .handle;
        game.answer_internal_card(question.actor, question.token, Answer::Pending(handle))
            .unwrap()
    } else {
        game.activate_internal_card("recipient3", AbilityId(4), PlayerId { seat: 1 })
            .unwrap()
    };
    for index in 0..total {
        let CardPlay::Awaiting(target) = play else {
            panic!("public recipient")
        };
        let InputDomain::Targets(domain) = &target.domain else {
            panic!("public targets")
        };
        play = game
            .answer_internal_card(
                target.actor,
                target.token,
                Answer::Targets(vec![domain.members[index].handle]),
            )
            .unwrap();
    }
    if let CardPlay::Awaiting(question) = &play
        && let InputDomain::Targets(domain) = &question.domain
        && domain
            .members
            .iter()
            .any(|member| member.card.as_deref() == Some("unit-amulet"))
    {
        play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap();
    }
    for _ in 0..total {
        let CardPlay::Awaiting(question) = play else {
            panic!("private evolution resource")
        };
        let InputDomain::Targets(resources) = &question.domain else {
            panic!("private evolution forms")
        };
        play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![resources.members[0].handle]),
            )
            .unwrap();
    }
    if let CardPlay::Awaiting(question) = &play {
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("cemetery re-entry choice")
        };
        assert!(
            domain
                .members
                .iter()
                .all(|member| member.card.as_deref() == Some("unit-amulet")),
            "the cemetery choice must select the genuinely departed observer"
        );
        play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap();
    }
    resolved(play);
    checks(game);
}

fn resolve_at(game: &mut Game, index: usize) {
    use sve_engine::game::internal::InputDomain;
    checks(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("ordinal pending")
    };
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(
                options
                    .iter()
                    .filter(|option| option.card == "unit-amulet")
                    .nth(index)
                    .unwrap()
                    .handle,
            ),
        )
        .unwrap(),
    );
    checks(game);
}

#[test]
fn four_event_captures_are_independent_of_later_history_and_resolution_order() {
    for order in [[0, 0, 0, 0], [2, 0, 1, 0]] {
        let mut game = fixture(false, false);
        effect_batch(&mut game, 4, false);
        let expected = if order[0] == 0 {
            [2, 4, 7, 11]
        } else {
            [4, 5, 9, 11]
        };
        for (index, life) in order.into_iter().zip(expected) {
            resolve_at(&mut game, index);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                life,
                "each pending retains the ordinal of its own event"
            );
        }
    }
}

#[test]
fn history_before_observer_entry_and_own_evolutions_on_opponent_turn_are_counted() {
    let mut late = fixture(true, false);
    effect_evolve(&mut late, 0);
    assert!(late.internal_pending_choice().unwrap().is_none());
    assert!(matches!(
        late.play_internal_card("observer", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    checks(&mut late);
    effect_evolve(&mut late, 1);
    resolve_at(&mut late, 0);
    assert_eq!(
        late.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        3,
        "the new observer captures the second evolution, including its pre-entry history"
    );
    let mut other_turn = fixture(false, true);
    other_turn.internal_rule_action(&json!({"do":"evolve","source":"opponent","evolve_card":"opponent-form","pay":{"pp":1}})).unwrap();
    checks(&mut other_turn);
    assert!(
        other_turn.internal_pending_choice().unwrap().is_none(),
        "the opponent's actual evolution does not trigger the own observer"
    );
    effect_batch(&mut other_turn, 1, true);
    resolve_at(&mut other_turn, 0);
    assert_eq!(
        other_turn.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        2,
        "own subject controller, not active player, determines whose evolution counts"
    );
}

#[test]
fn captured_values_survive_source_departure_without_modifying_its_hand_generation() {
    let mut game = fixture_with_departure(false, false, SourceReturn::Hand);
    effect_batch(&mut game, 3, false);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["observer"])
    );

    for _ in 0..3 {
        resolve_at(&mut game, 0);
    }
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["leader"]["life"], 7,
        "all three saved ordinals still resolve after the source leaves"
    );
    assert_eq!(
        view["objects"]["observer"]["power"], 2,
        "the old holder must not buff its hand generation"
    );
    assert_eq!(view["objects"]["observer"]["hp"], 3);
    assert!(
        !view["objects"]["observer"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("疾走")),
        "the old holder must not grant Storm to its hand generation"
    );
}

#[test]
fn natural_event_capture_uses_the_same_closed_query_and_formal_play_headers_stay_closed() {
    let original = lower(&tree(&read()));
    let mut natural = original.card().clone();
    let Capture::Integer {
        at,
        value: Expr::Compute(operation),
        ..
    } = natural.abilities[0].node.event_captures[0].clone()
    else {
        panic!("integer capture")
    };
    let IntOperation::FromNat { value } = *operation else {
        panic!("natural history")
    };
    let binding = Binding::<Nat>::new(999, ScopeId(1));
    natural.abilities[0].node.event_captures[0] = Capture::Natural {
        at,
        value,
        into: CaptureRef { binding },
    };
    let Node::Instruction { instruction } =
        &mut natural.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("life instruction")
    };
    let Action::Life {
        change: NumericChange::Add { amount },
        ..
    } = &mut instruction.action
    else {
        panic!("life delta")
    };
    *amount = Expr::Compute(Box::new(IntOperation::FromNat {
        value: Expr::Captured(CaptureRef { binding }),
    }));
    load(&WellFormedCardProgram::check(natural).unwrap()).unwrap();
    let mut formal = original.card().clone();
    formal.abilities[0].node.play = Some(PlaySpec {
        parameters: vec![],
        captures: vec![],
        costs: CostSpec::Ordered { steps: vec![] },
        limit: None,
    });
    let checked = WellFormedCardProgram::check(formal).unwrap();
    load(&checked).unwrap_err();
}

#[test]
fn captured_ordinals_do_not_follow_a_source_that_returns_to_the_field() {
    let mut game = fixture_with_departure(false, false, SourceReturn::Field);
    effect_batch(&mut game, 3, false);
    let returned = game.projection(View::Referee).unwrap();
    assert!(
        returned["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("observer")),
        "the source genuinely re-enters through a new cemetery choice and successful field move"
    );
    for _ in 0..3 {
        resolve_at(&mut game, 0);
    }
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["leader"]["life"], 7);
    assert_eq!(
        view["objects"]["observer"]["power"], 2,
        "the old pending must not buff the new field generation"
    );
    assert_eq!(view["objects"]["observer"]["hp"], 3);
    assert!(
        !view["objects"]["observer"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("疾走"))
    );
}
