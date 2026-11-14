#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed live-history discount scenario."
)]
use super::*;

fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn resolve_next(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed pending");
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

#[test]
fn bp18_004_whole_card_draws_for_own_ex_entry_then_discounts_eve_from_live_plays() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"tia","card":"BP18-004"},{"id":"other","card":"BP01-009"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"draw-1","card":"BP01-009"},{"id":"draw-2","card":"BP01-009"}
    ]);
    let mut game = game(&initial);
    assert!(
        matches!(
            game.play_internal_card("tia", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "Tia enters normally"
    );
    finish(&mut game);
    resolve_next(&mut game);
    resolve_next(&mut game);
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(
        before["P1"]["hand"],
        json!(["other", "draw-1"]),
        "the native own-turn EX observer draws once"
    );
    let eve = before["P1"]["ex"][0].as_str().unwrap().to_owned();
    let definition = selection().current["BP18-004"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == 3)
        .unwrap()
        .node
        .id;
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("tia", definition, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("selected Eve");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("EX target");
    };
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    finish(&mut game);
    assert!(
        matches!(
            game.play_internal_card("other", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "another play updates the live history"
    );
    finish(&mut game);
    assert!(
        matches!(
            game.play_internal_card(&eve, PlayerId { seat: 1 }).unwrap(),
            CardPlay::Entered(_)
        ),
        "the selected token retains its live discount"
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["pp"]["current"], 3_u8,
        "Tia costs two, the other card three, and Eve costs four minus two prior plays"
    );
    assert_eq!(
        view["objects"]["tia"]["acted"], true,
        "the native activation pays Act"
    );
    assert_eq!(
        view["objects"][&eve]["name"], "クリスタリア・イヴ",
        "the whole native token is played"
    );
}
