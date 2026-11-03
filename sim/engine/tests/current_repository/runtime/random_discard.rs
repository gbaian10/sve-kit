#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn game(initial: &Value, random: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        random,
        "random-discard",
    )
    .unwrap()
}

fn position(opponent_hand: &[&str]) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"d1","card":"BP01-171"},{"id":"d2","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["hand"] = Value::Array(
        opponent_hand
            .iter()
            .map(|id| json!({"id":id,"card":"BP01-171"}))
            .collect(),
    );
    initial
}

fn finish(game: &mut Game, mut step: CardPlay) {
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Modes { modes, .. } => Answer::Modes(vec![*modes.last().unwrap()]),
            InputDomain::Targets(domain) => {
                Answer::Targets(domain.members.iter().take(1).map(|m| m.handle).collect())
            }
            other => panic!("unexpected decision: {other:?}"),
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn triggers(game: &mut Game) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending ability expected")
        };
        let step = game
            .answer_internal_card(question.actor, question.token, Answer::Pending(options[0].handle))
            .unwrap();
        finish(game, step);
    }
}

fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish(game, step);
    triggers(game);
}

#[test]
fn bp02_074_discards_the_scripted_random_card_from_the_opponents_hand() {
    let mut initial = position(&["a", "b", "c"]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP02-074"}]);
    let script = json!({"random_selections":[{"n":1_u8,"from":"P2.hand","result":["b"]}]});
    let mut scripted = game(&initial, &script);
    play(&mut scripted, "source");
    let state = scripted.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["hand"], json!(["a", "c"]));
    assert_eq!(state["P2"]["cemetery"], json!(["b"]), "the random card is discarded");

    // Without a script the shared random stream still discards exactly one card.
    let mut seeded = game(&initial, &Value::Null);
    play(&mut seeded, "source");
    let streamed = seeded.projection(View::Referee).unwrap();
    assert_eq!(streamed["P2"]["hand"].as_array().unwrap().len(), 2);
    assert_eq!(streamed["P2"]["cemetery"].as_array().unwrap().len(), 1);

    let mut empty_hand = position(&[]);
    empty_hand["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP02-074"}]);
    let mut empty = game(&empty_hand, &Value::Null);
    play(&mut empty, "source");
    assert_eq!(
        empty.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!([]),
        "an empty hand discards nothing"
    );
}

#[test]
fn bp15_114_places_the_nectar_whose_play_discards_two_random_cards() {
    let mut initial = position(&["a", "b", "c"]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"base","card":"BP15-113"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP15-114"}]);
    let script = json!({"random_selections":[{"n":1_u8,"from":"P2.hand","result":["c","a"]}]});
    let mut game = game(&initial, &script);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"base","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the base pays its printed evolution cost"
    );
    triggers(&mut game);
    let ex = game.projection(View::Referee).unwrap()["P1"]["ex"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(ex.len(), 1, "mode two places one nectar token in EX");
    play(&mut game, ex[0].as_str().unwrap());
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
    assert_eq!(state["P1"]["hand"], json!(["d1", "d2"]));
    assert_eq!(state["P2"]["hand"], json!(["b"]));
    assert_eq!(state["P2"]["cemetery"], json!(["c", "a"]), "two random cards are discarded");
}

#[test]
fn bp02_074_refuses_to_draw_around_anonymous_hand_cards() {
    for script in [
        Value::Null,
        json!({"random_selections":[{"n":1_u8,"from":"P2.hand","result":["h0"]}]}),
    ] {
        let mut initial = position(&["h0"]);
        initial["players"]["P2"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"filler":2_u8}));
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP02-074"}]);
        let mut game = game(&initial, &script);
        let step = game.play_internal_card("source", actor()).unwrap();
        finish(&mut game, step);
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending fanfare expected")
        };
        let before = game.projection(View::Referee).unwrap();
        assert!(
            game.answer_internal_card(question.actor, question.token, Answer::Pending(options[0].handle))
                .is_err(),
            "the anonymous cards stay in the random population"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            before,
            "the refused discard leaves the whole position unchanged"
        );
    }
}
