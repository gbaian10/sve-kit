use super::*;
use alloc::collections::BTreeMap;

#[test]
fn accepted_modes_keep_printed_order_after_the_play_state_is_consumed() {
    let program = program(
        "activate",
        json!({"cost":{"pp":0},"do":[
            {"choice":{"n":2,"options":[
                [{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}],
                [{"optional":{"do":[{"life":{"who":"self","delta":2}}]}}]
            ]}}
        ]}),
    );
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = fixture(&program, actor);
        let q = start(&mut game, &program, actor);
        let native = game.internal_decision(View::Referee).unwrap();
        let decision = native.token.decision;
        let InputDomain::Modes { modes, .. } = native.domain else {
            panic!("formal modes");
        };
        assert_eq!(modes.len(), 2);
        parameters(&game, &[]);
        let before = exact_authority(&game);
        assert_eq!(
            submit(
                &mut game,
                actor,
                &q,
                Answer::Modes(vec![modes[0], modes[0]])
            ),
            SubmissionKind::Rejected
        );
        assert_eq!(
            submit(&mut game, actor, &q, Answer::Modes(vec![ModeId(u64::MAX)])),
            SubmissionKind::Rejected
        );
        assert!(
            exact_authority(&game) == before,
            "invalid mode IDs do not declare modes"
        );
        let mut selected = modes.clone();
        selected.reverse();
        assert_eq!(
            submit(&mut game, actor, &q, Answer::Modes(selected)),
            SubmissionKind::Applied
        );
        let expected = [PlayDeclaration::Modes {
            player: actor.player(),
            decision,
            modes,
        }];
        parameters(&game, &expected);
        assert_eq!(game.internal_play_phase(), Some(PlayPhase::Resolve));
        decline(&mut game, actor);
        parameters(&game, &expected);
        closed_wire(&game, actor);
        decline(&mut game, actor);
        assert_eq!(game.internal_play_declarations(), None);
    }
}

fn spell_fixture(actor: Viewer) -> (Game, WellFormedCardProgram) {
    let mut document = payment_fixtures::x_document(
        fanfare::expanded::tree(),
        "spell",
        json!({
            "x":{},"optional_cost":{"pp":"x","result":"paid"},
            "do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]
        }),
    )
    .unwrap();
    let entry = document["cards"]["unit-follower"].take();
    document["cards"] = json!({"unit-spell":entry});
    let program = lowerer()
        .lower_with_printed_costs(&document, &BTreeMap::from([("unit-spell".into(), Nat(1))]))
        .unwrap()
        .remove("unit-spell")
        .unwrap();
    let (seat, me) = if actor == Viewer::P1 {
        (1, "P1")
    } else {
        (2, "P2")
    };
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(me);
    initial["players"][me]["pp"] = json!({"current":3,"max":3});
    initial["players"][me]["zones"]["hand"] = json!([{"id":"spell","card":"unit-spell"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "play-declarations",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &program).unwrap();
    for number in ["unit-follower", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    (game, program)
}

#[test]
fn optional_cost_acceptance_is_not_payment_and_declining_does_not_declare_implicit_x() {
    for actor in [Viewer::P1, Viewer::P2] {
        for accepted in [false, true] {
            let me = if actor == Viewer::P1 { "P1" } else { "P2" };
            let (mut game, _) = spell_fixture(actor);
            game.play_internal_card("spell", actor.player()).unwrap();
            let optional = question(&game, actor);
            let receipt = ReceiptId(
                game.internal_decision(View::Referee)
                    .unwrap()
                    .token
                    .decision
                    .0,
            );
            parameters(&game, &[]);
            assert_eq!(
                submit(&mut game, actor, &optional, Answer::OptionalCost(accepted)),
                SubmissionKind::Applied
            );
            let mut expected = vec![PlayDeclaration::OptionalCost {
                player: actor.player(),
                receipt,
                required: false,
                accepted,
            }];
            if accepted {
                parameters(&game, &expected);
                assert_eq!(
                    game.projection(View::Referee).unwrap()[me]["pp"]["current"],
                    3
                );
                let x = question(&game, actor);
                assert_eq!(
                    submit(&mut game, actor, &x, Answer::Integer(Nat(1))),
                    SubmissionKind::Applied
                );
                expected.push(PlayDeclaration::X {
                    player: actor.player(),
                    value: Nat(1),
                });
            }
            parameters(&game, &expected);
            assert_eq!(
                game.projection(View::Referee).unwrap()[me]["pp"]["current"],
                if accepted { 1 } else { 2 }
            );
            decline(&mut game, actor);
            assert_eq!(game.internal_play_declarations(), None);
        }
    }
}

fn end_trigger(game: &mut Game, actor: Viewer) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    game.advance_player_input();
    let pending = question(game, actor);
    let choices =
        if let QuestionShape::Decision(player::DecisionShape::Pending(choices)) = &pending.shape {
            Some(choices)
        } else {
            None
        };
    assert!(
        choices.is_some(),
        "the fixture requires a pending trigger question"
    );
    let choices = choices.unwrap();
    assert_eq!(
        choices.len(),
        1,
        "the fixture installs one eligible end trigger"
    );
    assert_eq!(
        submit(game, actor, &pending, Answer::Pending(choices[0].handle)),
        SubmissionKind::Applied,
        "choosing the pending trigger enters its play"
    );
}

#[test]
fn a_required_trigger_head_is_recorded_only_for_its_surviving_execution() {
    let program = program(
        "auto",
        json!({"on":{"phase":{"whose":"self","which":"end"}},"cost":{"pp":1},"do":[
            {"optional":{"do":[{"life":{"who":"self","delta":1}}]}}
        ]}),
    );
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = fixture(&program, actor);
        end_trigger(&mut game, actor);
        let head = question(&game, actor);
        let receipt = ReceiptId(
            game.internal_decision(View::Referee)
                .unwrap()
                .token
                .decision
                .0,
        );
        parameters(&game, &[]);
        assert_eq!(
            submit(&mut game, actor, &head, Answer::OptionalCost(true)),
            SubmissionKind::Applied
        );
        parameters(
            &game,
            &[PlayDeclaration::OptionalCost {
                player: actor.player(),
                receipt,
                required: true,
                accepted: true,
            }],
        );
        decline(&mut game, actor);
        assert_eq!(game.internal_play_declarations(), None);
    }
}

#[test]
fn a_resolution_payment_answer_is_not_a_play_time_cost_declaration() {
    let program = program(
        "auto",
        json!({"on":{"phase":{"whose":"self","which":"end"}},"do":[
            {"pay":{"cost":{"pp":1},"do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]}}
        ]}),
    );
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = fixture(&program, actor);
        end_trigger(&mut game, actor);
        let payment = question(&game, actor);
        assert_eq!(game.internal_question_stage(), Some(QuestionStage::Pay));
        parameters(&game, &[]);
        assert_eq!(
            submit(&mut game, actor, &payment, Answer::OptionalCost(true)),
            SubmissionKind::Applied
        );
        parameters(&game, &[]);
        decline(&mut game, actor);
        assert_eq!(game.internal_play_declarations(), None);
    }
}
