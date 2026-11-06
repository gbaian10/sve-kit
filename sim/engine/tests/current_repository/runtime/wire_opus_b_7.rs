#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

/// Answers every target or private choice with its first member.
fn answer(game: &mut Game, mut step: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(q) = step {
        let InputDomain::Targets(d) = &q.domain else {
            panic!("unexpected printed decision: {:?}", q.domain);
        };
        let first = d.members.first().map(|m| m.handle).into_iter().collect();
        step = game
            .answer_internal_card(q.actor, q.token, Answer::Targets(first))
            .unwrap();
    }
    finish_checks(game);
    step
}

fn triggers(game: &mut Game) {
    while let Some(q) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &q.domain else {
            panic!("printed pending ability");
        };
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        answer(game, step);
    }
}

fn play(game: &mut Game) {
    let step = game.play_internal_card("source", actor()).unwrap();
    assert!(matches!(step, CardPlay::Entered(_)), "the follower enters");
    finish_checks(game);
    triggers(game);
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
fn bp10_109_all_printings_destroy_and_draw_only_alone_and_heal_a_low_leader() {
    for printing in ["BP10-109", "BP10-SL25", "BP10-U07", "SD08-006"] {
        for (ally, life) in [(false, 10_u8), (true, 11)] {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
            initial["players"]["P1"]["leader"]["life"] = json!(life);
            if ally {
                initial["players"]["P1"]["zones"]["field"] =
                    json!([{"id":"ally","card":"BP01-037"}]);
            }
            let mut game = game(&initial);
            play(&mut game);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(
                view["P2"]["field"].as_array().unwrap().is_empty(),
                !ally,
                "{printing}: destroyed only without another own follower"
            );
            // Q1373: with another own follower neither the destruction nor the draw happens.
            assert_eq!(
                view["P1"]["hand"],
                if ally { json!([]) } else { json!(["drawn"]) },
                "{printing}"
            );
            assert_eq!(
                view["P1"]["leader"]["life"],
                if life <= 10 { 15_u8 } else { 11 },
                "{printing}"
            );
        }
    }
}

#[test]
fn bp10_109_banishes_itself_from_the_cemetery_to_put_the_named_advance_in_ex() {
    for (pp, usable) in [(9_u8, false), (10, true)] {
        let mut initial = position();
        initial["players"]["P1"]["pp"] = json!({"current":pp,"max":10_u8});
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"source","card":"BP10-109"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
            {"id":"world","card":"BP10-110"},
            {"id":"other","card":"BP01-102"}
        ]);
        let mut game = game(&initial);
        let step = game.activate_internal_card("source", activated("BP10-109"), actor());
        if !usable {
            step.unwrap_err();
            continue;
        }
        let CardPlay::Awaiting(q) = step.unwrap() else {
            panic!("the named Advance is chosen");
        };
        let InputDomain::Targets(d) = &q.domain else {
            panic!("private Advance choice");
        };
        let offered: Vec<_> = d.members.iter().filter_map(|m| m.card.as_deref()).collect();
        assert_eq!(offered, ["BP10-110"], "only the printed name is offered");
        let handle = d.members[0].handle;
        let picked = game
            .answer_internal_card(q.actor, q.token, Answer::Targets(vec![handle]))
            .unwrap();
        answer(&mut game, picked);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["ex"], json!(["world"]));
        assert_eq!(view["P1"]["banish"], json!(["source"]));
        assert_eq!(view["P1"]["field"], json!([]));
        assert_eq!(view["P1"]["pp"]["current"], 0_u8);
    }
}
