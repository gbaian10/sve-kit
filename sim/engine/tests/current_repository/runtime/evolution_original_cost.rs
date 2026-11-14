#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed Plumeria scenario."
)]
use super::*;
fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn pending(game: &mut Game) {
    finish(game);
    while let Some(question) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("printed trigger");
        };
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        finish(game);
    }
}
#[test]
fn bp20_004_whole_card_draws_and_grants_storm_to_the_real_plant_evolution() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"draw-a","card":"BP01-013"},{"id":"draw-b","card":"BP01-013"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"plumeria","card":"BP20-004"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"plant","card":"BP05-014"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0_u8..3)
            .map(|n| json!({"id":format!("plant-{n}"),"card":"BP05-014"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP05-015"}]);
    let mut game = game(&initial);
    assert!(
        matches!(
            game.play_internal_card("plumeria", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the whole printed card is admitted"
    );
    pending(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "three cemetery plants enable the printed draw"
    );
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"plant","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the original-cost-two plant evolves through its own definition"
    );
    pending(&mut game);
    assert!(
        game.projection(View::Referee).unwrap()["objects"]["plant"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("疾走")),
        "the captured subject receives Storm"
    );
}
