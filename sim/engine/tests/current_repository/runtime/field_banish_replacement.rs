use super::*;

fn initial(silenced: bool, target: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"replacement","card":"BP18-061","state":{"silenced":silenced}}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"departing","card":target}
    ]);
    initial
}

fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

#[test]
fn bp18_061_whole_card_ward_entry_and_fanfare_use_the_replacement() {
    let mut initial = initial(false, "BP01-P25");
    initial["players"]["P1"]["zones"]["hand"] = initial["players"]["P1"]["zones"]["field"].take();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    let mut game = game(&initial);
    let mut play = game
        .play_internal_card("replacement", PlayerId { seat: 1 })
        .unwrap();
    while let CardPlay::Awaiting(question) = play {
        assert!(matches!(question.domain, InputDomain::WardEntry { .. }));
        play = game
            .answer_internal_card(question.actor, question.token, Answer::WardEntry(true))
            .unwrap();
    }
    assert!(matches!(play, CardPlay::Entered(_)));
    finish(&mut game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("fanfare pending");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "BP18-061");
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    finish(&mut game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["pp"]["current"], 2_u8);
    assert_eq!(view["P2"]["leader"]["life"], 16_u8);
    assert_eq!(view["P2"]["banish"], json!(["departing"]));
    assert!(game.internal_pending_choice().unwrap().is_none());
}
