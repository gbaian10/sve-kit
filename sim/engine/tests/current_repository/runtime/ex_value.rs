#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
//! Numeric modifications of EX followers, carried onto the field when played (CR 4.8.3.3).
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ex-value",
    )
    .unwrap()
}

fn finish(game: &mut Game, mut step: CardPlay) {
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
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

fn stats(state: &Value, id: &str) -> (u64, u64) {
    (
        state["objects"][id]["power"].as_u64().unwrap(),
        state["objects"][id]["hp"].as_u64().unwrap(),
    )
}

#[test]
fn bp06_003_buffs_ex_fairies_and_the_buff_survives_playing_one_onto_the_field() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP06-003"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"waiting","card":"BP03-T01"},
        {"id":"other","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    let ex = state["P1"]["ex"].as_array().unwrap().clone();
    assert_eq!(ex.len(), 3, "the fanfare places one more fairy in EX");
    assert_eq!(stats(&state, "waiting"), (2, 2), "an EX fairy follower gains +1/+1");
    assert_eq!(stats(&state, "other"), (2, 2), "a non-fairy EX follower is unchanged");
    for id in &ex {
        let id = id.as_str().unwrap();
        if id != "other" {
            assert_eq!(stats(&state, id), (2, 2), "both fairies in EX gain +1/+1");
        }
    }

    play(&mut game, "waiting");
    let played = game.projection(View::Referee).unwrap();
    assert!(
        played["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("waiting")),
        "the buffed fairy is played from EX"
    );
    assert_eq!(stats(&played, "waiting"), (2, 2), "the EX buff stays on the field (CR 4.8.3.3)");
}

#[test]
fn bp01_011_buffs_other_field_fairies_and_ex_fairies_but_not_itself() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP01-011"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"ally","card":"BP03-T01"},
        {"id":"goblin","card":"BP01-171"}
    ]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"waiting","card":"BP03-T01"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP03-T01"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(stats(&state, "ally"), (2, 2));
    assert_eq!(stats(&state, "waiting"), (2, 2));
    assert_eq!(stats(&state, "source"), (2, 3), "the dancer itself is excluded");
    assert_eq!(stats(&state, "goblin"), (2, 2), "a non-fairy is unchanged");
    assert_eq!(stats(&state, "enemy"), (1, 1), "the opponent's fairy is unchanged");
}
