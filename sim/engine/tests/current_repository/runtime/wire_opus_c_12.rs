#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

/// Evolves the tamer, opens its evolve trigger and returns the offered cards.
fn tamer() -> (Game, CardPlay, Vec<String>) {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":4_u8,"max":4_u8});
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP16-013"},
        {"id":"beast","card":"BP01-013"},
        {"id":"sage","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"lila","card":"BP03-004"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP16-014"}]);
    let mut game = game(&initial);
    let outcome = game
        .internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}))
        .unwrap();
    assert_eq!(outcome.outcome, "resolved", "the printed evolution");
    finish_checks(&mut game);
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("evolve trigger");
    };
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap();
    let CardPlay::Awaiting(pick) = &step else {
        panic!("fairy choice");
    };
    let InputDomain::Targets(d) = &pick.domain else {
        panic!("fairy choice: {:?}", pick.domain);
    };
    let mut offered: Vec<_> = d.members.iter().filter_map(|m| m.card.clone()).collect();
    offered.sort();
    (game, step, offered)
}

#[test]
fn bp16_014_buffs_up_to_two_other_fairies_on_the_field_or_in_ex() {
    let (mut game, step, offered) = tamer();
    // The evolved tamer itself and the non-fairy follower are not offered.
    assert_eq!(offered, ["BP01-013", "BP03-004"]);
    let CardPlay::Awaiting(q) = step else {
        panic!("fairy choice");
    };
    let InputDomain::Targets(d) = &q.domain else {
        panic!("fairy choice");
    };
    let all = d.members.iter().map(|m| m.handle).collect();
    let done = game
        .answer_internal_card(q.actor, q.token, Answer::Targets(all))
        .unwrap();
    assert!(!matches!(done, CardPlay::Awaiting(_)), "one choice only");
    finish_checks(&mut game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["beast"]["power"], 6_u8);
    assert_eq!(view["objects"]["beast"]["hp"], 6_u8);
    assert_eq!(view["objects"]["lila"]["power"], 2_u8, "{}", view["objects"]["lila"]);
    assert_eq!(view["objects"]["lila"]["hp"], 5_u8, "{}", view["objects"]["lila"]);
    assert_eq!(view["objects"]["sage"]["power"], 5_u8, "the non-fairy keeps its power");
}

#[test]
fn bp16_014_may_choose_no_fairy() {
    let (mut game, step, _) = tamer();
    let CardPlay::Awaiting(q) = step else {
        panic!("fairy choice");
    };
    game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![]))
        .unwrap();
    finish_checks(&mut game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["beast"]["power"], 5_u8);
    assert_eq!(view["objects"]["lila"]["power"], 1_u8);
}
