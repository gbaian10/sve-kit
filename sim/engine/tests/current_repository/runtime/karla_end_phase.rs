#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed Karla scenarios."
)]
use super::event_cemetery::finish_checks;
use super::*;

fn end_phase(pp: u8) -> (Game, CardPlay) {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"]["current"] = json!(pp);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"karla","card":"BP06-091"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"drawn","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the end phase starts"
    );
    finish_checks(&mut game);
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("Karla's End-phase occurrence");
    };
    let play = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(options[0].handle))
        .unwrap();
    (game, play)
}

#[test]
fn bp06_091_at_three_pp_damages_the_leader_without_asking_for_a_target() {
    let (game, play) = end_phase(3);
    assert!(
        matches!(play, CardPlay::Resolved(_)),
        "only the damage clause applies at three PP"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(state["P2"]["field"], json!(["enemy"]));
}

#[test]
fn bp06_091_at_six_pp_chooses_up_to_one_target_before_drawing() {
    let (mut game, play) = end_phase(6);
    let CardPlay::Awaiting(question) = play else {
        panic!("Karla declares a target before resolving");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("Karla's formal target choice");
    };
    assert_eq!(domain.cardinality.minimum, Nat(0));
    assert_eq!(domain.cardinality.maximum, Some(Nat(1)));
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(before["P2"]["leader"]["life"], 20_u8);
    assert_eq!(before["P1"]["deck"], json!(["drawn"]));
    let [enemy] = domain.members.as_slice() else {
        panic!("one opposing follower");
    };
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![enemy.handle]),
        )
        .unwrap(),
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P2"]["leader"]["life"], 18_u8);
    assert_eq!(after["P1"]["hand"], json!(["drawn"]));
    assert_eq!(after["P2"]["cemetery"], json!(["enemy"]));
}
