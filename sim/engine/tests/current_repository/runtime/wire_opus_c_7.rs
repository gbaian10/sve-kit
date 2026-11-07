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

/// Answers printed questions: targets prefer `chosen` (else the first member), orders keep the offer.
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
            InputDomain::Order(cards) => Answer::Order(cards.iter().map(|c| c.handle).collect()),
            other => panic!("unexpected printed decision: {other:?}"),
        };
        step = game.answer_internal_card(q.actor, q.token, reply).unwrap();
    }
    finish_checks(game);
}

/// Resolves every pending printed trigger in offer order; returns how many resolved.
fn drain(game: &mut Game, chosen: &str) -> usize {
    let mut resolved = 0_usize;
    finish_checks(game);
    while let Some(q) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &q.domain else {
            panic!("printed pending ability");
        };
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        answer(game, step, chosen);
        resolved = resolved.saturating_add(1);
    }
    resolved
}

fn activated(number: &str) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}

#[test]
fn bp09_117_all_printings_reveal_an_angel_and_act_with_two_angel_or_fallen_followers() {
    for printing in ["BP09-117", "PR-619"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"other","card":"BP01-037"},
            {"id":"fallen","card":"BP03-111"},
            {"id":"angel","card":"BP01-151"},
            {"id":"tail","card":"BP01-037"}
        ]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"seraph","card":"BP02-110"}]);
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        let mut game = game(&initial);
        let entered = game.play_internal_card("source", actor()).unwrap();
        assert!(matches!(entered, CardPlay::Entered(_)), "{printing}");
        finish_checks(&mut game);
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("{printing}: Fanfare");
        };
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        let CardPlay::Awaiting(pick) = &step else {
            panic!("{printing}: Look choice");
        };
        let InputDomain::Targets(d) = &pick.domain else {
            panic!("{printing}: Look choice");
        };
        let mut offered: Vec<_> = d.members.iter().filter_map(|m| m.card.clone()).collect();
        offered.sort();
        assert_eq!(offered, ["BP01-151", "BP03-111"], "{printing}");
        answer(&mut game, step, "BP03-111");
        let revealed = game.projection(View::Referee).unwrap();
        assert_eq!(revealed["P1"]["hand"], json!(["fallen"]), "{printing}");
        // One angel follower is not enough; a fallen angel follower makes two.
        game.activate_internal_card("source", activated("BP09-117"), actor())
            .unwrap_err();
        let fallen = game.play_internal_card("fallen", actor()).unwrap();
        answer(&mut game, fallen, "");
        drain(&mut game, "");
        let act = game
            .activate_internal_card("source", activated("BP09-117"), actor())
            .unwrap();
        answer(&mut game, act, "");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["leader"]["life"], 11_u8, "{printing}");
        assert!(
            view["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            "{printing}"
        );
    }
}
