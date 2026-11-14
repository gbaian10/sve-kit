use super::*;
use sve_engine::game::internal::InputDomain;

fn tree(steps: &Value) -> Value {
    equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0_u8},"do":steps}}),
        "unit-amulet",
    )
}

#[test]
fn author_shuffle_shapes_are_closed_and_do_not_expose_unused_variants() {
    let valid = tree(&json!([
        {"move":{"ref":{"who":"self","zone":"cemetery"},"to":"deck"}},
        {"shuffle":{"who":"self"}}, {"look":{"top":{"who":"self","n":2_u8},"bind":"seen"}}
    ]));
    let programs = lowerer().lower(&valid).unwrap();
    assert!(
        matches!(&programs["unit-amulet"].card().abilities[0].node.body.as_ref().unwrap().steps[1].node,
        Node::Instruction {instruction} if matches!(instruction.action,Action::Shuffle {scope:ShuffleScope::FullZone {},..}))
    );
    for steps in [
        json!([{"shuffle":{"who":"opp"}}]),
        json!([{"shuffle":{"who":"self","result":"saved"}}]),
        json!([{"shuffle":{"who":"self","precondition":true}}]),
        json!([{"move":{"ref":{"who":"self","zone":"cemetery"},"to":"deck"}}]),
        json!([{"move":{"ref":{"who":"self","zone":"cemetery"},"to":"deck","result":"saved"}},{"shuffle":{"who":"self"}},{"shuffle":{"who":"self","result":"saved"}}]),
        json!([{"move":{"ref":{"who":"opp","zone":"cemetery"},"to":"deck"}},{"shuffle":{"who":"self"}}]),
        json!([{"move":{"ref":{"who":"self","zone":"hand","filter":{"kind":"follower"}},"to":"deck"}},{"shuffle":{"who":"self"}}]),
        json!([{"move":{"ref":{"who":"self","zone":"hand"},"to":"deck","result":"saved"}},{"shuffle":{"who":"self"}},{"move":{"ref":{"in":{"objects_of":"saved","view":"before"}},"to":"deck","position":"bottom","order":"shuffle"}}]),
        json!([{"look":{"top":{"who":"self","n":3_u8},"bind":"seen"}},{"move":{"ref":{"in":"seen"},"to":"deck","position":"top","order":"shuffle"}}]),
        json!([{"look":{"top":{"who":"self","n":3_u8},"bind":"seen"}},{"move":{"ref":{"in":"seen"},"to":"deck","position":"bottom","order":"shuffle","result":"saved"}}]),
    ] {
        lowerer().lower(&tree(&steps)).unwrap_err();
    }
}

fn player_procedure() -> WellFormedCardProgram {
    lowerer()
        .lower(&tree(&json!([{"each_player":{"as":"p","stages":[
            [
                {"move":{"ref":{"who":"p","zone":["hand","cemetery"]},"to":"deck"}},
                {"shuffle":{"who":"p"}}
            ],
            [{"draw":{"who":"p","n":2_u8}}],
            [{"draw":{"who":"p","n":1_u8}}]
        ]}}])))
        .unwrap()
        .remove("unit-amulet")
        .unwrap()
}

fn player_procedure_game(random: &Value, seat: u32) -> Game {
    let mut initial = setup(seat, json!([]));
    let actor = if seat == 1 { "P1" } else { "P2" };
    initial["turn"]["active"] = json!(actor);
    initial["players"][actor]["zones"]["field"] = json!([{"id":"source","card":"unit-amulet"}]);
    for player in ["P1", "P2"] {
        for (zone, prefix, count) in [("hand", "h", 2_u8), ("cemetery", "g", 1), ("deck", "d", 2)] {
            initial["players"][player]["zones"][zone] = json!((0..count)
                .map(|index| json!({"id":format!("{player}{prefix}{index}"),"card":"unit-follower"}))
                .collect::<Vec<_>>());
        }
    }
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        random,
        "player-procedure",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &player_procedure())
        .unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    game
}

#[test]
fn player_stages_return_both_whole_hands_atomically_then_cross_players_per_draw() {
    let program = player_procedure();
    let ability = program.card().abilities[0].node.id;
    for seat in [1, 2] {
        let mut game = player_procedure_game(&Value::Null, seat);
        let result = resolved(
            game.activate_internal_card("source", ability, PlayerId { seat })
                .unwrap(),
        );
        let movements = result
            .progress
            .receipts
            .values()
            .find_map(|outcome| {
                if let InstructionOutcome::Move(outcome) = outcome {
                    Some(outcome)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(movements.moved.len(), 6);
        assert!(movements.moved.iter().all(|movement| {
            movement
                .actual_to
                .is_some_and(|zone| zone.kind == ZoneKind::Deck)
        }));
        let draws = result
            .progress
            .facts
            .iter()
            .filter_map(|fact| {
                if let Fact::Draw(draw) = fact {
                    Some((draw.player.seat, draw.ordinal.0))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            draws,
            vec![
                (seat, 1),
                (3 - seat, 1),
                (seat, 2),
                (3 - seat, 2),
                (seat, 1),
                (3 - seat, 1)
            ]
        );
        let view = game.projection(View::Referee).unwrap();
        for player in ["P1", "P2"] {
            assert_eq!(view[player]["cemetery"], json!([]));
            assert_eq!(view[player]["hand"].as_array().unwrap().len(), 3);
            assert_eq!(view[player]["deck"].as_array().unwrap().len(), 2);
        }
        assert!(game.internal_decision(View::P1).is_none());
        assert!(game.internal_decision(View::P2).is_none());
    }
}

#[test]
fn player_stage_failure_rolls_back_both_players_and_random_state() {
    let random = json!({"shuffles":[
        {"player":"P1","result":["P1d0","P1d1","P1h0","P1h1","P1g0"]},
        {"player":"P2","result":["missing"]}
    ]});
    let mut game = player_procedure_game(&random, 1);
    let before = exact_authority(&game);
    let ability = player_procedure().card().abilities[0].node.id;
    let error = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("scripted shuffle is not a permutation")
    );
    assert_eq!(exact_authority(&game), before);
}

#[test]
fn whole_hand_return_counts_successful_moves_for_the_following_draw() {
    let program = lowerer()
        .lower(&tree(&json!([
            {"move":{"ref":{"who":"self","zone":"hand"},"to":"deck","result":"returned"}},
            {"shuffle":{"who":"self"}},
            {"draw":{"n":{"count_of":"returned"}}}
        ])))
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let mut initial = setup(1, json!([{"id":"d","card":"unit-follower"}]));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-amulet"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"h1","card":"unit-follower"}, {"id":"h2","card":"unit-follower"}
    ]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "whole-hand-count",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &program).unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    let result = resolved(
        game.activate_internal_card(
            "source",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    assert!(result.progress.receipts.values().any(|outcome| matches!(
        outcome,
        InstructionOutcome::Cards(CardQuantityOutcome {
            requested: Int(2),
            actual: Nat(2)
        })
    )));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"].as_array().unwrap().len(), 2);
    assert_eq!(view["P1"]["deck"].as_array().unwrap().len(), 1);
}

#[test]
fn resolving_spell_self_banish_preserves_surrounding_draw_order_and_cleanup() {
    for banish in [false, true] {
        let steps = if banish {
            vec![
                draw(1, 1),
                instruction(
                    2,
                    Action::Banish {
                        objects: Selector::Objects {
                            objects: vec![ObjectOperand::SourceHolder {}],
                        },
                    },
                ),
                draw(3, 1),
                instruction(
                    4,
                    Action::Banish {
                        objects: Selector::Objects {
                            objects: vec![ObjectOperand::SourceHolder {}],
                        },
                    },
                ),
            ]
        } else {
            vec![draw(1, 2)]
        };
        let program = card(vec![play::spell(steps)]);
        let mut initial = setup(
            1,
            json!([
                {"id":"d0","card":"unit-follower"}, {"id":"d1","card":"unit-follower"}
            ]),
        );
        initial["players"]["P1"]["zones"]["resolution"] =
            json!([{"id":"other","card":"unit-spell"}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "spell-self-banish",
        )
        .unwrap();
        game.load_internal_card("unit-spell", &program).unwrap();
        game.load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
        let result = resolved(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
        let trace = result
            .progress
            .facts
            .iter()
            .filter_map(|fact| {
                if matches!(fact, Fact::Draw(_)) {
                    Some("draw")
                } else if matches!(fact,Fact::Movement(moved) if moved.method == MoveMethod::Banish)
                {
                    Some("banish")
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            trace,
            if banish {
                vec!["draw", "banish", "draw"]
            } else {
                vec!["draw", "draw"]
            }
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["hand"], json!(["d0", "d1"]));
        assert_eq!(view["P1"]["resolution"], json!(["other"]));
        assert_eq!(
            view["P1"]["banish"],
            if banish { json!(["s"]) } else { json!([]) }
        );
        assert_eq!(
            view["P1"]["cemetery"],
            if banish { json!([]) } else { json!(["s"]) }
        );
    }
}

#[test]
fn resolving_spell_banish_rejects_a_capture_alias_and_non_spell_execution() {
    let holder = Selector::Objects {
        objects: vec![ObjectOperand::SourceHolder {}],
    };
    let binding = Binding::new(9, ScopeId(1));
    let capture = node(Node::Capture {
        capture: Capture::Objects {
            at: CaptureTime::AtResolution { scope: ScopeId(1) },
            value: holder.clone(),
            into: CaptureRef { binding },
        },
    });
    let program = card(vec![play::spell(vec![
        capture,
        instruction(
            2,
            Action::Banish {
                objects: Selector::Captured {
                    reference: CaptureRef { binding },
                },
            },
        ),
    ])]);
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "source-alias",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &program).unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    let before = exact_authority(&game);
    let error = game
        .play_internal_card("s", PlayerId { seat: 1 })
        .unwrap_err();
    assert!(error.to_string().contains("non-public movement origin"));
    assert_eq!(exact_authority(&game), before);

    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["resolution"] = json!([{"id":"s","card":"unit-spell"}]);
    let mut synthetic = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "non-spell-source",
    )
    .unwrap();
    let before = exact_authority(&synthetic);
    let error = synthetic
        .execute_internal(
            &checked(vec![instruction(1, Action::Banish { objects: holder })]),
            &source("synthetic"),
            context(1),
        )
        .unwrap_err();
    assert!(error.to_string().contains("non-public movement origin"));
    assert_eq!(exact_authority(&synthetic), before);
}

#[test]
fn cutthroat_fanfare_fragment_moves_one_card_to_hand_and_randomizes_only_the_looked_remainder() {
    // Isolate shuffle semantics from whole-card construction and admission checks.
    let input = tree(&json!([
        {"look":{"top":{"who":"self","n":8_u8},"bind":"seen"}},
        {"choose":{"from":{"in":"seen"},"n":1_u8,"bind":"picked"}},
        {"move":{"ref":"picked","to":"hand","result":"taken"}},
        {"move":{"ref":{"in":"seen","except":{"in":{"objects_of":"taken","view":"before"}}},"to":"deck","position":"bottom","order":"shuffle"}}
    ]));
    let program = lowerer()
        .lower(&input)
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let ability = program.card().abilities[0].node.id;
    let deck = (0_u8..10)
        .map(|i| json!({"id":format!("d{i}"),"card":"unit-follower"}))
        .collect::<Vec<_>>();
    let mut initial = setup(1, json!(deck));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-amulet"}]);
    let random =
        json!({"shuffles":[{"player":"P1","result":["d7","d6","d5","d4","d3","d2","d0"]}]});
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &random,
        "cutthroat-fragment",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &program).unwrap();
    for number in ["unit-follower", "unit-spell"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("Look choice");
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("one of the eight observed cards");
    };
    assert_eq!(domain.members.len(), 8);
    let result = resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[1].handle]),
        )
        .unwrap(),
    );
    assert!(
        !result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Shuffled { .. })),
        "a partial shuffle is not a full-deck shuffle"
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"], json!(["d1"]));
    assert_eq!(
        view["P1"]["deck"],
        json!(["d8", "d9", "d7", "d6", "d5", "d4", "d3", "d2", "d0"])
    );
    assert!(
        game.internal_decision(View::P1).is_none(),
        "random placement never asks for a chooser order"
    );
}
