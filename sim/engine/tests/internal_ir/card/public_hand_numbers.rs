#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the public count scenarios."
)]
use super::dynamic_declarations::lower_body;
use super::*;
use sve_engine::game::internal::{DecisionView, InputDomain, SearchAnswer};

fn hand() -> Value {
    json!({"count":{"who":"self","zone":"hand"}})
}

fn target() -> Value {
    json!({"targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},
        "upto":{"plus":[hand(),{"count":{"who":"self","zone":"ex"}}]}}],
        "do":[{"damage":{"to":"enemy","n":1}}]})
}

fn entered(body: &Value, seat: u32, hidden: &[&str]) -> (Game, DecisionView) {
    let program = lower_body(body).unwrap();
    let (actor, opponent) = if seat == 1 {
        ("P1", "P2")
    } else {
        ("P2", "P1")
    };
    let mut initial = setup(
        seat,
        json!([
            {"id":"deck-a","card":"unit-follower"}, {"id":"deck-b","card":"unit-follower"}
        ]),
    );
    initial["turn"]["active"] = json!(actor);
    let mut hand = vec![json!({"id":"source","card":"unit-follower"})];
    hand.extend(
        hidden
            .iter()
            .enumerate()
            .map(|(i, card)| json!({"id":format!("hidden-{i}"),"card":card})),
    );
    initial["players"][actor]["zones"]["hand"] = json!(hand);
    initial["players"][actor]["zones"]["field"] = json!([]);
    initial["players"][actor]["zones"]["ex"] = json!([{"id":"extra","card":"unit-spell"}]);
    initial["players"][opponent]["zones"]["field"] = json!(
        (0..5)
            .map(|i| json!({"id":format!("enemy-{i}"),"card":"unit-follower"}))
            .collect::<Vec<_>>()
    );
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "public-hand-count",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    for card in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(card, &super::card(vec![])).unwrap();
    }
    game.play_internal_card("source", PlayerId { seat })
        .unwrap();
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("fanfare")
    };
    let CardPlay::Awaiting(question) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("public quantity question")
    };
    (game, question)
}

#[test]
fn target_limit_counts_hand_and_ex_after_the_source_has_left_the_hand() {
    for hidden in [vec![], vec!["unit-spell", "unit-amulet"]] {
        let (mut game, question) = entered(&target(), 1, &hidden);
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("targets")
        };
        let maximum = u64::try_from(hidden.len()).unwrap() + 1;
        assert_eq!(
            domain.cardinality,
            Cardinality {
                minimum: Nat(0),
                maximum: Some(Nat(maximum))
            }
        );
        let before = exact_authority(&game);
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets((0..=maximum).collect()),
        )
        .unwrap_err();
        assert_eq!(exact_authority(&game), before);
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(
                    domain
                        .members
                        .iter()
                        .take(hidden.len() + 1)
                        .map(|m| m.handle)
                        .collect(),
                ),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        for i in 0..5 {
            assert_eq!(
                state["objects"][format!("enemy-{i}")]["damage"],
                u64::from(i < hidden.len() + 1)
            );
        }
    }
}

#[test]
fn equal_hand_sizes_give_identical_public_questions_for_different_hidden_faces_both_seats() {
    for seat in [1, 2] {
        let mut questions = Vec::new();
        for hidden in [["unit-spell", "unit-spell"], ["unit-amulet", "unit-spell"]] {
            let (game, _) = entered(&target(), seat, &hidden);
            let (own, other) = if seat == 1 {
                (View::P1, View::P2)
            } else {
                (View::P2, View::P1)
            };
            questions.push(game.projection(own).unwrap()["awaiting"].clone());
            let opponent = game.projection(other).unwrap();
            for id in ["hidden-0", "hidden-1"] {
                assert!(opponent["objects"].get(id).is_none());
            }
        }
        assert_eq!(questions[0], questions[1]);
        assert!(!questions[0].is_null());
    }
}

#[test]
fn private_search_compares_printed_cost_with_only_the_public_hand_size() {
    let body = json!({"do":[{"search":{"sel":{"who":"self","zone":"deck","kind":"follower",
        "orig_cost_le":hand()},"n":1,"to":"hand"}}]});
    let (mut game, question) = entered(&body, 1, &["unit-spell"]);
    let InputDomain::Search(domain) = &question.domain else {
        panic!("search")
    };
    assert_eq!(domain.cards.len(), 2);
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 2);
    assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 1);
}

#[test]
fn mode_limit_counts_only_complete_pairs_in_the_current_hand() {
    let body = json!({"do":[{"choice":{"upto":{"per":{"of":hand(),"unit":2}},
        "options":[[{"life":{"who":"self","delta":1}}],
            [{"life":{"who":"self","delta":2}}], [{"life":{"who":"self","delta":4}}]]}}]});
    for (size, maximum) in [(3_usize, 1), (4, 2)] {
        let hidden = vec!["unit-spell"; size];
        let (mut game, question) = entered(&body, 1, &hidden);
        let InputDomain::Modes { cardinality, .. } = &question.domain else {
            panic!("modes")
        };
        assert_eq!(cardinality.maximum, Some(Nat(maximum)));
        let before = exact_authority(&game);
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Modes(vec![ModeId(1), ModeId(2), ModeId(3)]),
        )
        .unwrap_err();
        assert_eq!(exact_authority(&game), before);
        let modes = if size == 3 {
            vec![ModeId(1)]
        } else {
            vec![ModeId(1), ModeId(2)]
        };
        resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Modes(modes))
                .unwrap(),
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            if size == 3 { 21 } else { 23 }
        );
    }
}
