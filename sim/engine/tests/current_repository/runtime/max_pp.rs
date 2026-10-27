use super::*;
use sve_engine::game::internal::{InstructionOutcome, ResourceOutcome};

#[expect(
    clippy::panic,
    reason = "A different pending domain invalidates this scenario."
)]
fn resolve_maximum(game: &mut Game, seat: &str, maximum: i64) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected the departed follower's Lastword");
    };
    assert_eq!(
        options.len(),
        1,
        "only the departed follower has a pending ability"
    );
    assert_eq!(
        options[0].card, "BP01-079",
        "the original printed source is retained"
    );
    assert_eq!(options[0].line, 2_u64, "only the Lastword is pending");
    let before = format!("{game:?}");
    game.answer_internal_card(
        PlayerId {
            seat: if question.actor.seat == 1 { 2 } else { 1 },
        },
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap_err();
    assert!(
        format!("{game:?}") == before,
        "wrong actor must not alter the pending ability"
    );
    let result = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    let actual = i64::from(maximum != 10);
    assert_eq!(
        result.source.controller.seat,
        seat_number(seat),
        "Lastword keeps the original controller"
    );
    assert_eq!(
        result.progress.receipts.len(),
        1,
        "one resource instruction completes"
    );
    assert_eq!(
        result.progress.receipts.values().next().unwrap(),
        &InstructionOutcome::Resource(ResourceOutcome {
            player: question.actor,
            resource: Resource::PpMaximum,
            requested: Int(1),
            before: Int(maximum),
            after: Int(maximum.saturating_add(actual)),
            actual: Int(actual),
        }),
        "the receipt distinguishes the printed request and the capped increase"
    );
    assert!(
        result.progress.facts.iter().all(|fact| matches!(
            fact,
            Fact::AbilityPlayed { .. } | Fact::AbilityResolved { .. }
        )),
        "the resource body adds no facts beyond the existing ability lifecycle"
    );
    let completed = format!("{game:?}");
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap_err();
    assert!(
        format!("{game:?}") == completed,
        "old pending token must not resolve twice"
    );
}

#[test]
fn bp01_079_lastword_increases_original_controllers_cap_without_recovering_pp() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        for maximum in [3_i64, 9, 10] {
            let mut initial = draw_position();
            initial["turn"]["active"] = json!(seat);
            initial["players"][seat]["pp"] = json!({"current":maximum,"max":maximum});
            initial["players"][opponent]["pp"] = json!({"current":1_u8,"max":4_u8});
            initial["players"][seat]["zones"]["field"] =
                json!([{"id":"aira","card":"BP01-079","owner":opponent}]);
            let mut game = with_producer(
                &initial,
                Action::Destroy {
                    objects: subjects(),
                },
            );
            assert!(game.internal_pending_choice().unwrap().is_none());
            activate(&mut game, seat);
            game.resume_internal_card().unwrap();
            resolve_maximum(&mut game, seat, maximum);
            for view in [View::P1, View::P2, View::Referee] {
                let state = game.projection(view).unwrap();
                assert_eq!(
                    state[seat]["pp"],
                    json!({"current":maximum,"max":maximum.saturating_add(1).min(10)})
                );
                assert_eq!(state[opponent]["pp"], json!({"current":1_u8,"max":4_u8}));
                assert!(
                    state[opponent]["cemetery"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("aira"))
                );
            }
        }
    }
}

#[test]
fn bp01_079_banish_does_not_trigger_a_maximum_pp_increase() {
    for seat in ["P1", "P2"] {
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(seat);
        initial["players"][seat]["pp"] = json!({"current":2_u8,"max":3_u8});
        initial["players"][seat]["zones"]["field"] = json!([{"id":"aira","card":"BP01-079"}]);
        let mut game = with_producer(
            &initial,
            Action::Banish {
                objects: subjects(),
            },
        );
        activate(&mut game, seat);
        game.resume_internal_card().unwrap();
        assert!(game.internal_pending_choice().unwrap().is_none());
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[seat]["pp"], json!({"current":2_u8,"max":3_u8}));
        assert!(
            state[seat]["banish"]
                .as_array()
                .unwrap()
                .contains(&json!("aira"))
        );
    }
}

#[test]
fn bp01_079_keeps_assail_against_an_active_enemy_follower() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"aira","card":"BP01-079"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":false}}]);
    let mut game = game(&initial);
    let attack = game
        .internal_rule_action(&json!({"do":"attack","attacker":"aira","target":"enemy"}))
        .unwrap();
    assert_eq!(attack.outcome, "resolved");
}
