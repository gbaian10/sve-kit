#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

/// Answers printed questions: targets prefer `chosen` (else the first member); head costs are paid.
fn answer(game: &mut Game, mut step: CardPlay, chosen: &str) {
    while let CardPlay::Awaiting(q) = step {
        let reply = match &q.domain {
            InputDomain::Targets(d) => Answer::Targets(
                d.members
                    .iter()
                    .find(|m| m.card.as_deref() == Some(chosen))
                    .or_else(|| d.members.first())
                    .map(|m| m.handle)
                    .into_iter()
                    .collect(),
            ),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            other => panic!("unexpected printed decision: {other:?}"),
        };
        step = game.answer_internal_card(q.actor, q.token, reply).unwrap();
    }
    finish_checks(game);
}

/// Resolves every pending printed trigger in offer order; returns the cards whose triggers resolved.
fn drain(game: &mut Game, chosen: &str) -> Vec<String> {
    let mut resolved = Vec::new();
    finish_checks(game);
    while let Some(q) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &q.domain else {
            panic!("printed pending ability");
        };
        resolved.push(options[0].card.clone());
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        answer(game, step, chosen);
    }
    resolved
}

fn runes(form: &str) -> Game {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":4_u8,"max":4_u8});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP13-026"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"albert","card":"BP16-021"},
        {"id":"costly","card":"BP13-019"},
        {"id":"commander","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    let outcome = game
        .internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":3_u8}}))
        .unwrap();
    assert_eq!(outcome.outcome, "resolved", "{form}: the printed evolution");
    game
}

#[test]
fn bp13_027_all_printings_put_an_albert_within_max_pp_and_shoot_when_it_enters() {
    for form in ["BP13-027", "BP13-P07", "SD07-014"] {
        let mut game = runes(form);
        finish_checks(&mut game);
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("{form}: evolution trigger");
        };
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        let CardPlay::Awaiting(pick) = &step else {
            panic!("{form}: hand choice");
        };
        let InputDomain::Targets(d) = &pick.domain else {
            panic!("{form}: hand choice");
        };
        // Max PP 4: the cost-6 Albert and the non-Albert follower are not offered.
        let offered: Vec<_> = d.members.iter().filter_map(|m| m.card.clone()).collect();
        assert_eq!(offered, ["BP16-021"], "{form}");
        answer(&mut game, step, "BP16-021");
        // The entering Albert triggers line 1 (paid with the last PP) besides its own Fanfare.
        let resolved = drain(&mut game, "BP01-037");
        assert!(resolved.iter().any(|card| card == form), "{form}: {resolved:?}");
        let view = game.projection(View::Referee).unwrap();
        assert!(
            view["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("albert")),
            "{form}"
        );
        assert_eq!(view["objects"]["enemy"]["damage"], 3_u8, "{form}");
        assert_eq!(view["P1"]["pp"]["current"], 0_u8, "{form}");
    }
}

#[test]
fn bp13_027_ignores_a_royal_follower_without_albert_in_its_name() {
    let mut board = draw_position();
    board["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP13-027"}]);
    board["players"]["P1"]["zones"]["hand"] = json!([{"id":"commander","card":"BP01-037"}]);
    board["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&board);
    let entered = game.play_internal_card("commander", actor()).unwrap();
    assert!(matches!(entered, CardPlay::Entered(_)), "the commander enters");
    let resolved = drain(&mut game, "");
    assert!(
        resolved.iter().all(|card| card != "BP13-027"),
        "{resolved:?}"
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["enemy"]["damage"], 0_u8);
}
