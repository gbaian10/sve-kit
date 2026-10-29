#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate this real-card scenario."
)]

use super::*;
use sve_engine::game::internal::DecisionView;

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "token-discard-binding",
    )
    .unwrap()
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected the printed ability's next decision");
    };
    question
}

fn position(print: &str, occupied: usize) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":print}, {"id":"held","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"}, {"id":"second","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["ex"] = json!(
        (0..occupied)
            .map(|index| json!({"id":format!("old-{index}"),"card":"BP01-T03"}))
            .collect::<Vec<_>>()
    );
    initial
}

fn fanfare_discard(game: &mut Game) -> DecisionView {
    let entry = awaiting(game.play_internal_card("source", PlayerId { seat: 1 }).unwrap());
    assert!(
        matches!(entry.domain, InputDomain::WardEntry { .. }),
        "the real follower first offers its Ward entry posture"
    );
    game.answer_internal_card(entry.actor, entry.token, Answer::WardEntry(false))
        .unwrap();
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("expected the printed fanfare to be pending");
    };
    assert_eq!(options.len(), 1, "only the fanfare is pending");
    awaiting(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    )
}

#[test]
fn bp22_015_and_p06_create_then_draw_before_their_private_discard() {
    for print in ["BP22-015", "BP22-P06"] {
        for occupied in [0_usize, 5] {
            let mut game = game(&position(print, occupied));
            let discard = fanfare_discard(&mut game);
            let InputDomain::Targets(domain) = &discard.domain else {
                panic!("expected the controller's private hand selection");
            };
            assert_eq!(domain.members.len(), 2, "the newly drawn card is selectable");
            assert!(
                game.internal_decision(View::P2).is_none(),
                "the opponent cannot see private discard handles"
            );
            let before = game.projection(View::Referee).unwrap();
            assert_eq!(before["P1"]["ex"].as_array().unwrap().len(), (occupied + 2).min(5));
            assert_eq!(before["P2"]["ex"], json!([]));
            assert_eq!(before["P1"]["hand"], json!(["held", "first"]));
            assert_eq!(before["P1"]["deck"], json!(["second"]));
            assert_eq!(before["P1"]["cemetery"], json!([]));
            assert!(
                game.answer_internal_card(discard.actor, discard.token, Answer::Targets(vec![]))
                    .is_err(),
                "an available required discard cannot be omitted"
            );
            assert_eq!(
                game.projection(View::Referee).unwrap(),
                before,
                "a rejected answer preserves the paid play and resolved earlier steps"
            );
            let completed = game
                .answer_internal_card(
                    discard.actor,
                    discard.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap();
            assert!(matches!(completed, CardPlay::Resolved(_)), "the fanfare completes");
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["P1"]["hand"].as_array().unwrap().len(), 1);
            assert_eq!(after["P1"]["cemetery"].as_array().unwrap().len(), 1);
            assert_eq!(after["P1"]["deck"], json!(["second"]));
            assert_eq!(after["P1"]["ex"], before["P1"]["ex"]);
            assert_eq!(after["P1"]["pp"]["current"], 8_u8);
            assert_eq!(after["P1"]["field"], json!(["source"]));
        }
    }
}

#[test]
fn bp22_015_and_p06_keep_their_printed_ward() {
    for print in ["BP22-015", "BP22-P06"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"attacker","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"ward","card":print,"state":{"acted":true}}]);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"attacker","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "cannot-attack",
            "the rested Ward protects its controller's leader"
        );
    }
}
