use super::*;
use sve_engine::EngineFailure;
use sve_engine::game::internal::{InputDomain, InstructionOutcome};

fn steps() -> Value {
    json!([
        {"reveal":{"ref":{"who":"self","zone":"hand"},"random":{"n":2},"bind":"shown"}},
        {"move":{"ref":{"in":"shown","kind":"follower"},"to":"field","result":"entered"}}
    ])
}

#[test]
fn revealed_arrivals_fill_every_free_space_and_only_overflow_requires_a_choice() {
    for seat in [1_u32, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let observer = if seat == 1 { View::P2 } else { View::P1 };
        for occupied in [3_usize, 4, 5] {
            let hand = json!([
                {"id":"a","card":"unit-follower"},
                {"id":"b","card":"unit-follower"},
                {"id":"hidden","card":"unit-follower"}
            ]);
            let mut game = loaded(seat, &hand, occupied, &json!(["a", "b"]));
            let play = game
                .activate_internal_card("source", AbilityId(1), PlayerId { seat })
                .unwrap();
            let play = if occupied == 4 {
                let CardPlay::Awaiting(q) = play else {
                    panic!("two public followers compete for the one remaining space");
                };
                assert_eq!(q.actor, PlayerId { seat });
                let InputDomain::SearchPartition(domain) = &q.domain else {
                    panic!("the ordinary movement capacity question");
                };
                assert_eq!(
                    domain.cardinality,
                    Cardinality {
                        minimum: Nat(1),
                        maximum: Some(Nat(1))
                    }
                );
                assert_eq!(domain.cards.len(), 2);
                let public = game.projection(observer).unwrap();
                assert_eq!(public["objects"]["a"]["card"], "unit-follower");
                assert_eq!(public["objects"]["b"]["card"], "unit-follower");
                assert!(public["objects"]["hidden"].is_null());
                let before = exact_authority(&game);
                for handles in [vec![], vec![0, 1], vec![0, 0], vec![99]] {
                    game.answer_internal_card(q.actor, q.token, Answer::SearchPartition(handles))
                        .unwrap_err();
                    assert_eq!(exact_authority(&game), before);
                }
                game.answer_internal_card(
                    PlayerId { seat: 3 - seat },
                    q.token,
                    Answer::SearchPartition(vec![1]),
                )
                .unwrap_err();
                assert_eq!(exact_authority(&game), before);
                game.answer_internal_card(q.actor, q.token, Answer::SearchPartition(vec![1]))
                    .unwrap()
            } else {
                play
            };
            resolved(play);
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after[actor]["field"].as_array().unwrap().len(), 5);
            let expected = match occupied {
                3 => json!(["hidden"]),
                4 => json!(["a", "hidden"]),
                _ => json!(["a", "b", "hidden"]),
            };
            assert_eq!(after[actor]["hand"], expected);
            let public = game.projection(observer).unwrap();
            assert!(public["objects"]["hidden"].is_null());
            if occupied >= 4 {
                assert!(
                    public["objects"]["a"].is_null(),
                    "unmoved revealed cards become hidden again"
                );
            }
        }
    }
}

fn loaded(seat: u32, hand: &Value, occupied: usize, sample: &Value) -> Game {
    let actor = if seat == 1 { "P1" } else { "P2" };
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(actor);
    initial["players"][actor]["zones"]["hand"] = hand.clone();
    let mut field = vec![json!({"id":"source","card":"unit-follower"})];
    field.extend((1..occupied).map(|i| json!({"id":format!("old-{i}"),"card":"unit-follower"})));
    initial["players"][actor]["zones"]["field"] = json!(field);
    let script =
        json!({"random_selections":[{"n":1_u8,"from":format!("{actor}.hand"),"result":sample}]});
    configured(&steps(), &initial, &script)
}

fn configured(steps: &Value, initial: &Value, script: &Value) -> Game {
    let tree =
        evolution_linked_movement::tree(&[("activate", json!({"cost":{"pp":0},"do":steps}))]);
    let programs = lowerer().lower(&tree).unwrap();
    let mut game = Game::new(catalog(), initial, &Value::Null, script, "revealed-entry").unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    game
}

#[test]
fn the_same_public_receipt_entry_can_filter_amulets() {
    let mut steps = steps();
    steps[1]["move"]["ref"]["kind"] = json!("amulet");
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"follower","card":"unit-follower"},{"id":"amulet","card":"unit-amulet"}
    ]);
    let script =
        json!({"random_selections":[{"n":1_u8,"from":"P1.hand","result":["follower","amulet"]}]});
    let mut game = configured(&steps, &initial, &script);
    resolved(
        game.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["field"], json!(["source", "amulet"]));
    assert_eq!(after["P1"]["hand"], json!(["follower"]));
}

#[test]
fn empty_short_and_nonfollower_samples_enter_only_possible_members() {
    for kinds in [
        vec![],
        vec!["unit-spell"],
        vec!["unit-follower"],
        vec!["unit-spell", "unit-spell"],
    ] {
        let hand = json!(
            kinds
                .iter()
                .enumerate()
                .map(|(i, kind)| json!({"id":format!("h{i}"),"card":kind}))
                .collect::<Vec<_>>()
        );
        let sample = json!(
            (0..kinds.len())
                .map(|i| format!("h{i}"))
                .collect::<Vec<_>>()
        );
        let mut game = loaded(1, &hand, 1, &sample);
        resolved(
            game.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        let after = game.projection(View::Referee).unwrap();
        let follower = kinds == ["unit-follower"];
        assert_eq!(
            after["P1"]["field"].as_array().unwrap().len(),
            if follower { 2 } else { 1 }
        );
        assert_eq!(
            after["P1"]["hand"].as_array().unwrap().len(),
            if follower { 0 } else { kinds.len() }
        );
    }
}

#[test]
fn a_public_receipt_does_not_follow_a_card_that_left_its_revealed_generation() {
    let mut steps = steps();
    steps
        .as_array_mut()
        .unwrap()
        .insert(1, json!({"discard":{"who":"self","all":true}}));
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"h","card":"unit-follower"}]);
    let script = json!({"random_selections":[{"n":1_u8,"from":"P1.hand","result":["h"]}]});
    let mut game = configured(&steps, &initial, &script);
    resolved(
        game.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["field"], json!(["source"]));
    assert_eq!(after["P1"]["cemetery"], json!(["h"]));
}

#[test]
fn revealing_an_opponents_hand_does_not_authorize_putting_it_on_our_field() {
    let mut steps = steps();
    steps[0]["reveal"]["ref"]["who"] = json!("opp");
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"foreign","card":"unit-follower"}]);
    let script = json!({"random_selections":[{"n":1_u8,"from":"P2.hand","result":["foreign"]}]});
    let mut game = configured(&steps, &initial, &script);
    let before = exact_authority(&game);
    for _ in 0..2 {
        let error = game
            .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
            .unwrap_err();
        assert!(matches!(error, EngineFailure::Unsupported(_)));
        assert_eq!(
            exact_authority(&game),
            before,
            "foreign ownership rejects the entry and rolls back publication and RNG"
        );
    }
}

#[test]
fn only_the_successfully_revealed_followers_enter_from_a_mixed_hand() {
    for seat in [1_u32, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let observer = if seat == 1 { View::P2 } else { View::P1 };
        let hand = json!([
            {"id":"shown-spell","card":"unit-spell"},
            {"id":"shown-follower","card":"unit-follower"},
            {"id":"private-follower","card":"unit-follower"}
        ]);
        let mut game = loaded(seat, &hand, 1, &json!(["shown-spell", "shown-follower"]));
        let result = resolved(
            game.activate_internal_card("source", AbilityId(1), PlayerId { seat })
                .unwrap(),
        );
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after[actor]["field"], json!(["source", "shown-follower"]));
        assert_eq!(
            after[actor]["hand"],
            json!(["shown-spell", "private-follower"])
        );
        let public = game.projection(observer).unwrap();
        assert_eq!(public["objects"]["shown-follower"]["card"], "unit-follower");
        assert!(public["objects"]["shown-spell"].is_null());
        assert!(public["objects"]["private-follower"].is_null());
        assert!(result.progress.receipts.values().any(|outcome| matches!(outcome, InstructionOutcome::Move(movement) if movement.moved.len()==1 && movement.moved[0].after.is_some())));
    }
}
