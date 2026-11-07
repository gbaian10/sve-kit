#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

/// Resolves every pending printed trigger; an optional head payment is accepted when `pay`.
fn drain(game: &mut Game, pay: bool) -> usize {
    let mut resolved = 0_usize;
    finish_checks(game);
    while let Some(q) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &q.domain else {
            panic!("printed pending ability");
        };
        let mut step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        while let CardPlay::Awaiting(question) = step {
            let reply = match &question.domain {
                InputDomain::OptionalCost => Answer::OptionalCost(pay),
                other => panic!("unexpected printed decision: {other:?}"),
            };
            step = game
                .answer_internal_card(question.actor, question.token, reply)
                .unwrap();
        }
        finish_checks(game);
        resolved = resolved.saturating_add(1);
    }
    resolved
}

fn evolved(base: &str, form: &str, enemy_life: u8, pay: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy0","card":"BP01-037"},
        {"id":"enemy1","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["leader"]["life"] = json!(enemy_life);
    let mut game = game(&initial);
    let outcome = game
        .internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}))
        .unwrap();
    assert_eq!(outcome.outcome, "resolved", "{form}: the printed evolution");
    drain(&mut game, pay);
    game.projection(View::Referee).unwrap()
}

#[test]
fn bp02_075_evolution_sets_the_opponent_leader_to_ten() {
    // Q309: the life becomes 10 even when it was lower.
    for (before, after) in [(20_u8, 10_u8), (9, 10)] {
        let view = evolved("BP02-074", "BP02-075", before, true);
        assert_eq!(view["P2"]["leader"]["life"], after, "{before}");
        assert_eq!(view["P1"]["leader"]["life"], 20_u8, "{before}");
        assert!(
            view["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("必殺")),
            "{}",
            view["objects"]["source"]
        );
    }
}
