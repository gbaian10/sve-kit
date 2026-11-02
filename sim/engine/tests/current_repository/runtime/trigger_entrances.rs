use super::*;

#[test]
fn bp04_104_quick_amulet_pays_from_the_field_and_resolves_after_sacrifice() {
    let mut initial = draw_position();
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"globe","card":"BP04-104"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    let mut board = game(&initial);
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let question = board.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &question.domain else {
        panic!("Quick");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "BP04-104");
    let before = board.projection(View::Referee).unwrap();
    assert_eq!(before["P2"]["pp"]["current"], 10_u8);
    assert_eq!(before["objects"]["globe"]["acted"], false);
    resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Quick(Some(options[0].handle)),
            )
            .unwrap(),
    );
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["pp"]["current"], 8_u8);
    assert_eq!(state["P2"]["leader"]["life"], 21_u8);
    assert_eq!(state["P2"]["cemetery"], json!(["globe"]));
    assert_eq!(state["P2"]["hand"], json!(["drawn"]));
}

#[test]
fn bp10_043_end_phase_uses_the_ex_source_instead_of_playing_the_spell() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"pressure","card":"BP10-043"}]);
    let mut board = game(&initial);
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "BP10-043");
    resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
    );
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
    assert_eq!(state["P1"]["ex"], json!(["pressure"]));
    assert_eq!(state["P1"]["pp"]["current"], 10_u8);
}

#[test]
fn bp19_019_discount_applies_to_treasure_in_ex_during_its_controllers_turn() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"barbaros","card":"BP19-019"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"flag","card":"BP19-P33"}]);
    let mut board = game(&initial);
    let CardPlay::Awaiting(question) = board
        .play_internal_card("flag", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("flag target");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("target");
    };
    assert_eq!(domain.members.len(), 1);
    resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
    );
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 10_u8);
    assert_eq!(state["objects"]["barbaros"]["power"], 4_u8);
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
}
