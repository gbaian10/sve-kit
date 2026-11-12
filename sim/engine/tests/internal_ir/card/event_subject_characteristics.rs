#![expect(
    clippy::panic,
    reason = "Unexpected synthetic decisions invalidate the event oracle."
)]
use super::*;
use alloc::collections::BTreeMap;
use core::slice::from_ref;
use sve_engine::game::internal::InputDomain;

fn tree(head: &Value) -> Value {
    tree_for("unit-follower", from_ref(head))
}

fn tree_for(number: &str, heads: &[Value]) -> Value {
    let mut source = fanfare::expanded::tree().clone();
    let template = source["cards"]["BP08-038"]["abilities"][0].clone();
    let meta = source["cards"]["BP08-038"]["meta"].clone();
    let entries = heads
        .iter()
        .enumerate()
        .map(|(index, head)| {
            let mut entry = template.clone();
            entry.as_object_mut().unwrap().remove("fanfare");
            entry["sources"][0]["ability_scope"] =
                json!(format!("subject_characteristics_{index}"));
            for (key, value) in head.as_object().unwrap() {
                entry[key] = value.clone();
            }
            entry
        })
        .collect::<Vec<_>>();
    source["cards"] = json!({number:{"meta":meta,"abilities":entries}});
    source
}

fn loads(head: &Value) {
    let program = lowerer()
        .lower(&tree(head))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut game = Game::new(
        static_keywords::keyword_catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "subject-characteristics",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
}

#[test]
fn own_follower_moved_to_cemetery_can_filter_its_effective_ward_before_exit() {
    loads(&json!({"auto":{
        "on":{"moved":{"from":{"who":"self","zone":"field"},"to":"cemetery","sel":{"kind":"follower","keyword":"ward"}}},
        "do":[{"damage":{"to":{"leaders":"opponents"},"n":1}}]
    }}));
}

#[test]
fn another_own_follower_entry_can_filter_its_original_cost() {
    loads(&json!({"auto":{
        "on":{"enter_field":{"who":"self","sel":{"kind":"follower","orig_cost_eq":2,"other":true}}},
        "do":[{"damage":{"to":{"leaders":"opponents"},"n":1}}]
    }}));
}

#[test]
fn wilbert_look_with_ward_cost_and_own_name_exclusion_already_lowers_and_loads() {
    loads(&json!({"fanfare":{"do":[
        {"look":{"top":{"who":"self","n":5},"bind":"seen"}},
        {"choose":{"bind":"picked","from":{"in":"seen","kind":"follower","keyword":"ward","orig_cost_le":5,"not_same_name_as_this":true},"n":1,"may":true}},
        {"move":{"ref":"picked","to":"field","result":"picked_move"}},
        {"move":{"ref":{"in":"seen","except":{"in":{"objects_of":"picked_move","view":"before"}}},"to":"deck","position":"bottom","order":"chooser"}}
    ]}}));
}

fn ward_observer() -> Value {
    json!({"auto":{"on":{"moved":{"from":{"who":"self","zone":"field"},"to":"cemetery","sel":{"kind":"follower","keyword":"ward"}}},"do":[{"damage":{"to":{"leaders":"opponents"},"n":1}}]}})
}

fn entry_observer() -> Value {
    json!({"auto":{"on":{"enter_field":{"who":"self","sel":{"kind":"follower","orig_cost_eq":2,"other":true}}},"do":[{"damage":{"to":{"leaders":"opponents"},"n":1}}]}})
}

fn fixture(heads: &[(&str, Vec<Value>)], initial: &Value, cost: u64) -> Game {
    fixture_named(heads, initial, cost, None)
}

fn fixture_named(
    heads: &[(&str, Vec<Value>)],
    initial: &Value,
    cost: u64,
    name: Option<&str>,
) -> Game {
    let mut input = tree(&ward_observer());
    input["cards"] = json!({});
    for (number, heads) in heads {
        input["cards"][number] = tree_for(number, heads)["cards"][number].clone();
    }
    let costs = heads
        .iter()
        .map(|(number, _)| ((*number).to_owned(), Nat(cost)))
        .collect::<BTreeMap<_, _>>();
    let programs = lowerer().lower_with_printed_costs(&input, &costs).unwrap();
    let (mut snapshot, _, document) = catalog_documents(&json!(cost.to_string()));
    if let Some(name) = name {
        snapshot = snapshot
            .lines()
            .map(|line| {
                let mut row: Value = serde_json::from_str(line).unwrap();
                if row["number"] == "unit-follower" {
                    row["faces"][0]["name"] = json!(name);
                }
                row.to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../authored/rules/keywords.yaml"
            )),
            &[("unit.yaml".into(), document)],
        )
        .unwrap(),
    );
    let mut game = Game::new(
        catalog,
        initial,
        &Value::Null,
        &Value::Null,
        "event-subject-runtime",
    )
    .unwrap();
    for (number, program) in programs {
        game.load_internal_card(&number, &program).unwrap();
    }
    game
}

fn initial() -> Value {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":10,"max":10});
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"observer","card":"unit-amulet"},
        {"id":"first","card":"unit-follower"},
        {"id":"second","card":"unit-follower"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial
}

fn activate_observer(game: &mut Game) {
    let sources = game.internal_activated_sources("observer").unwrap();
    assert_eq!(
        sources.len(),
        1,
        "the helper has one actual activated definition"
    );
    resolved(
        game.activate_internal_card("observer", sources[0].definition, PlayerId { seat: 1 })
            .unwrap(),
    );
}

fn drain(game: &mut Game) -> u8 {
    let mut count = 0_u8;
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            return count;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("only event occurrences should remain");
        };
        resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
        count = count.checked_add(1).unwrap();
    }
}

#[test]
fn static_ward_is_frozen_before_the_whole_batch_and_its_provider_leave() {
    let mut game = fixture(
        &[
            (
                "unit-amulet",
                vec![
                    ward_observer(),
                    json!({"static":{"effects":[{"grant":{"on":{"who":"self","zone":"field","kind":"follower"},"keywords":["ward"]}}]}}),
                    json!({"activate":{"cost":{"pp":0},"do":[{"move":{"ref":{"who":"self","zone":"field"},"to":"cemetery"}}]}}),
                ],
            ),
            ("unit-follower", vec![]),
            ("unit-spell", vec![]),
        ],
        &initial(),
        2,
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["first"]["keywords"],
        json!(["守護"])
    );
    activate_observer(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"],
        json!([])
    );
    assert_eq!(drain(&mut game), 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 18);
    assert_eq!(state["objects"]["first"]["keywords"], json!([]));
}

#[test]
fn installed_ward_counts_and_only_the_own_follower_cemetery_move_matches() {
    for destination in ["cemetery", "hand", "banish"] {
        let movement = if destination == "banish" {
            json!({"banish":{"ref":{"who":"self","zone":"field","kind":"follower"}}})
        } else {
            json!({"move":{"ref":{"who":"self","zone":"field","kind":"follower"},"to":destination}})
        };
        let mut game = fixture(
            &[
                (
                    "unit-amulet",
                    vec![
                        ward_observer(),
                        json!({"activate":{"cost":{"pp":0},"do":[
                            {"grant":{"on":{"who":"self","zone":"field","kind":"follower"},"keywords":["ward"]}},
                            movement
                        ]}}),
                    ],
                ),
                ("unit-follower", vec![]),
                ("unit-spell", vec![]),
            ],
            &initial(),
            2,
        );
        activate_observer(&mut game);
        let expected = if destination == "cemetery" { 2 } else { 0 };
        assert_eq!(drain(&mut game), expected);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            20 - expected
        );
    }
}

#[test]
fn printed_entry_cost_remains_distinct_from_its_real_discounted_payment() {
    for cost in [2, 3] {
        let mut state = initial();
        state["players"]["P1"]["zones"]["field"] = json!([{"id":"observer","card":"unit-amulet"}]);
        state["players"]["P1"]["zones"]["hand"] = json!([{"id":"incoming","card":"unit-follower"}]);
        let mut game = fixture(
            &[
                ("unit-amulet", vec![entry_observer()]),
                ("unit-follower", vec![json!({"play":{"cost_delta":-1}})]),
                ("unit-spell", vec![]),
            ],
            &state,
            cost,
        );
        assert!(matches!(
            game.play_internal_card("incoming", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            11 - cost
        );
        let expected = u8::from(cost == 2);
        assert_eq!(drain(&mut game), expected);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            20 - expected
        );
    }
}

#[test]
fn movement_keyword_does_not_escape_to_other_headers_or_history() {
    for on in [
        json!({"enter_field":{"who":"self","sel":{"kind":"follower"}}}),
        json!({"left_field":{"who":"self","sel":{"kind":"follower"}}}),
        json!({"destroyed":{"who":"self","sel":{"kind":"follower"}}}),
        json!({"played":{"who":"self","sel":{"kind":"follower"}}}),
        json!({"placed_ex":{"who":"self","sel":{"kind":"follower"}}}),
    ] {
        let mut head = json!({"auto":{"on":on,"do":[{"life":{"who":"self","delta":1}}]}});
        lowerer().lower(&tree(&head)).unwrap();
        let (_, event) = head["auto"]["on"]
            .as_object_mut()
            .unwrap()
            .iter_mut()
            .next()
            .unwrap();
        event["sel"]["keyword"] = json!("ward");
        lowerer().lower(&tree(&head)).unwrap_err();
    }
    for keyword in ["storm", "rush", "aura"] {
        let mut head = ward_observer();
        head["auto"]["on"]["moved"]["sel"]["keyword"] = json!(keyword);
        lowerer().lower(&tree(&head)).unwrap_err();
    }
    let mut head = json!({"auto":{"on":{"phase":{"which":"end","whose":"self"}},"do":[{"life":{"who":"self","delta":{"history_count":{"window":"this_turn","event":"left_field","who":"self","sel":{"kind":"follower"}}}}}]}});
    lowerer().lower(&tree(&head)).unwrap();
    head["auto"]["do"][0]["life"]["delta"]["history_count"]["sel"]["keyword"] = json!("ward");
    lowerer().lower(&tree(&head)).unwrap_err();
}

#[test]
fn raw_keyword_headers_keep_the_committed_subject_view_and_ward_predicate_closed() {
    let valid = lowerer()
        .lower(&tree(&ward_observer()))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for case in 0_u8..4 {
        let mut candidate = valid.card().clone();
        let EventPattern::LeaveField { predicate, .. } =
            (match &mut candidate.abilities[0].node.trigger.as_mut().unwrap().on {
                TriggerCondition::Event { pattern } => pattern,
                TriggerCondition::State { .. } => panic!("the seed is a committed event"),
            })
        else {
            panic!("the seed is a departure");
        };
        let Expr::Compute(root) = predicate else {
            panic!("canonical conjunction");
        };
        let Predicate::And { predicates } = root.as_mut() else {
            panic!("canonical conjunction");
        };
        let (abilities, ability) = predicates
            .iter_mut()
            .find_map(|part| {
                if let Expr::Compute(operation) = part
                    && let Predicate::HasAbility {
                        abilities,
                        predicate,
                    } = operation.as_mut()
                {
                    Some((abilities, predicate))
                } else {
                    None
                }
            })
            .unwrap();
        if case == 0 {
            ability.keyword = Some(KeywordId::Storm);
        } else {
            let Expr::Live(read) = abilities else {
                panic!("committed effective abilities");
            };
            let AbilitiesRead::Abilities { object, view } = read.as_mut();
            match case {
                1 => *view = CharacteristicView::Original,
                2 => {
                    let ObjectView::EventSnapshot { policy, .. } = object else {
                        panic!("event snapshot");
                    };
                    *policy = EventSnapshotPolicy::AfterEntry;
                }
                _ => {
                    *object = ObjectView::Current {
                        object: Box::new(ObjectOperand::SourceHolder {}),
                    }
                }
            }
        }
        WellFormedCardProgram::check(candidate).unwrap_err();
    }
}

#[test]
fn a_canonical_ward_departure_header_does_not_grant_a_keyword_history_query() {
    let valid = lowerer()
        .lower(&tree(&ward_observer()))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let TriggerCondition::Event { pattern } = valid.card().abilities[0]
        .node
        .trigger
        .as_ref()
        .unwrap()
        .on
        .clone()
    else {
        panic!("the seed has a departure event");
    };
    let query = HistoryQuery {
        pattern,
        window: HistoryWindow {
            range: HistoryRange::CurrentTurn {},
            cutoff: HistoryCutoff::CurrentPoint {},
            relative_player: None,
        },
        include_execution: InclusionPolicy::IncludeAll {},
    };
    let program = CardProgram {
        abilities: vec![ability(
            1,
            vec![instruction(
                1,
                Action::Life {
                    player: PlayerRef::Controller {},
                    change: NumericChange::Add {
                        amount: Expr::Compute(Box::new(IntOperation::FromNat {
                            value: Expr::Live(Box::new(NatRead::HistoryCount {
                                query,
                                count: HistoryCount::Facts,
                            })),
                        })),
                    },
                },
            )],
        )],
        card_play_costs: vec![],
    };
    WellFormedCardProgram::check(program).unwrap_err();
}

#[test]
fn original_cost_entry_keeps_the_own_other_follower_shape_and_one_bound() {
    for case in 0_u8..8 {
        let mut head = entry_observer();
        let entry = &mut head["auto"]["on"]["enter_field"];
        match case {
            0 => entry["who"] = json!("opp"),
            1 => {
                entry.as_object_mut().unwrap().remove("who");
            }
            2 => {
                entry["sel"].as_object_mut().unwrap().remove("other");
            }
            3 => entry["sel"]["kind"] = json!("amulet"),
            4 => entry["sel"]["orig_cost_le"] = json!(2),
            5 => entry["sel"]["orig_cost_eq"] = json!(u64::MAX),
            6 => entry["sel"]["trait"] = json!("妖精"),
            _ => entry["during"] = json!({"turn":"self"}),
        }
        lowerer().lower(&tree(&head)).unwrap_err();
    }
}

#[test]
fn damage_subject_keywords_remain_rejected_after_movement_keyword_admission() {
    let mut head = json!({"auto":{
        "on":{"dealt_damage":{"who":"self","zone":"field","sel":{"trait":"妖精"},"recipient":{"who":"opp","zone":"field","kind":"follower"},"during":{"turn":"self"}}},
        "aggregation":"batch","do":[{"damage":{"to":{"leaders":"opponents"},"n":1}}]
    }});
    lowerer().lower(&tree(&head)).unwrap();
    head["auto"]["on"]["dealt_damage"]["sel"]["keyword"] = json!("ward");
    lowerer().lower(&tree(&head)).unwrap_err();
}

#[test]
fn bp13_094_entry_comparisons_include_printed_zero_before_enqueue() {
    for cost in [0, 1, 2, 3] {
        for (bound, threshold) in [
            ("orig_cost_eq", 0),
            ("orig_cost_le", 2),
            ("orig_cost_ge", 2),
        ] {
            let mut observer = entry_observer();
            let sel = &mut observer["auto"]["on"]["enter_field"]["sel"];
            sel.as_object_mut().unwrap().remove("orig_cost_eq");
            sel[bound] = json!(threshold);
            let mut state = initial();
            state["players"]["P1"]["zones"]["field"] =
                json!([{"id":"observer","card":"unit-amulet"}]);
            state["players"]["P1"]["zones"]["hand"] =
                json!([{"id":"incoming","card":"unit-follower"}]);
            let mut game = fixture(
                &[
                    ("unit-amulet", vec![observer]),
                    ("unit-follower", vec![]),
                    ("unit-spell", vec![]),
                ],
                &state,
                cost,
            );
            assert!(matches!(
                game.play_internal_card("incoming", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ));
            let expected = u8::from(match bound {
                "orig_cost_eq" => cost == threshold,
                "orig_cost_le" => cost <= threshold,
                _ => cost >= threshold,
            });
            assert_eq!(
                drain(&mut game),
                expected,
                "printed {cost}, {bound} {threshold}"
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
                20 - expected
            );
        }
    }
}

#[test]
fn bp13_094_a_zero_cost_entry_batch_keeps_two_per_fact_occurrences() {
    let mut observer = entry_observer();
    let sel = &mut observer["auto"]["on"]["enter_field"]["sel"];
    sel.as_object_mut().unwrap().remove("orig_cost_eq");
    sel["orig_cost_le"] = json!(2);
    let producer = json!({"activate":{"cost":{"pp":0},"targets":[{"bind":"returned","from":{"who":"self","zone":"cemetery","kind":"follower"},"n":2}],"do":[{"move":{"ref":"returned","to":"field","result":"entered"}}]}});
    let mut state = initial();
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"observer","card":"unit-amulet"}]);
    state["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"first","card":"unit-follower"},{"id":"second","card":"unit-follower"}]);
    let mut game = fixture(
        &[
            ("unit-amulet", vec![observer, producer]),
            ("unit-follower", vec![]),
            ("unit-spell", vec![]),
        ],
        &state,
        0,
    );
    let source = game
        .internal_activated_sources("observer")
        .unwrap()
        .remove(0);
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("observer", source.definition, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("two cemetery targets")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("cemetery targets")
    };
    assert_eq!(domain.members.len(), 2);
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(domain.members.iter().map(|m| m.handle).collect()),
        )
        .unwrap(),
    );
    assert_eq!(drain(&mut game), 2);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        18
    );
}

fn name_union_observer() -> Value {
    let branch = |name: &str| json!({"enter_field":{"who":"self","sel":{"kind":"follower","other":true,"name_contains":name}}});
    json!({"auto":{"per_turn":1,"on":{"any_of":[branch("早坂美玲"),branch("星輝子")]},"do":[{"buff":{"on":"it","power":1,"hp":1}}]}})
}

#[test]
fn ecp02_name_or_is_one_occurrence_even_when_both_fragments_match() {
    for (name, expected) in [
        ("早坂美玲", true),
        ("星輝子", true),
        ("早坂美玲・星輝子", true),
        ("美鈴", false),
    ] {
        let mut state = initial();
        state["players"]["P1"]["zones"]["field"] = json!([{"id":"observer","card":"unit-amulet"}]);
        state["players"]["P1"]["zones"]["hand"] = json!([{"id":"incoming","card":"unit-follower"}]);
        let mut observer = name_union_observer();
        observer["auto"].as_object_mut().unwrap().remove("per_turn");
        let mut game = fixture_named(
            &[
                ("unit-amulet", vec![observer]),
                ("unit-follower", vec![]),
                ("unit-spell", vec![]),
            ],
            &state,
            1,
            Some(name),
        );
        assert!(matches!(
            game.play_internal_card("incoming", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        assert_eq!(drain(&mut game), u8::from(expected));
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["incoming"]["power"],
            2 + u8::from(expected)
        );
    }
}

#[test]
fn name_or_rejects_foreign_events_roles_and_duplicate_branches() {
    lowerer().lower(&tree(&name_union_observer())).unwrap();
    for case in 0..4 {
        let mut head = name_union_observer();
        match case {
            0 => head["auto"]["on"]["any_of"][1]["enter_field"]["who"] = json!("opp"),
            1 => head["auto"]["on"]["any_of"][1] = json!({"phase":{"whose":"self","which":"end"}}),
            2 => head["auto"]["on"]["any_of"][1] = head["auto"]["on"]["any_of"][0].clone(),
            _ => head["auto"]["zone"] = json!("ex"),
        }
        lowerer().lower(&tree(&head)).unwrap_err();
    }
}

#[test]
fn raw_zero_cost_comparison_requires_the_same_known_printing_guard() {
    let mut head = entry_observer();
    head["auto"]["on"]["enter_field"]["sel"]["orig_cost_eq"] = json!(0);
    let valid = lowerer()
        .lower(&tree(&head))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "known-cost-baseline",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &valid).unwrap();
    let mut raw = valid.card().clone();
    let TriggerCondition::Event {
        pattern:
            EventPattern::EnterField {
                predicate: Expr::Compute(root),
                ..
            },
    } = &mut raw.abilities[0].node.trigger.as_mut().unwrap().on
    else {
        panic!("entry predicate")
    };
    let Predicate::And { predicates } = root.as_mut() else {
        panic!("conjunction")
    };
    let before = predicates.len();
    predicates.retain(|part| !matches!(part,Expr::Compute(operation) if matches!(operation.as_ref(),Predicate::Not{predicate:Expr::Compute(inner)} if matches!(inner.as_ref(),Predicate::Equal{operands} if matches!(operands.as_ref(),ValueEquality::InfoInt{..})))));
    assert_eq!(predicates.len().checked_add(1).unwrap(), before);
    let error = WellFormedCardProgram::check(raw).unwrap_err();
    assert!(error.ir_path.contains("/trigger"));
}

fn not_class_program(targeted: bool) -> WellFormedCardProgram {
    let mut head = json!({"auto":{"per_turn":1,"on":{"enter_field":{"who":"self","sel":{"kind":"follower","other":true,"not_class":"ドラゴン"}}},"do":[{"life":{"who":"self","delta":1}}]}});
    if targeted {
        head["auto"]["targets"] =
            json!([{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}]);
        head["auto"]["do"] =
            json!([{"damage":{"to":"enemy","n":1}},{"life":{"who":"self","delta":1}}]);
    }
    lowerer()
        .lower(&tree_for("unit-amulet", from_ref(&head)))
        .unwrap()
        .remove("unit-amulet")
        .unwrap()
}

#[test]
fn author_not_class_stays_in_the_entry_subject_family() {
    not_class_program(false);
    for on in [
        json!({"left_field":{"who":"self","sel":{"kind":"follower","class":"ドラゴン"}}}),
        json!({"destroyed":{"who":"self","sel":{"kind":"follower","class":"ドラゴン"}}}),
        json!({"placed_ex":{"who":"self","sel":{"kind":"follower","class":"ドラゴン"}}}),
        json!({"discarded":{"who":"self","sel":{"kind":"follower","class":"ドラゴン"}}}),
    ] {
        let mut head = json!({"auto":{"on":on,"do":[{"life":{"who":"self","delta":1}}]}});
        lowerer().lower(&tree(&head)).unwrap();
        let (_, event) = head["auto"]["on"]
            .as_object_mut()
            .unwrap()
            .iter_mut()
            .next()
            .unwrap();
        event["sel"].as_object_mut().unwrap().remove("class");
        event["sel"]["not_class"] = json!("ドラゴン");
        let error = lowerer().lower(&tree(&head)).unwrap_err();
        assert_eq!(error.code, "unsupported_construct");
    }
    for excluded in ["", "ドラゴン"] {
        let mut head = json!({"auto":{"on":{"enter_field":{"who":"self","sel":{"kind":"follower","not_class":excluded}}},"do":[{"life":{"who":"self","delta":1}}]}});
        if !excluded.is_empty() {
            head["auto"]["on"]["enter_field"]["sel"]["class"] = json!("ニュートラル");
        }
        lowerer().lower(&tree(&head)).unwrap_err();
    }
}

fn not_class_board(missing: bool, targeted: bool) -> Game {
    let (snapshot, registry, document) = catalog_documents(&json!("1"));
    let mut rows = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let mut neutral = rows
        .iter()
        .find(|row| row["number"] == "unit-follower")
        .unwrap()
        .clone();
    neutral["number"] = json!("unit-neutral");
    for row in &mut rows {
        if row["number"] == "unit-follower" {
            if missing {
                row["faces"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("card_class");
            } else {
                row["faces"][0]["card_class"] = json!("ドラゴン");
            }
        }
    }
    rows.push(neutral);
    let mut document: Value = serde_json::from_str(&document).unwrap();
    document["cards"]["unit-neutral"] = json!({"status":"complete","abilities":[]});
    let catalog = Arc::new(
        Catalog::from_documents(
            &rows
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
            &registry,
            &[("unit.yaml".into(), document.to_string())],
        )
        .unwrap(),
    );
    let mut state = initial();
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"observer","card":"unit-amulet"}]);
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"mismatch","card":"unit-follower"},{"id":"match","card":"unit-neutral"},{"id":"again","card":"unit-neutral"},{"id":"quick","card":"unit-spell"}]);
    state["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"returned","card":"unit-neutral"}]);
    for player in ["P1", "P2"] {
        state["players"][player]["zones"]["deck"] = json!([
            {"id":format!("{player}-draw1"),"card":"unit-follower"},
            {"id":format!("{player}-draw2"),"card":"unit-follower"}
        ]);
    }
    let mut game = Game::new(
        catalog,
        &state,
        &Value::Null,
        &Value::Null,
        "not-class-entry",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &not_class_program(targeted))
        .unwrap();
    for number in ["unit-follower", "unit-neutral"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    let head = json!({"spell":{"targets":[{"bind":"returned","from":{"who":"self","zone":"cemetery","kind":"follower"},"n":1}],"do":[{"move":{"ref":"returned","to":"field","result":"entered"}}]}});
    let mut spell = lowerer()
        .lower_with_printed_costs(
            &tree_for("unit-spell", from_ref(&head)),
            &BTreeMap::from([("unit-spell".into(), Nat(1))]),
        )
        .unwrap()
        .remove("unit-spell")
        .unwrap()
        .card()
        .clone();
    spell.abilities.push(quick::permit());
    game.load_internal_card("unit-spell", &WellFormedCardProgram::check(spell).unwrap())
        .unwrap();
    game
}

#[test]
fn not_dragon_entry_mismatches_and_unknown_class_do_not_consume_the_turn_quota() {
    for missing in [false, true] {
        let mut game = not_class_board(missing, false);
        if missing {
            game.play_internal_card("mismatch", PlayerId { seat: 1 })
                .unwrap_err();
        } else {
            assert!(matches!(
                game.play_internal_card("mismatch", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ));
        }
        assert_eq!(drain(&mut game), 0);
        assert!(matches!(
            game.play_internal_card("match", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        assert_eq!(drain(&mut game), 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            21
        );
        assert!(matches!(
            game.play_internal_card("again", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        assert_eq!(drain(&mut game), 0);
    }
}

#[test]
fn not_class_entry_also_observes_an_ability_put_during_the_opponents_turn() {
    let mut game = not_class_board(false, false);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    quick::pass(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P2"
    );
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let q = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &q.domain else {
        panic!("Quick card window")
    };
    assert_eq!(q.actor, PlayerId { seat: 1 });
    assert_eq!(options.len(), 1);
    let CardPlay::Awaiting(target) = game
        .answer_internal_card(q.actor, q.token, Answer::Quick(Some(options[0].handle)))
        .unwrap()
    else {
        panic!("cemetery choice")
    };
    let InputDomain::Targets(domain) = &target.domain else {
        panic!("one follower target")
    };
    assert_eq!(domain.members.len(), 1);
    resolved(
        game.answer_internal_card(
            target.actor,
            target.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    assert_eq!(drain(&mut game), 1);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        21
    );
}

#[test]
fn bp21_071_cannot_heal_when_the_required_opposing_target_is_absent() {
    let mut game = not_class_board(false, true);
    assert!(matches!(
        game.play_internal_card("match", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("one not-class occurrence")
    };
    assert_eq!(options.len(), 1);
    assert!(matches!(
        game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap(),
        CardPlay::Cancelled
    ));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        20
    );
}

#[test]
fn raw_not_class_cannot_turn_missing_information_into_a_match() {
    let mut raw = not_class_program(false).card().clone();
    let TriggerCondition::Event {
        pattern:
            EventPattern::EnterField {
                predicate: Expr::Compute(root),
                ..
            },
    } = &mut raw.abilities[0].node.trigger.as_mut().unwrap().on
    else {
        panic!("entry")
    };
    let Predicate::And { predicates } = root.as_mut() else {
        panic!("conjunction")
    };
    let before = predicates.len();
    predicates.retain(|part| {
        let Expr::Compute(operation) = part else {
            return true;
        };
        let Predicate::Not {
            predicate: Expr::Compute(inner),
        } = operation.as_ref()
        else {
            return true;
        };
        !matches!(
            inner.as_ref(),
            Predicate::ClassEquals {
                right: Expr::Literal(Info::AbsentByRules),
                ..
            }
        )
    });
    assert_eq!(predicates.len().checked_add(1).unwrap(), before);
    let error = WellFormedCardProgram::check(raw).unwrap_err();
    assert!(error.ir_path.contains("/trigger"));
}

#[test]
fn raw_name_union_cannot_read_the_observer_in_place_of_the_entering_subject() {
    let mut raw = lowerer()
        .lower(&tree(&name_union_observer()))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
        .card()
        .clone();
    let TriggerCondition::Event {
        pattern:
            EventPattern::EnterField {
                predicate: Expr::Compute(root),
                ..
            },
    } = &mut raw.abilities[0].node.trigger.as_mut().unwrap().on
    else {
        panic!("entry")
    };
    let Predicate::And { predicates } = root.as_mut() else {
        panic!("conjunction")
    };
    let branch = predicates
        .iter_mut()
        .find_map(|part| {
            if let Expr::Compute(operation) = part
                && let Predicate::Or { predicates } = operation.as_mut()
            {
                predicates.first_mut()
            } else {
                None
            }
        })
        .unwrap();
    let Expr::Compute(operation) = branch else {
        panic!("name branch")
    };
    let Predicate::NameContains { object, .. } = operation.as_mut() else {
        panic!("name fragment")
    };
    *object = ObjectView::Current {
        object: Box::new(ObjectOperand::SourceHolder {}),
    };
    let error = WellFormedCardProgram::check(raw).unwrap_err();
    assert!(error.ir_path.contains("/trigger"));
}

#[test]
fn new_departure_cost_comparisons_do_not_extend_history_read_authority() {
    for (bound, cost, accepted) in [
        ("orig_cost_eq", 2, true),
        ("orig_cost_eq", 0, false),
        ("orig_cost_le", 2, false),
    ] {
        let mut sel = json!({"kind":"follower"});
        sel[bound] = json!(cost);
        let head = json!({"auto":{"on":{"left_field":{"who":"self","sel":sel}},"do":[]}});
        let valid = lowerer()
            .lower(&tree(&head))
            .unwrap()
            .remove("unit-follower")
            .unwrap();
        let TriggerCondition::Event { pattern } = valid.card().abilities[0]
            .node
            .trigger
            .as_ref()
            .unwrap()
            .on
            .clone()
        else {
            panic!("departure")
        };
        let query = HistoryQuery {
            pattern,
            window: HistoryWindow {
                range: HistoryRange::CurrentTurn {},
                cutoff: HistoryCutoff::CurrentPoint {},
                relative_player: None,
            },
            include_execution: InclusionPolicy::IncludeAll {},
        };
        let program = CardProgram {
            abilities: vec![ability(
                1,
                vec![instruction(
                    1,
                    Action::Life {
                        player: PlayerRef::Controller {},
                        change: NumericChange::Add {
                            amount: Expr::Compute(Box::new(IntOperation::FromNat {
                                value: Expr::Live(Box::new(NatRead::HistoryCount {
                                    query,
                                    count: HistoryCount::Facts,
                                })),
                            })),
                        },
                    },
                )],
            )],
            card_play_costs: vec![],
        };
        assert_eq!(
            WellFormedCardProgram::check(program).is_ok(),
            accepted,
            "{bound}={cost}"
        );
    }
}
