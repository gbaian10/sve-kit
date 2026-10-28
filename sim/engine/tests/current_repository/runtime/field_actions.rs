#![expect(
    clippy::panic,
    reason = "A missing target question fails the scenario."
)]

use super::*;

fn opposite(seat: &str) -> &str {
    if seat == "P1" { "P2" } else { "P1" }
}

fn position(seat: &str, number: &str) -> Value {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!(seat);
    initial["players"][seat]["zones"]["hand"] = json!([{"id":"spell","card":number}]);
    initial
}

fn target(game: &mut Game, seat: &str) -> Activation {
    let CardPlay::Awaiting(question) = game
        .play_internal_card(
            "spell",
            PlayerId {
                seat: seat_number(seat),
            },
        )
        .unwrap()
    else {
        panic!("one target is required");
    };
    let before = format!("{game:?}");
    game.answer_internal_card(
        PlayerId {
            seat: seat_number(opposite(seat)),
        },
        question.token,
        Answer::Targets(vec![0]),
    )
    .unwrap_err();
    assert!(format!("{game:?}") == before, "another actor cannot answer");
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap(),
    )
}

#[test]
fn bp06_072_adds_only_selected_followers_power() {
    for seat in ["P1", "P2"] {
        let mut initial = position(seat, "BP06-072");
        let mut no_target = game(&initial);
        let before = format!("{no_target:?}");
        no_target
            .play_internal_card(
                "spell",
                PlayerId {
                    seat: seat_number(seat),
                },
            )
            .unwrap_err();
        assert!(
            format!("{no_target:?}") == before,
            "no target rejects before payment"
        );
        initial["players"][seat]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
        let mut game = game(&initial);
        target(&mut game, seat);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["ally"]["power"], 7_i64);
        assert_eq!(state["objects"]["ally"]["hp"], 5_i64);
    }
}

#[test]
fn bp01_169_destroys_selected_opposing_field_card() {
    for seat in ["P1", "P2"] {
        let opponent = opposite(seat);
        let mut initial = position(seat, "BP01-169");
        initial["players"][seat]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
        initial["players"][opponent]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        let result = target(&mut game, seat);
        assert!(
            result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Movement(m) if m.method == MoveMethod::Destroy))
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[opponent]["cemetery"], json!(["enemy"]));
        assert_eq!(state[seat]["field"], json!(["ally"]));
    }
}

#[test]
fn bp01_015_returns_to_owner_then_draws_without_exposing_hidden_generation() {
    for (seat, target_card) in [
        ("P1", "BP01-037"),
        ("P2", "BP01-037"),
        ("P1", "BP19-108"),
        ("P2", "BP19-108"),
    ] {
        let opponent = opposite(seat);
        let mut initial = position(seat, "BP01-015");
        initial["players"][seat]["zones"]["field"] =
            json!([{"id":"borrowed","card":target_card,"owner":opponent}]);
        initial["players"][seat]["zones"]["deck"] = json!([{"id":"drawn","card":"CP02-068"}]);
        let mut game = game(&initial);
        target(&mut game, seat);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[opponent]["hand"], json!(["borrowed"]));
        assert_eq!(state[seat]["hand"], json!(["drawn"]));
        let view = if seat == "P1" { View::P1 } else { View::P2 };
        let packet = game.projection(view).unwrap();
        assert!(
            packet["objects"].get("borrowed").is_none(),
            "the caster cannot track the returned hidden object"
        );
        assert_eq!(
            packet[opponent]["hand"],
            json!([{"filler":1_u8}]),
            "the hidden destination carries only a count"
        );
        let encoded = packet.to_string();
        assert!(
            !encoded.contains("receipts"),
            "authority movement receipts remain private"
        );
    }
}

#[test]
fn bp01_180_damages_all_opposing_followers_and_excludes_own_field() {
    for seat in ["P1", "P2"] {
        let opponent = opposite(seat);
        let mut initial = position(seat, "BP01-180");
        initial["players"][seat]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
        initial["players"][opponent]["zones"]["field"] = json!([
            {"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"}
        ]);
        let mut game = game(&initial);
        let result = resolved(
            game.play_internal_card(
                "spell",
                PlayerId {
                    seat: seat_number(seat),
                },
            )
            .unwrap(),
        );
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::Damage(_)))
                .count(),
            2
        );
        let state = game.projection(View::Referee).unwrap();
        for id in ["first", "second"] {
            assert_eq!(state["objects"][id]["damage"], 1_u8);
            assert_eq!(state["objects"][id]["hp"], 4_i64);
        }
        assert_eq!(state["objects"]["ally"]["damage"], 0_u8);
        assert_eq!(state[opponent]["field"].as_array().unwrap().len(), 2);
        assert_eq!(state[opponent]["leader"]["life"], 20_u8);
    }
}
