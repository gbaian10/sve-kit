#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions fail these focused payment scenarios."
)]
use super::*;
use sve_engine::game::internal::InputDomain;

#[test]
fn mode_payment_rejects_resolution_alternatives_and_nonleading_payment_steps() {
    let paid = json!({"pay":{"cost":{"pp":1_i64},"do":[{"draw":{"n":1_i64}}]}});
    let cases = [
        json!([paid]),
        json!([{"either":{"options":[[paid],[{"draw":{"n":1_i64}}]]}}]),
        json!([{"choice":{"n":1_i64,"options":[[{"draw":{"n":1_i64}},paid],[{"draw":{"n":1_i64}}]]}}]),
    ];
    for steps in cases {
        let mut tree = fanfare::expanded::tree().clone();
        tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"] = steps;
        lowerer().lower(&tree).unwrap_err();
    }
}

fn mode_tree(cost: Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut card = tree["cards"]["BP08-038"].clone();
    card["abilities"][0]["fanfare"] = json!({"do":[{"choice":{"n":1,"options":[
        [{"pay":{"cost":null,"do":[{"life":{"who":"self","delta":2}}]}}],
        [{"life":{"who":"self","delta":1}}]
    ]}}]});
    card["abilities"][0]["fanfare"]["do"][0]["choice"]["options"][0][0]["pay"]["cost"] = cost;
    tree["cards"] = json!({"unit-follower":card});
    tree
}

fn start(cost: Value, mode: u64) -> (Game, CardPlay) {
    let program = lowerer().lower(&mode_tree(cost)).unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"s","card":"unit-follower"},{"id":"material","card":"unit-spell"}
    ]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "mode-payment",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program["unit-follower"])
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let CardPlay::Awaiting(modes) = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(1))
        .unwrap()
    else {
        panic!("mode choice")
    };
    let play = game
        .answer_internal_card(modes.actor, modes.token, Answer::Modes(vec![ModeId(mode)]))
        .unwrap();
    (game, play)
}

#[test]
fn selected_mode_fee_can_be_declined_and_an_unselected_fee_is_absent() {
    for (mode, accepted) in [(1, true), (1, false), (2, false)] {
        let (mut game, play) = start(json!({"pp":1}), mode);
        let before = game.projection(View::Referee).unwrap();
        let result = if mode == 1 {
            let CardPlay::Awaiting(cost) = play else {
                panic!("mode fee")
            };
            assert_eq!(cost.domain, InputDomain::OptionalCost);
            game.answer_internal_card(cost.actor, cost.token, Answer::OptionalCost(accepted))
                .unwrap()
        } else {
            play
        };
        resolved(result);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["leader"]["life"],
            if mode == 2 {
                21
            } else if accepted {
                22
            } else {
                20
            }
        );
        assert_eq!(
            state["P1"]["pp"]["current"].as_i64().unwrap(),
            before["P1"]["pp"]["current"].as_i64().unwrap() - i64::from(accepted && mode == 1)
        );
    }
}

#[test]
fn mode_hand_fee_is_private_and_is_not_requested_when_declined() {
    for accepted in [false, true] {
        let (mut game, play) = start(
            json!({"discard":{"sel":{"who":"self","zone":"hand"},"n":1}}),
            1,
        );
        let CardPlay::Awaiting(cost) = play else {
            panic!("mode fee")
        };
        let play = game
            .answer_internal_card(cost.actor, cost.token, Answer::OptionalCost(accepted))
            .unwrap();
        let result = if accepted {
            let CardPlay::Awaiting(materials) = play else {
                panic!("private fee")
            };
            assert!(game.internal_decision(View::P2).is_none());
            game.answer_internal_card(
                materials.actor,
                materials.token,
                Answer::CostMaterials(vec![0]),
            )
            .unwrap()
        } else {
            play
        };
        resolved(result);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["cemetery"],
            if accepted {
                json!(["material"])
            } else {
                json!([])
            }
        );
        assert_eq!(
            state["P1"]["leader"]["life"],
            if accepted { 22 } else { 20 }
        );
    }
}

#[test]
fn a_failed_late_mode_fee_rolls_back_earlier_pp_and_posture() {
    let (mut game, play) = start(
        json!({"steps":[{"pp":1},{"act_self":true},{"act_self":true}]}),
        1,
    );
    let CardPlay::Awaiting(cost) = play else {
        panic!("mode fee")
    };
    let before = game.projection(View::Referee).unwrap();
    game.answer_internal_card(cost.actor, cost.token, Answer::OptionalCost(true))
        .unwrap_err();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"], before["P1"]);
    assert_eq!(state["objects"], before["objects"]);
}

#[test]
fn a_mode_cannot_hide_a_second_required_fee_beside_its_optional_declaration() {
    let program = lowerer().lower(&mode_tree(json!({"pp":1}))).unwrap();
    let mut raw = program["unit-follower"].card().clone();
    let Node::Control {
        control: Control::Mode { modes, .. },
    } = &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("mode")
    };
    let Node::Control {
        control:
            Control::Pay {
                costs: CostSpec::Ordered { steps },
                ..
            },
    } = &modes[0].body.steps[0].node
    else {
        panic!("mode payment")
    };
    let mut extra = steps[0].clone();
    extra.instruction.id = InstructionId(9001);
    extra.instruction.receipt = ReceiptId(9001);
    modes[0].costs = CostSpec::Ordered { steps: vec![extra] };
    let checked = WellFormedCardProgram::check(raw).unwrap();
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "duplicate-mode-fee",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &checked)
        .unwrap_err();
}

fn fee_card(head: &str, body: Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry[head] = body;
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    tree
}

#[test]
fn selectable_mode_material_fees_cannot_share_one_material_pool() {
    let hand = json!({"discard":{"sel":{"who":"self","zone":"hand"},"n":1}});
    let ex = json!({"banish":{"sel":{"who":"self","zone":"ex"},"n":1}});
    let paid =
        |cost: &Value| json!([{"pay":{"cost":cost,"do":[{"life":{"who":"self","delta":1}}]}}]);
    let plain = json!([{"life":{"who":"self","delta":1}}]);
    let cases = [
        (
            "fanfare",
            json!({"do":[{"choice":{"n":1,"options":[paid(&hand),paid(&hand)]}}]}),
            None,
        ),
        (
            "fanfare",
            json!({"do":[{"choice":{"n":2,"options":[paid(&hand),paid(&hand)]}}]}),
            Some("several selectable mode material fees"),
        ),
        (
            "fanfare",
            json!({"do":[{"choice":{"n":2,"options":[paid(&ex),paid(&ex)]}}]}),
            Some("several selectable mode material fees"),
        ),
        (
            "activate",
            json!({"cost":hand,"do":[{"choice":{"n":1,"options":[paid(&ex),plain]}}]}),
            Some("mode material fee beside another material cost"),
        ),
    ];
    for (head, body, reason) in cases {
        let program = lowerer().lower(&fee_card(head, body)).unwrap();
        let mut game = Game::new(
            catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "mode-material-pool",
        )
        .unwrap();
        let loaded = game.load_internal_card("unit-follower", &program["unit-follower"]);
        match reason {
            None => loaded.unwrap(),
            Some(reason) => {
                let error = loaded.unwrap_err();
                assert!(
                    format!("{error:?}").contains(reason),
                    "the shared-pool gate rejects it, not another check: {error:?}"
                );
            }
        }
    }
}

#[test]
fn two_selected_fees_ask_both_choices_then_both_targets_before_one_public_material() {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"].clone();
    entry["abilities"][0]["fanfare"] = json!({
        "targets":[
            {"bind":"first","from":{"who":"opp","zone":"field","kind":"follower"},"n":1,"option":1},
            {"bind":"second","from":{"who":"opp","zone":"field","kind":"follower"},"n":1,"option":2}
        ],
        "do":[{"choice":{"n":2,"options":[
            [{"pay":{"cost":{"banish":{"sel":{"who":"self","zone":"ex"},"n":1}},
                "do":[{"damage":{"to":"first","n":1}}]}}],
            [{"pay":{"cost":{"pp":1},"do":[{"damage":{"to":"second","n":1}}]}}]
        ]}}]
    });
    tree["cards"] = json!({"unit-follower":entry});
    let program = lowerer().lower(&tree).unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"repair","card":"unit-amulet"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"x","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "two-mode-fees",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program["unit-follower"])
        .unwrap();
    for id in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(id, &card(vec![])).unwrap();
    }
    game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let CardPlay::Awaiting(modes) = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(1))
        .unwrap()
    else {
        panic!("mode choice")
    };
    let before = game.projection(View::Referee).unwrap();
    let mut play = game
        .answer_internal_card(
            modes.actor,
            modes.token,
            Answer::Modes(vec![ModeId(1), ModeId(2)]),
        )
        .unwrap();
    let mut asked = vec![];
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => {
                asked.push("fee");
                Answer::OptionalCost(true)
            }
            InputDomain::Targets(domain) => {
                asked.push("target");
                Answer::Targets(vec![domain.members[0].handle])
            }
            InputDomain::CostMaterials(domain) => {
                asked.push("material");
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            _ => panic!("unexpected two-fee question"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    resolved(play);
    assert_eq!(
        asked,
        ["fee", "fee", "target", "target", "material"],
        "fee choices precede every formal target, and cost materials come last"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!([]), "the accepted EX fee is paid");
    assert_eq!(
        state["P1"]["pp"]["current"].as_i64().unwrap(),
        before["P1"]["pp"]["current"].as_i64().unwrap() - 1_i64,
        "the accepted PP fee is paid"
    );
    assert_eq!(
        state["objects"]["x"]["damage"], 2_i64,
        "both paid bodies resolve"
    );
}

#[test]
fn player_question_resolution_payment_keeps_acceptance_separate_from_declared_cost() {
    use crate::player_pipeline::{answer, question};
    use sve_engine::game::player::{
        DecisionShape, QuestionPurpose, QuestionShape, SubmissionKind, Viewer,
    };
    for accept in [false, true] {
        let tree = evolution_linked_movement::tree(&[(
            "auto",
            json!({"on":{"phase":{"whose":"self","which":"end"}},"do":[{"pay":{"cost":{"act_self":true},"do":[{"life":{"who":"self","delta":2}}]}}]}),
        )]);
        let program = lowerer()
            .lower(&tree)
            .unwrap()
            .remove("unit-follower")
            .unwrap();
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":"unit-follower"}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "player-resolution-pay",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program).unwrap();
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        game.advance_player_input();
        assert_eq!(
            answer(&mut game, Viewer::P1, Answer::Pending(1)).kind,
            SubmissionKind::Applied
        );
        game.advance_player_input();
        let payment = question(&game, Viewer::P1);
        assert_eq!(payment.purpose, QuestionPurpose::ResolutionChoice);
        assert_eq!(
            payment.shape,
            QuestionShape::Decision(DecisionShape::ResolutionPayment)
        );
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            answer(&mut game, Viewer::P1, Answer::OptionalCost(accept)).kind,
            SubmissionKind::Applied
        );
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["leader"]["life"], if accept { 22 } else { 20 });
        assert_eq!(
            after["P1"]["pp"]["current"].as_u64().unwrap(),
            before["P1"]["pp"]["current"].as_u64().unwrap()
        );
    }
}

#[test]
fn player_question_cost_discard_keeps_the_native_material_answer_channel() {
    use crate::player_pipeline::{answer, question};
    use sve_engine::game::player::{
        DecisionShape, DiscardKind, QuestionShape, SubmissionKind, Viewer,
    };
    let (mut game, _) = start(
        json!({"discard":{"sel":{"who":"self","zone":"hand"},"n":1}}),
        1,
    );
    game.advance_player_input();
    assert_eq!(
        question(&game, Viewer::P1).shape,
        QuestionShape::Decision(DecisionShape::OptionalCost)
    );
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::OptionalCost(true)).kind,
        SubmissionKind::Applied
    );
    let discard = question(&game, Viewer::P1);
    let QuestionShape::Decision(DecisionShape::Discard {
        kind: DiscardKind::Cost,
        cards,
    }) = discard.shape
    else {
        panic!("cost discard")
    };
    assert_eq!(cards.members.len(), 1);
    assert_eq!(cards.cardinality.minimum, Nat(1));
    assert_eq!(
        answer(
            &mut game,
            Viewer::P1,
            Answer::CostMaterials(vec![cards.members[0].handle])
        )
        .kind,
        SubmissionKind::Applied
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(state["P1"]["cemetery"], json!(["material"]));
    assert_eq!(state["P1"]["leader"]["life"], 22);
}
