#![expect(
    clippy::panic,
    reason = "The complete printed cards must expose only their declared pending and target choices."
)]
use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::internal::DecisionView;

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "real-public-history",
    )
    .unwrap()
}
fn ability(number: &str, kind: AbilityKind) -> AbilityId {
    selection_with_rules().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == kind)
        .unwrap()
        .node
        .id
}
fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("the printed choice must be offered");
    };
    q
}
fn pending(board: &mut Game, number: &str) -> CardPlay {
    finish_checks(board);
    let q = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("one printed pending occurrence");
    };
    assert_eq!(
        options.len(),
        1_usize,
        "only the complete printed source has an occurrence"
    );
    assert_eq!(
        options[0].card, number,
        "the expected printing owns the occurrence"
    );
    board
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap()
}
fn choose_first(board: &mut Game, play: CardPlay, expected: usize) -> Activation {
    let q = awaiting(play);
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("a formal follower target");
    };
    assert_eq!(
        domain.members.len(),
        expected,
        "the whole printed target domain is available"
    );
    resolved(
        board
            .answer_internal_card(
                q.actor,
                q.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
    )
}

#[test]
fn every_brand_new_beat_print_pays_one_and_observes_a_real_lesson_then_its_own_paid_departure() {
    for number in ["CP02-007", "CP02-P04"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"miku","card":"CP02-003"}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"item","card":"CP02-T01"}]);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"drawn","card":"BP01-171"},{"id":"tail","card":"BP01-171"}]);
        let mut board = game(&initial);
        assert!(
            matches!(
                board
                    .play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "the actual amulet must pay and enter"
        );
        finish_checks(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            9_u8
        );
        let play = board
            .activate_internal_card(
                "miku",
                ability("CP02-003", AbilityKind::Activated),
                PlayerId { seat: 1 },
            )
            .unwrap();
        let q = awaiting(play);
        let InputDomain::CostMaterials(domain) = &q.domain else {
            panic!("the actual Lesson requires an EX material");
        };
        assert_eq!(
            domain.members.len(),
            1_usize,
            "only the actual magic item is available"
        );
        resolved(
            board
                .answer_internal_card(
                    q.actor,
                    q.token,
                    Answer::CostMaterials(vec![domain.members[0].handle]),
                )
                .unwrap(),
        );
        finish_checks(&mut board);
        let before = board.projection(View::Referee).unwrap();
        assert_eq!(before["P1"]["ex"], json!([]));
        assert_eq!(before["P1"]["cemetery"], json!([]));
        assert_eq!(before["P1"]["pp"]["current"], 8_u8);
        let played = board
            .activate_internal_card(
                "source",
                ability(number, AbilityKind::Activated),
                PlayerId { seat: 1 },
            )
            .unwrap();
        choose_first(&mut board, played, 1);
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["cemetery"], json!(["source"]));
        assert_eq!(view["objects"]["miku"]["power"], 4_u8);
        assert_eq!(view["objects"]["miku"]["hp"], 2_u8);
        let trigger = pending(&mut board, number);
        resolved(trigger);
        finish_checks(&mut board);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["hand"], json!(["drawn"]));
        assert_eq!(after["P1"]["deck"], json!(["tail"]));
        assert_eq!(after["P1"]["pp"]["current"], 8_u8);
    }
}

#[test]
fn both_blizzard_prints_follow_the_real_five_pp_fanfare_and_one_pp_evolution() {
    for number in ["BP20-069", "BP20-P37"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["class"] = json!("ドラゴン");
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP20-068"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"victim","card":"BP01-171"},
            {"id":"historical","card":"BP01-171","state":{"damaged_this_turn":true,"damage":0_u8,"hp":2_u8,"max_hp":2_u8}},
            {"id":"untouched","card":"BP01-171"}
        ]);
        let mut board = game(&initial);
        assert!(
            matches!(
                board
                    .play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "the actual normal face must pay five PP"
        );
        let fanfare = pending(&mut board, "BP20-068");
        choose_first(&mut board, fanfare, 3);
        finish_checks(&mut board);
        let before = board.projection(View::Referee).unwrap();
        assert_eq!(before["P1"]["pp"]["current"], 5_u8);
        assert_eq!(before["objects"]["victim"]["hp"], 1_u8);
        assert_eq!(before["objects"]["historical"]["hp"], 2_u8);
        assert_eq!(
            board
                .internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
            "resolved",
            "the real printed one-PP evolution must complete"
        );
        let trigger = pending(&mut board, number);
        resolved(trigger);
        finish_checks(&mut board);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["pp"]["current"], 4_u8);
        assert_eq!(after["P2"]["field"], json!(["untouched"]));
        assert_eq!(after["P2"]["cemetery"], json!(["victim", "historical"]));
        assert_eq!(after["objects"]["source"]["power"], 5_u8);
        assert_eq!(after["objects"]["source"]["hp"], 5_u8);
        assert_eq!(after["P1"]["evolution"], json!(["form"]));
    }
}
