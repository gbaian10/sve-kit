#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the complete printed history scenarios."
)]
use super::event_cemetery::finish_checks;
use super::*;

fn actual(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "printed-discard-history",
    )
    .unwrap()
}

fn initial() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["class"] = json!("ドラゴン");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"top","card":"BP01-171"},{"id":"spare","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"next-draw","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay, leader: bool, accept: bool) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = if question.domain == InputDomain::OptionalCost {
            Answer::OptionalCost(accept)
        } else if let InputDomain::CostMaterials(domain) = &question.domain {
            assert_eq!(
                domain.cardinality.minimum,
                Nat(1),
                "the printed choice requires exactly one card"
            );
            let member = domain
                .members
                .iter()
                .find(|member| member.card.as_deref() == Some("BP01-171"))
                .unwrap();
            Answer::CostMaterials(vec![member.handle])
        } else if let InputDomain::Targets(domain) = &question.domain {
            assert_eq!(
                domain.cardinality.minimum,
                Nat(1),
                "the printed choice requires exactly one card"
            );
            let member = domain
                .members
                .iter()
                .find(|member| member.card.is_none() == leader)
                .unwrap();
            assert_eq!(
                member.player,
                PlayerId { seat: 2 },
                "the printed target belongs to the opponent"
            );
            Answer::Targets(vec![member.handle])
        } else {
            panic!("only the printed target and discard material may intervene");
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

fn resolve(game: &mut Game, number: &str, line: u64, leader: bool, accept: bool) -> CardPlay {
    finish_checks(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed occurrence order")
    };
    let option = options
        .iter()
        .find(|option| option.card == number && option.line == line)
        .unwrap();
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(option.handle),
        )
        .unwrap();
    let play = finish(game, play, leader, accept);
    finish_checks(game);
    play
}

fn enter(game: &mut Game, holder: &str) {
    assert!(
        matches!(
            game.play_internal_card(holder, PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the complete printed card enters through ordinary play"
    );
    finish_checks(game);
}

#[test]
fn every_ritual_printing_counts_its_real_fanfare_cost_for_discount_and_two_end_phase_targets() {
    for number in ["BP02-062", "BP02-P19"] {
        let mut initial = initial();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"old","card":"BP02-062"}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":number},{"id":"fuel","card":"BP01-171"},{"id":"girl","card":"BP02-063"}
        ]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = actual(&initial);
        enter(&mut game, "source");
        resolved(resolve(&mut game, number, 2, true, true));
        let after_cost = game.projection(View::Referee).unwrap();
        assert_eq!(after_cost["P1"]["leader"]["life"], 21_u8);
        assert_eq!(after_cost["P1"]["cemetery"], json!(["fuel"]));
        enter(&mut game, "girl");
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            6_u8
        );
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        resolved(resolve(&mut game, "BP02-062", 1, false, true));
        resolved(resolve(&mut game, number, 1, true, true));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
        assert_eq!(state["P2"]["leader"]["life"], 18_u8);
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}

#[test]
fn declining_the_real_fanfare_leaves_history_false_and_keeps_the_printed_play_cost() {
    let mut initial = initial();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP02-062"},{"id":"fuel","card":"BP01-171"},{"id":"girl","card":"BP02-063"}
    ]);
    let mut game = actual(&initial);
    enter(&mut game, "source");
    assert!(matches!(
        resolve(&mut game, "BP02-062", 2, true, false),
        CardPlay::Cancelled
    ));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        20_u8
    );
    enter(&mut game, "girl");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        4_u8
    );
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    resolved(resolve(&mut game, "BP02-062", 1, true, true));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    assert_eq!(state["P1"]["hand"], json!(["fuel"]));
}

#[test]
fn q302_hand_limit_discard_occurs_after_the_real_ritual_resolves_and_does_not_retry_it() {
    let mut initial = initial();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP02-062"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!(
        (0..8_u8)
            .map(|i| json!({"id":format!("h{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    let mut game = actual(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    resolved(resolve(&mut game, "BP02-062", 1, true, true));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        20_u8
    );
    let question = game.internal_quick_choice().unwrap().unwrap();
    assert!(matches!(question.domain, InputDomain::Quick { .. }));
    assert!(matches!(
        game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap(),
        CardPlay::Continued(_)
    ));
    game.internal_rule_action(&json!({"do":"end-discard","select":["h7"]}))
        .unwrap();
    finish_checks(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["h7"]));
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 7);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    assert_eq!(state["turn"]["active"], "P2");
    assert!(game.internal_pending_choice().unwrap().is_none());
}

#[test]
fn ritual_reads_live_history_after_another_real_end_phase_cost_without_replaying_an_earlier_resolution()
 {
    for producer_first in [false, true] {
        let mut initial = initial();
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"source","card":"BP02-062"},{"id":"producer","card":"BP14-006"}
        ]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"fuel","card":"BP01-171"}]);
        let mut game = actual(&initial);
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let order = if producer_first {
            ["BP14-006", "BP02-062"]
        } else {
            ["BP02-062", "BP14-006"]
        };
        for number in order {
            resolved(resolve(&mut game, number, 1, true, true));
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!(["fuel"]));
        assert_eq!(state["P1"]["ex"], json!(["top"]));
        assert_eq!(
            state["P2"]["leader"]["life"],
            if producer_first { 18_u8 } else { 20_u8 }
        );
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}

#[test]
fn girl_reads_the_authoritative_turn_history_for_both_hand_and_ex_payment_origins() {
    for zone in ["hand", "ex"] {
        for discarded in [0_u8, 1] {
            let mut initial = initial();
            initial["players"]["P1"]["zones"][zone] = json!([{"id":"girl","card":"BP02-063"}]);
            initial["semantic_state"]["counters_this_turn"] = json!({"P1":{"discarded":discarded}});
            let mut game = actual(&initial);
            enter(&mut game, "girl");
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["pp"]["current"],
                if discarded == 1 { 9_u8 } else { 7_u8 }
            );
            assert_eq!(state["P1"]["field"], json!(["girl"]));
        }
    }
}
