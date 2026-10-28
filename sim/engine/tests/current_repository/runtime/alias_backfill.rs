#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these game scenarios."
)]

use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"}
    ]);
    initial
}

fn target(game: &mut Game, play: CardPlay, count: usize) -> CardPlay {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected a target choice");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected target candidates");
    };
    assert_eq!(
        domain.members.len(),
        count,
        "only the expected targets are offered"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Targets(vec![domain.members[0].handle]),
    )
    .unwrap()
}

fn trigger(game: &mut Game, printing: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected one pending trigger");
    };
    assert_eq!(options.len(), 1, "only this card trigger is pending");
    assert_eq!(
        options[0].card, printing,
        "the option keeps the actual printing"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

#[test]
fn ebd04_017_sacrifices_own_card_heals_and_draws() {
    let mut initial = position("EBD04-017");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"offering","card":"BP01-016"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    let play = game.play_internal_card("source", actor()).unwrap();
    assert!(matches!(target(&mut game, play, 1), CardPlay::Resolved(_)));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!([]));
    assert_eq!(state["P2"]["field"], json!(["enemy"]));
    assert_eq!(state["P1"]["leader"]["life"], 13_u8);
    assert_eq!(state["P1"]["hand"], json!(["first"]));
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn dark_offering_cannot_replace_own_target_with_enemy() {
    let mut initial = position("BP01-140");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    game.play_internal_card("source", actor()).unwrap_err();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 10_u8);
    assert_eq!(state["P1"]["hand"], json!(["source"]));
    assert_eq!(state["P1"]["leader"]["life"], 10_u8);
}

#[test]
fn ebd01_021_creates_three_fairies_in_ex() {
    let mut game = game(&position("EBD01-021"));
    assert!(matches!(
        game.play_internal_card("source", actor()).unwrap(),
        CardPlay::Resolved(_)
    ));
    let state = game.projection(View::Referee).unwrap();
    let fairies = state["P1"]["ex"].as_array().unwrap();
    assert_eq!(fairies.len(), 3);
    for id in fairies {
        let fairy = &state["objects"][id.as_str().unwrap()];
        assert_eq!(fairy["card"], "BP01-T03");
        assert_eq!(fairy["power"], 1_u8);
        assert_eq!(fairy["hp"], 1_u8);
    }
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn admitted_fairy_attacks_for_its_printed_power() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"fairy","card":"BP01-T03"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"fairy","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    let question = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
        .unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
    assert_eq!(state["objects"]["fairy"]["power"], 1_u8);
    assert_eq!(state["objects"]["fairy"]["hp"], 1_u8);
    assert_eq!(state["objects"]["fairy"]["acted"], true);
}

#[test]
fn etd01_013_fanfare_draws_before_discarding() {
    let initial = position("ETD01-013");
    let mut game = game(&initial);
    assert!(matches!(
        game.play_internal_card("source", actor()).unwrap(),
        CardPlay::Entered(_)
    ));
    let play = trigger(&mut game, "ETD01-013");
    let before_discard = game.projection(View::Referee).unwrap();
    assert_eq!(before_discard["P1"]["hand"], json!(["first"]));
    assert_eq!(before_discard["P1"]["deck"], json!(["second"]));
    assert!(matches!(target(&mut game, play, 1), CardPlay::Resolved(_)));
    let after_discard = game.projection(View::Referee).unwrap();
    assert_eq!(after_discard["P1"]["hand"], json!([]));
    assert_eq!(after_discard["P1"]["cemetery"], json!(["first"]));
    assert_eq!(after_discard["P1"]["field"], json!(["source"]));
    assert_eq!(after_discard["P1"]["pp"]["current"], 8_u8);
}

#[test]
fn bp15_p33_lastword_survives_token_erasure_and_draws_then_discards() {
    let mut initial = position("EBD04-017");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"paper","card":"BP15-P33"}]);
    let mut game = game(&initial);
    let offered = game.play_internal_card("source", actor()).unwrap();
    assert!(matches!(
        target(&mut game, offered, 1),
        CardPlay::Resolved(_)
    ));
    game.resume_internal_card().unwrap();
    let lastword = trigger(&mut game, "BP15-P33");
    let before_discard = game.projection(View::Referee).unwrap();
    assert!(before_discard["objects"].get("paper").is_none());
    assert_eq!(before_discard["P1"]["field"], json!([]));
    assert_eq!(before_discard["P1"]["deck"], json!([]));
    assert_eq!(before_discard["P1"]["hand"], json!(["first", "second"]));
    assert!(matches!(
        target(&mut game, lastword, 2),
        CardPlay::Resolved(_)
    ));
    let after_discard = game.projection(View::Referee).unwrap();
    assert_eq!(after_discard["P1"]["hand"].as_array().unwrap().len(), 1);
    assert_eq!(after_discard["P1"]["cemetery"].as_array().unwrap().len(), 2);
}
