#![expect(
    clippy::panic,
    reason = "Missing prompts or resolved abilities invalidate the card scenario."
)]

use super::*;

fn actor(seat: &str) -> PlayerId {
    PlayerId {
        seat: seat_number(seat),
    }
}

fn resolve_pending(game: &mut Game, card: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected pending ability");
    };
    assert_eq!(options.len(), 1, "one pending entry is expected for {card}");
    assert_eq!(
        options[0].card, card,
        "the pending entry must be for {card}"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

fn attack(game: &mut Game, attacker: &str, target: &str) {
    game.internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":target}))
        .unwrap();
}

#[test]
fn bp04_096_attack_damages_both_leaders() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"scorpio","card":"BP04-096"}]);
    let mut game = game(&initial);
    attack(&mut game, "scorpio", "P2.leader");
    resolved(resolve_pending(&mut game, "BP04-096"));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 19_u8);
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
}

#[test]
fn bp04_117_fanfare_restores_four_life() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"isra","card":"BP04-117"}]);
    let mut game = game(&initial);
    assert!(matches!(
        game.play_internal_card("isra", actor("P1")).unwrap(),
        CardPlay::Entered(_)
    ));
    resolved(resolve_pending(&mut game, "BP04-117"));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        14_u8
    );
}

#[test]
fn bp04_078_activated_amulet_moves_to_cemetery_and_gains_life() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"egg","card":"BP04-078"}]);
    let mut game = game(&initial);
    game.activate_internal_card("egg", AbilityId(1), actor("P1"))
        .unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["egg"]));
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
}

#[test]
fn bp04_114_end_phase_grows_its_toughness() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"octopus","card":"BP04-114"}]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    resolved(resolve_pending(&mut game, "BP04-114"));
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["octopus"]["hp"],
        6_i64
    );
}
