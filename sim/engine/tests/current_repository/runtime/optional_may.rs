#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the card scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn begin(number: &str, seat: &str, empty: bool) -> (Game, DecisionView) {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!(seat);
    if !empty {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":"first","card":"BP01-037"},
            {"id":"second","card":"BP01-037"}
        ]);
    }
    let evolution = number == "BP09-047";
    initial["players"][seat]["zones"][if evolution { "field" } else { "hand" }] =
        json!([{"id":"source","card":if evolution { "BP09-046" } else { number }}]);
    if evolution {
        initial["players"][seat]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
    }
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "optional-may",
    )
    .unwrap();
    if evolution {
        assert_eq!(
            game.internal_rule_action(&json!({
                "do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}
            }))
            .unwrap()
            .outcome,
            "resolved",
            "the real printed evolution enters before its trigger"
        );
    } else {
        assert!(
            matches!(
                game.play_internal_card(
                    "source",
                    PlayerId {
                        seat: seat_number(seat)
                    }
                )
                .unwrap(),
                CardPlay::Entered(_)
            ),
            "the real follower enters before its fanfare"
        );
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = pending.domain else {
        panic!("printed trigger")
    };
    assert_eq!(options.len(), 1, "exactly the printed trigger is pending");
    assert_eq!(
        options[0].card, number,
        "the pending trigger belongs to this printing"
    );
    let CardPlay::Awaiting(choice) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("optional body")
    };
    assert!(
        matches!(
            choice.domain,
            InputDomain::Integer(Cardinality {
                minimum: Nat(0),
                maximum: Some(Nat(1))
            })
        ),
        "the May choice accepts only decline or acceptance"
    );
    (game, choice)
}

fn observable_state(game: &Game) -> (Value, Option<DecisionView>, Option<ResumeStage>) {
    (
        game.projection(View::Referee).unwrap(),
        game.internal_decision(View::Referee),
        game.internal_card_stage(),
    )
}

fn answer(game: &mut Game, choice: &DecisionView, accept: bool) {
    let before = observable_state(game);
    for (actor, value) in [
        (
            PlayerId {
                seat: if choice.actor.seat == 1 { 2 } else { 1 },
            },
            Nat(1),
        ),
        (choice.actor, Nat(2)),
    ] {
        game.answer_internal_card(actor, choice.token, Answer::Integer(value))
            .unwrap_err();
        assert!(
            observable_state(game) == before,
            "invalid optional answer preserves state"
        );
    }
    resolved(
        game.answer_internal_card(
            choice.actor,
            choice.token,
            Answer::Integer(Nat(u64::from(accept))),
        )
        .unwrap(),
    );
    let after = observable_state(game);
    game.answer_internal_card(choice.actor, choice.token, Answer::Integer(Nat(1)))
        .unwrap_err();
    assert!(
        observable_state(game) == after,
        "stale optional answer preserves state"
    );
}

fn mill(number: &str) {
    for seat in ["P1", "P2"] {
        for empty in [false, true] {
            for accept in [false, true] {
                let (mut game, choice) = begin(number, seat, empty);
                let other = if seat == "P1" { View::P2 } else { View::P1 };
                assert!(
                    game.internal_decision(other).is_none(),
                    "only the controller answers"
                );
                let public = game.projection(other).unwrap();
                assert!(
                    !public.to_string().contains("\"first\""),
                    "looking does not expose a private deck identity"
                );
                answer(&mut game, &choice, accept);
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(
                    state[seat]["cemetery"],
                    if accept && !empty {
                        json!(["first"])
                    } else {
                        json!([])
                    },
                    "only an accepted nonempty observation moves to cemetery"
                );
                assert_eq!(
                    projected_zone_count(&state[seat]["deck"]),
                    if empty {
                        0_u64
                    } else if accept {
                        1
                    } else {
                        2
                    },
                    "declining preserves the original deck while acceptance removes its top"
                );
                assert_eq!(
                    state[seat]["hand"],
                    json!([]),
                    "optional mill does not draw"
                );
            }
        }
    }
}

#[test]
fn bp09_046_can_decline_or_mill_the_observed_top_card() {
    mill("BP09-046");
}

#[test]
fn sd03_010_can_decline_or_mill_the_observed_top_card() {
    mill("SD03-010");
}

#[test]
fn bp09_047_draws_after_accepting_or_declining_the_optional_mill() {
    for seat in ["P1", "P2"] {
        for accept in [false, true] {
            let (mut game, choice) = begin("BP09-047", seat, false);
            answer(&mut game, &choice, accept);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state[seat]["hand"],
                if accept {
                    json!(["second"])
                } else {
                    json!(["first"])
                }
            );
            assert_eq!(
                state[seat]["cemetery"],
                if accept { json!(["first"]) } else { json!([]) }
            );
        }
    }
}

#[test]
fn bp09_047_still_attempts_the_mandatory_draw_after_looking_at_an_empty_deck() {
    for seat in ["P1", "P2"] {
        for accept in [false, true] {
            let (mut game, choice) = begin("BP09-047", seat, true);
            answer(&mut game, &choice, accept);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state[seat]["hand"], json!([]));
            assert_eq!(state[seat]["cemetery"], json!([]));
            assert_ne!(state["game"]["ended"], true);
            game.resume_internal_card().unwrap();
            assert_eq!(
                game.projection(View::Referee).unwrap()["game"]["ended"],
                true
            );
        }
    }
}
