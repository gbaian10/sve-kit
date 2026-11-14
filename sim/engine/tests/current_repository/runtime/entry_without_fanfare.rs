#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real-card scenarios."
)]
use super::*;

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "entry-without-fanfare",
    )
    .unwrap()
}

/// Takes the first `picks` offered targets and resolves every pending ability in order.
fn settle(game: &mut Game, mut step: Option<CardPlay>, picks: usize) {
    for _ in 0_u8..32_u8 {
        if let Some(CardPlay::Awaiting(q)) = step.take() {
            let answer = match &q.domain {
                InputDomain::Targets(d) => {
                    Answer::Targets(d.members.iter().map(|m| m.handle).take(picks).collect())
                }
                InputDomain::Pending(options) => Answer::Pending(options[0].handle),
                InputDomain::Quick { .. } => Answer::Quick(None),
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                _ => panic!("unexpected decision: {:?}", q.domain),
            };
            step = Some(game.answer_internal_card(q.actor, q.token, answer).unwrap());
            continue;
        }
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
            continue;
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("expected a pending ability");
        };
        step = Some(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
    }
    panic!("scenario did not settle");
}

#[test]
fn bp04_038_puts_a_hand_follower_whose_fanfare_does_not_trigger() {
    // BP17-051's Fanfare would draw and banish; Ginger's entry suppresses it.
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP04-038"},
        {"id":"put","card":"BP17-051"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"BP01-037"}]);
    let mut game = game(&initial);
    let step = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    settle(&mut game, Some(step), 1);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"], json!(["source", "put"]));
    assert_eq!(
        view["P1"]["deck"],
        json!(["top"]),
        "no draw from the put card"
    );
    assert_eq!(view["P1"]["banish"], json!([]));
}

#[test]
fn bp04_039_puts_any_followers_without_fanfare_and_a_later_play_still_triggers() {
    // Every card of the same entry loses its own Fanfare; a card played afterwards keeps its own.
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP04-038"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP04-039"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"a","card":"BP17-051"},
        {"id":"b","card":"BP17-051"},
        {"id":"c","card":"BP17-051"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    settle(&mut game, None, 2);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["evolve_deck"], json!([]));
    assert_eq!(
        view["P1"]["field"].as_array().unwrap()[1..],
        [json!("a"), json!("b")]
    );
    assert_eq!(view["P1"]["deck"], json!(["top"]), "neither put card draws");
    let step = game.play_internal_card("c", PlayerId { seat: 1 }).unwrap();
    settle(&mut game, Some(step), 1);
    let played = game.projection(View::Referee).unwrap();
    assert_eq!(
        played["P1"]["field"].as_array().unwrap()[1..],
        [json!("a"), json!("b"), json!("c")]
    );
    assert_eq!(
        played["P1"]["deck"],
        json!([]),
        "the played card's Fanfare draws"
    );
    assert_eq!(played["P1"]["banish"], json!(["top"]));
}
