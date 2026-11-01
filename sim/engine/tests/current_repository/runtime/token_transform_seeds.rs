#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate these real-card scenarios."
)]
use super::event_cemetery::finish_checks;
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn transform_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "token-transform-seeds",
    )
    .unwrap()
}

fn finish(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = if let InputDomain::Targets(targets) = &question.domain {
            assert_eq!(
                targets.members.len(),
                1,
                "only the intended EX card can be selected"
            );
            Answer::Targets(vec![targets.members.first().unwrap().handle])
        } else if matches!(question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if matches!(question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            panic!("unexpected transformation question");
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

fn enter(game: &mut Game) {
    let play = game.play_internal_card("source", actor()).unwrap();
    assert!(
        matches!(finish(game, play), CardPlay::Entered(_)),
        "the real follower enters after payment"
    );
}

fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed fanfare");
    };
    assert_eq!(options.len(), 1, "one printed fanfare is pending");
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options.first().unwrap().handle),
        )
        .unwrap();
    resolved(finish(game, play));
}

#[test]
fn mutated_dragon_banishes_the_old_generation_and_creates_a_fresh_token_in_the_original_ex() {
    for (seat, owner) in [("P1", "P2"), ("P2", "P1")] {
        for (card, token) in [("BP01-037", false), ("BP01-T15", true)] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP02-061"}]);
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"outside","card":"BP01-037"}]);
            initial["players"][seat]["zones"]["ex"] = json!([
                {"id":"target","card":card,"owner":owner,"state":{"power":9_u8,"hp":9_u8,"max_hp":9_u8}}]);
            let mut game = transform_game(&initial);
            enter(&mut game);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            let ex = state[seat]["ex"].as_array().unwrap();
            assert_eq!(ex.len(), 1);
            let created = ex.first().unwrap().as_str().unwrap();
            assert_ne!(created, "target");
            assert_eq!(state["objects"][created]["card"], "BP01-T11");
            assert_eq!(state["objects"][created]["power"], 5_u8);
            assert_eq!(state["objects"][created]["hp"], 5_u8);
            assert_eq!(state[owner]["ex"], json!([]));
            assert_eq!(state["P2"]["field"], json!(["outside"]));
            if token {
                assert!(state["objects"].get("target").is_none());
                assert_eq!(state[owner]["banish"], json!([]));
            } else {
                assert_eq!(state[owner]["banish"], json!(["target"]));
                assert_eq!(state["objects"]["target"]["card"], "BP01-037");
            }
        }
    }
}

#[test]
fn lantern_fairy_pays_the_printed_cost_but_transforms_only_with_three_own_ex_cards() {
    for count in [2_usize, 3] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP14-014"}]);
        initial["players"]["P1"]["zones"]["ex"] = Value::Array(
            (0..count)
                .map(|index| json!({"id":format!("own{index}"),"card":"BP01-T15"}))
                .collect(),
        );
        initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"target","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"outside","card":"BP01-037"}]);
        let mut game = transform_game(&initial);
        enter(&mut game);
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 7_u8);
        assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), count);
        assert_eq!(state["P2"]["field"], json!(["outside"]));
        let ex = state["P2"]["ex"].as_array().unwrap();
        assert_eq!(ex.len(), 1);
        if count == 3 {
            let created = ex.first().unwrap().as_str().unwrap();
            assert_ne!(created, "target");
            assert_eq!(state["objects"][created]["card"], "BP01-T03");
            assert_eq!(state["objects"][created]["power"], 1_u8);
            assert_eq!(state["objects"][created]["hp"], 1_u8);
            assert_eq!(state["P2"]["banish"], json!(["target"]));
        } else {
            assert_eq!(ex.as_slice(), &[json!("target")]);
            assert_eq!(state["P2"]["banish"], json!([]));
        }
    }
}

#[test]
fn lantern_fairy_can_decline_payment_without_selecting_or_changing_an_ex_card() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP14-014"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"own1","card":"BP01-T15"},
        {"id":"own2","card":"BP01-T15"},
        {"id":"own3","card":"BP01-T15"}]);
    initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"target","card":"BP01-037"}]);
    let mut game = transform_game(&initial);
    enter(&mut game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed fanfare");
    };
    let CardPlay::Awaiting(payment) = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options.first().unwrap().handle),
        )
        .unwrap()
    else {
        panic!("the fanfare offers its optional payment");
    };
    assert!(matches!(payment.domain, InputDomain::OptionalCost));
    assert_eq!(
        game.answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(false))
            .unwrap(),
        CardPlay::Cancelled
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
    assert_eq!(state["P1"]["ex"], json!(["own1", "own2", "own3"]));
    assert_eq!(state["P2"]["ex"], json!(["target"]));
    assert_eq!(state["P2"]["banish"], json!([]));
    finish_checks(&mut game);
    assert!(game.internal_pending_choice().unwrap().is_none());
}
