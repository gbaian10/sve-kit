use super::*;

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "end-phase-pp",
    )
    .unwrap()
}
fn resolve(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("unexpected printed input: {:?}", question.domain);
        };
        let handles = domain
            .members
            .iter()
            .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
            .map(|member| member.handle)
            .collect();
        play = game
            .answer_internal_card(question.actor, question.token, Answer::Targets(handles))
            .unwrap();
    }
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn triggers(game: &mut Game) {
    while let Some(question) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending printed trigger");
        };
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        resolve(game, play);
    }
}
#[test]
fn bp06_103_end_phase_requires_two_remaining_pp() {
    for pp in [1_u8, 2] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP06-103"}]);
        initial["players"]["P1"]["pp"] = json!({"current":pp,"max":10_i64});
        let mut game = game(&initial);
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        triggers(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            if pp == 2 { 19_i64 } else { 20_i64 },
            "bp06 103 end phase requires two remaining pp"
        );
    }
}
