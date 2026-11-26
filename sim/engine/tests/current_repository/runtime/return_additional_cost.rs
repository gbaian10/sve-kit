use super::*;

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"},
        {"id":"second","card":"BP01-171"},
        {"id":"third","card":"BP01-171"},
        {"id":"fourth","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn triggers_with(game: &mut Game, mode: u64) {
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
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish_with(game, step, mode);
    }
}

fn play_with(game: &mut Game, id: &str, mode: u64) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish_with(game, step, mode);
    triggers_with(game, mode);
}

fn play(game: &mut Game, id: &str) {
    play_with(game, id, 1);
}

#[test]
fn bp19_018_returns_an_own_card_to_deal_six_and_two() {
    let mut initial = position("BP19-018");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"goblin","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-152"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["goblin"]));
    assert_eq!(state["objects"]["enemy"]["damage"], 6_u8);
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
}
