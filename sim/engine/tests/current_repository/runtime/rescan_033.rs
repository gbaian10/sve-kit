use super::*;

const fn actor(seat: u32) -> PlayerId {
    PlayerId { seat }
}

#[test]
fn cp02_084_end_phase_damages_both_opposing_leaders() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP02-084"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    let trigger = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &trigger.domain else {
        panic!("expected end-phase trigger");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "CP02-084");
    game.answer_internal_card(
        trigger.actor,
        trigger.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap();
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
}

#[test]
fn cp02_084_lastword_damages_both_opposing_leaders_when_destroyed() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"removal","card":"BP01-169"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"source","card":"CP02-084"}]);
    let mut game = game(&initial);
    let play = game.play_internal_card("removal", actor(1)).unwrap();
    let CardPlay::Awaiting(targets) = play else {
        panic!("expected Last Word source selection");
    };
    let InputDomain::Targets(domain) = &targets.domain else {
        panic!("expected opposing follower target");
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card.as_deref(), Some("CP02-084"));
    game.answer_internal_card(
        targets.actor,
        targets.token,
        Answer::Targets(vec![domain.members[0].handle]),
    )
    .unwrap();
    game.resume_internal_card().unwrap();
    let trigger = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &trigger.domain else {
        panic!("expected Last Word trigger");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "CP02-084");
    game.answer_internal_card(
        trigger.actor,
        trigger.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap();
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 19_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    assert_eq!(state["P2"]["cemetery"], json!(["source"]));
}
