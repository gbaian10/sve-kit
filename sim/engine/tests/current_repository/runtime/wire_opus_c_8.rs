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

/// Answers printed questions: targets prefer `chosen` (else the first member).
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
            InputDomain::CostMaterials(d) => Answer::CostMaterials(
                d.members
                    .iter()
                    .take(usize::try_from(d.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
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

#[test]
fn bp18_044_all_printings_hit_for_the_banish_zone_size_on_evolution() {
    for form in ["BP18-044", "BP18-P18"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP18-043"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P1"]["zones"]["banish"] = json!([
            {"id":"gone0","card":"BP01-037"},
            {"id":"gone1","card":"BP01-037"},
            {"id":"gone2","card":"BP01-037"}
        ]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP13-019"}]);
        let mut game = game(&initial);
        let outcome = game
            .internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}))
            .unwrap();
        assert_eq!(outcome.outcome, "resolved", "{form}: the printed evolution");
        assert_eq!(drain(&mut game, ""), 1, "{form}");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["enemy"]["damage"], 3_u8, "{form}");
    }
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

fn hector(printing: &str) -> Game {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"snake","card":"BP01-146"},
        {"id":"sister","card":"BP01-143"},
        {"id":"guard","card":"BP01-134"},
        {"id":"snake2","card":"BP01-146"}
    ]);
    let mut game = game(&initial);
    let entered = game.play_internal_card("source", actor()).unwrap();
    answer(&mut game, entered, "");
    game
}

#[test]
fn bp08_086_all_printings_revive_two_cheap_bishops_that_gain_rush() {
    for printing in ["BP08-086", "BP08-SL16", "BP22-R13"] {
        let mut game = hector(printing);
        finish_checks(&mut game);
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("{printing}: Fanfare");
        };
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        let CardPlay::Awaiting(pick) = &step else {
            panic!("{printing}: cemetery targets");
        };
        let InputDomain::Targets(d) = &pick.domain else {
            panic!("{printing}: cemetery targets");
        };
        // Original cost 3 is not offered; up to two of the cheap ones are chosen.
        let mut offered: Vec<_> = d.members.iter().filter_map(|m| m.card.clone()).collect();
        offered.sort();
        assert_eq!(offered, ["BP01-143", "BP01-146", "BP01-146"], "{printing}");
        let two = d.members.iter().take(2).map(|m| m.handle).collect();
        let resolved = game
            .answer_internal_card(pick.actor, pick.token, Answer::Targets(two))
            .unwrap();
        answer(&mut game, resolved, "");
        let view = game.projection(View::Referee).unwrap();
        let field = view["P1"]["field"].as_array().unwrap();
        assert_eq!(field.len(), 4, "{printing}: {field:?}");
        for id in field {
            let object = &view["objects"][id.as_str().unwrap()];
            let rush = object["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("突進"));
            assert_eq!(rush, id != "ally", "{printing}: {object}");
        }
    }
}

#[test]
fn bp08_086_banishes_three_cemetery_bishops_once_per_turn_to_buff_the_others() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP08-086"},
        {"id":"ally","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"b0","card":"BP01-146"},
        {"id":"b1","card":"BP01-143"},
        {"id":"b2","card":"BP01-134"},
        {"id":"b3","card":"BP01-146"},
        {"id":"b4","card":"BP01-143"},
        {"id":"b5","card":"BP01-134"}
    ]);
    let mut game = game(&initial);
    let before = game.projection(View::Referee).unwrap();
    let act = game
        .activate_internal_card("source", activated("BP08-086"), actor())
        .unwrap();
    answer(&mut game, act, "");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["banish"].as_array().unwrap().len(), 3);
    assert_eq!(
        view["objects"]["ally"]["power"].as_u64().unwrap(),
        before["objects"]["ally"]["power"].as_u64().unwrap().saturating_add(1)
    );
    assert_eq!(
        view["objects"]["source"]["power"],
        before["objects"]["source"]["power"],
        "only the other followers"
    );
    // Once per turn, even with three more bishops left.
    game.activate_internal_card("source", activated("BP08-086"), actor())
        .unwrap_err();
}
