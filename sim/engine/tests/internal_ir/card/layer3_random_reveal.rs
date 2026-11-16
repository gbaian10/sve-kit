use super::*;
use sve_engine::EngineFailure;
use sve_engine::game::internal::{InputDomain, InstructionOutcome};

fn tree(steps: &Value) -> Value {
    evolution_linked_movement::tree(&[("activate", json!({"cost":{"pp":0},"do":steps}))])
}

fn position(seat: u32, hand: &Value) -> Value {
    let actor = if seat == 1 { "P1" } else { "P2" };
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(actor);
    initial["players"][actor]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    initial["players"][actor]["zones"]["hand"] = hand.clone();
    initial
}

fn loaded(steps: &Value, initial: &Value, random: &Value, seed: &str) -> Game {
    let programs = lowerer().lower(&tree(steps)).unwrap();
    let mut game = Game::new(catalog(), initial, &Value::Null, random, seed).unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

#[test]
fn random_reveal_samples_the_whole_hand_and_only_publishes_its_sample_during_resolution() {
    let steps = json!([
        {"reveal":{"ref":{"who":"self","zone":"hand"},"random":{"n":2},"bind":"shown"}},
        {"choose":{"bind":"pause","from":{"who":"self","zone":"hand"},"upto":1}}
    ]);
    for seat in [1_u32, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let observer = if seat == 1 { View::P2 } else { View::P1 };
        let initial = position(
            seat,
            &json!([
                {"id":"h0","card":"unit-spell"},
                {"id":"h1","card":"unit-follower"},
                {"id":"h2","card":"unit-spell"}
            ]),
        );
        let script = json!({"random_selections":[{"n":1_u8,"from":format!("{actor}.hand"),"result":["h0","h1"]}]});
        let mut game = loaded(&steps, &initial, &script, "random-reveal");
        let CardPlay::Awaiting(q) = game
            .activate_internal_card("source", AbilityId(1), PlayerId { seat })
            .unwrap()
        else {
            panic!("the later hand choice pauses after the sample is public");
        };
        let InputDomain::Targets(domain) = &q.domain else {
            panic!("private pause");
        };
        assert_eq!(domain.members.len(), 3);
        let during = game.projection(observer).unwrap();
        assert_eq!(during["objects"]["h0"]["card"], "unit-spell");
        assert_eq!(during["objects"]["h1"]["card"], "unit-follower");
        assert!(
            during["objects"]["h2"].is_null(),
            "the unselected card stays private"
        );
        assert!(
            game.internal_decision(observer).is_none(),
            "publication does not authorize answering"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()[actor]["hand"],
            json!(["h0", "h1", "h2"])
        );
        let result = resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![]))
                .unwrap(),
        );
        let sizes: Vec<_> = result
            .progress
            .receipts
            .values()
            .filter_map(|outcome| {
                if let InstructionOutcome::Revealed(members) = outcome {
                    Some(members.len())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            sizes,
            [2],
            "the receipt contains only the two successful public members"
        );
        let after = game.projection(observer).unwrap();
        for id in ["h0", "h1", "h2"] {
            assert!(
                after["objects"][id].is_null(),
                "hand identities are concealed when the resolution ends"
            );
        }
    }
}

#[test]
fn random_reveal_uses_available_cards_and_an_empty_sample_never_requests_randomness() {
    for (cards, requested, expected) in [(0_u8, 2_u64, 0_usize), (1, 2, 1), (3, 5, 3), (3, 0, 0)] {
        let hand = json!(
            (0..cards)
                .map(|n| json!({"id":format!("h{n}"),"card":"unit-spell"}))
                .collect::<Vec<_>>()
        );
        let result_ids: Vec<_> = (0..expected).map(|n| format!("h{n}")).collect();
        let scripts = if expected == 0 {
            json!([])
        } else {
            json!([{"n":1_u8,"from":"P1.hand","result":result_ids}])
        };
        let steps = json!([{ "reveal":{"ref":{"who":"self","zone":"hand"},"random":{"n":requested},"bind":"shown"}}]);
        let mut game = loaded(
            &steps,
            &position(1, &hand),
            &json!({"random_selections":scripts}),
            "short-sample",
        );
        let result = resolved(
            game.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        let sizes: Vec<_> = result
            .progress
            .receipts
            .values()
            .filter_map(|outcome| {
                if let InstructionOutcome::Revealed(members) = outcome {
                    Some(members.len())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(sizes, [expected], "Q1578 uses the actual hand size");
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"]
                .as_array()
                .unwrap()
                .len(),
            usize::from(cards)
        );
    }
}

#[test]
fn random_reveal_and_random_discard_share_the_stream_without_replaying_after_a_bad_answer() {
    let steps = json!([
        {"reveal":{"ref":{"who":"self","zone":"hand"},"random":{"n":2},"bind":"shown"}},
        {"choose":{"bind":"pause","from":{"who":"self","zone":"hand"},"upto":1}},
        {"discard":{"who":"self","n":1,"random":true}}
    ]);
    let initial = position(
        1,
        &json!([
            {"id":"h0","card":"unit-spell"}, {"id":"h1","card":"unit-follower"}, {"id":"h2","card":"unit-spell"}
        ]),
    );
    let script = json!({"random_selections":[
        {"n":1_u8,"from":"P1.hand","result":["h0","h1"]},
        {"n":2_u8,"from":"P1.hand","result":["h2"]}
    ]});
    let mut game = loaded(&steps, &initial, &script, "shared-stream");
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("private pause");
    };
    let before = exact_authority(&game);
    game.answer_internal_card(PlayerId { seat: 2 }, q.token, Answer::Targets(vec![]))
        .unwrap_err();
    assert_eq!(
        exact_authority(&game),
        before,
        "a rejected answer preserves the sampled prefix"
    );
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![]))
            .unwrap(),
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["hand"], json!(["h0", "h1"]));
    assert_eq!(
        after["P1"]["cemetery"],
        json!(["h2"]),
        "the next common RNG occurrence performs the discard"
    );
}

#[test]
fn a_random_revelation_of_one_opposing_hand_keeps_unselected_cards_private() {
    let steps = json!([
        {"reveal":{"ref":{"who":"opp","zone":"hand"},"random":{"n":1},"bind":"shown"}},
        {"choose":{"bind":"pause","from":{"who":"self","zone":"hand"},"upto":1}}
    ]);
    let mut initial = position(1, &json!([{"id":"own","card":"unit-spell"}]));
    initial["players"]["P2"]["zones"]["hand"] = json!([
        {"id":"enemy0","card":"unit-spell"}, {"id":"enemy1","card":"unit-follower"}
    ]);
    let script = json!({"random_selections":[{"n":1_u8,"from":"P2.hand","result":["enemy0"]}]});
    let mut game = loaded(&steps, &initial, &script, "opposing-sample");
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("private pause");
    };
    let during = game.projection(View::P1).unwrap();
    assert_eq!(during["objects"]["enemy0"]["card"], "unit-spell");
    assert!(
        during["objects"]["enemy1"].is_null(),
        "one publication does not authorize the other hand member"
    );
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![]))
            .unwrap(),
    );
    assert!(
        game.projection(View::P1).unwrap()["objects"]["enemy0"].is_null(),
        "the opposing card is concealed again"
    );
}

#[test]
fn random_reveal_rejects_anonymous_members_and_invalid_samples_without_a_partial_commit() {
    let steps =
        json!([{ "reveal":{"ref":{"who":"self","zone":"hand"},"random":{"n":2},"bind":"shown"}}]);
    for (anonymous, selected) in [
        (true, json!(["h0"])),
        (false, json!(["h0", "h0"])),
        (false, json!(["h0"])),
        (false, json!(["h0", "source"])),
    ] {
        let mut hand = json!([{"id":"h0","card":"unit-spell"}]);
        hand.as_array_mut().unwrap().push(if anonymous {
            json!({"filler":1_u8})
        } else {
            json!({"id":"h1","card":"unit-follower"})
        });
        let script = json!({"random_selections":[{"n":1_u8,"from":"P1.hand","result":selected}]});
        let mut game = loaded(&steps, &position(1, &hand), &script, "invalid-sample");
        let before = exact_authority(&game);
        for _ in 0..2 {
            let error = game
                .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
                .unwrap_err();
            if anonymous {
                assert!(matches!(error, EngineFailure::Unsupported(_)));
            }
            assert_eq!(
                exact_authority(&game),
                before,
                "failed sampling restores revelation, resources and script cursors"
            );
        }
    }
}

#[test]
fn random_reveal_keeps_sampling_authority_closed_to_private_unfiltered_hands() {
    for reveal in [
        json!({"ref":{"who":"all","zone":"hand"},"random":{"n":2},"bind":"shown"}),
        json!({"ref":{"who":"self","zone":"hand","kind":"follower"},"random":{"n":2},"bind":"shown"}),
        json!({"top":{"who":"self","n":2},"random":{"n":2},"bind":"shown"}),
    ] {
        lowerer()
            .lower(&tree(&json!([{ "reveal":reveal}])))
            .unwrap_err();
    }
    let input = tree(
        &json!([{ "reveal":{"ref":{"who":"self","zone":"hand"},"random":{"n":2},"bind":"shown"}}]),
    );
    let program = lowerer()
        .lower(&input)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for mutation in 0..3_u8 {
        let mut raw = program.card().clone();
        let Node::Instruction { instruction } =
            &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
        else {
            panic!("random revelation");
        };
        let Action::Random {
            population,
            visibility,
            ..
        } = &mut instruction.action
        else {
            panic!("random revelation");
        };
        if mutation == 2 {
            *visibility = Visibility::Public {};
        } else if let Selector::Zone {
            zone, predicate, ..
        } = population
        {
            if mutation == 0 {
                *zone = ZoneKind::Deck;
            } else {
                **predicate = Expr::Literal(Bool(false));
            }
        } else {
            panic!("the lowered sample uses a whole hand");
        }
        let checked = WellFormedCardProgram::check(raw).unwrap();
        let mut game = Game::new(
            catalog(),
            &position(1, &json!([])),
            &Value::Null,
            &Value::Null,
            "closed-sample",
        )
        .unwrap();
        let before = exact_authority(&game);
        let error = game
            .load_internal_card("unit-follower", &checked)
            .unwrap_err();
        assert!(
            matches!(error, EngineFailure::Unsupported(_)),
            "a fresh loader rejects the unsupported shape"
        );
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn sampling_does_not_publish_until_the_explicit_reveal_instruction() {
    let input = tree(&json!([
        {"reveal":{"ref":{"who":"self","zone":"hand"},"random":{"n":1},"bind":"shown"}},
        {"choose":{"bind":"pause","from":{"who":"self","zone":"hand"},"upto":1}}
    ]));
    let program = lowerer()
        .lower(&input)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut raw = program.card().clone();
    raw.abilities[0]
        .node
        .body
        .as_mut()
        .unwrap()
        .steps
        .swap(1, 2);
    let checked = WellFormedCardProgram::check(raw).unwrap();
    let initial = position(1, &json!([{"id":"h0","card":"unit-spell"}]));
    let script = json!({"random_selections":[{"n":1_u8,"from":"P1.hand","result":["h0"]}]});
    let mut game = Game::new(catalog(), &initial, &Value::Null, &script, "private-sample").unwrap();
    game.load_internal_card("unit-follower", &checked).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("pause between sample and Reveal");
    };
    assert!(
        game.projection(View::P2).unwrap()["objects"]["h0"].is_null(),
        "the sampling instruction alone does not disclose the card"
    );
    let result = resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![]))
            .unwrap(),
    );
    assert!(
        result.progress.receipts.values().any(
            |outcome| matches!(outcome, InstructionOutcome::Revealed(members) if members.len()==1)
        ),
        "the following Reveal consumes the saved sample"
    );
}

#[test]
fn a_sample_cannot_publish_a_relocated_generation_and_failure_restores_the_prefix() {
    let input = tree(
        &json!([{ "reveal":{"ref":{"who":"self","zone":"hand"},"random":{"n":1},"bind":"shown"}}]),
    );
    let program = lowerer()
        .lower(&input)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut raw = program.card().clone();
    let body = raw.abilities[0].node.body.as_mut().unwrap();
    let mut discard = body.steps[0].clone();
    let Node::Instruction { instruction } = &mut discard.node else {
        panic!("sample");
    };
    instruction.id = InstructionId(1000);
    instruction.receipt = ReceiptId(1000);
    let Action::Random { population, .. } = &instruction.action else {
        panic!("sample population");
    };
    let mut eligible = population.clone();
    if let Selector::Zone { binding, .. } = &mut eligible {
        *binding = Binding::new(900, body.scope);
    }
    instruction.action = Action::Discard {
        player: PlayerRef::Controller {},
        mode: DiscardMode::All { eligible },
    };
    body.steps.insert(1, discard);
    let checked = WellFormedCardProgram::check(raw).unwrap();
    let initial = position(1, &json!([{"id":"h0","card":"unit-spell"}]));
    let script = json!({"random_selections":[{"n":1_u8,"from":"P1.hand","result":["h0"]}]});
    let mut game = Game::new(catalog(), &initial, &Value::Null, &script, "stale-sample").unwrap();
    game.load_internal_card("unit-follower", &checked).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    let before = exact_authority(&game);
    for _ in 0..2 {
        let error = game
            .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
            .unwrap_err();
        assert!(
            matches!(error, EngineFailure::Unsupported(_)),
            "Reveal requires the sampled hand generation"
        );
        assert_eq!(
            exact_authority(&game),
            before,
            "a stale sample does not commit a discard, publication or RNG cursor"
        );
    }
}
