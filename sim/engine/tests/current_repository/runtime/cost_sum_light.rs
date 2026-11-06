#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn awaiting(play: CardPlay, why: &str) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("{why}");
    };
    question
}

fn handle(question: &DecisionView, card: &str) -> u64 {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected a card choice");
    };
    domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some(card))
        .unwrap_or_else(|| panic!("{card} must be a candidate"))
        .handle
}

fn candidates(question: &DecisionView) -> usize {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected a card choice");
    };
    domain.members.len()
}

/// Rejected answers must leave the same question open.
fn rejects(game: &mut Game, question: &DecisionView, handles: Vec<u64>) {
    let before = format!("{game:?}");
    game.answer_internal_card(question.actor, question.token, Answer::Targets(handles))
        .unwrap_err();
    assert_eq!(
        format!("{game:?}"),
        before,
        "a rejected answer leaves the choice open"
    );
}

fn order_rest(game: &mut Game, mut play: CardPlay) {
    // A Ward follower put onto the field may first choose to enter acted.
    while let CardPlay::Awaiting(question) = &play
        && matches!(question.domain, InputDomain::WardEntry { .. })
    {
        play = game
            .answer_internal_card(question.actor, question.token, Answer::WardEntry(false))
            .unwrap();
    }
    let order = awaiting(play, "the unchosen Look cards must be ordered");
    let InputDomain::Order(cards) = &order.domain else {
        panic!("expected the private bottom order, got {:?}", order.domain);
    };
    let handles = cards.iter().map(|card| card.handle).collect();
    resolved(
        game.answer_internal_card(order.actor, order.token, Answer::Order(handles))
            .unwrap(),
    );
}

fn position(card: &str, deck: &Value, cemetery: &Value) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = deck.clone();
    initial["players"]["P1"]["zones"]["cemetery"] = cemetery.clone();
    initial
}

fn field(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()["P1"]["field"].clone()
}

/// Resolves the returned followers' own entry abilities; nothing here can pay an optional cost.
fn settle(game: &mut Game) {
    loop {
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("entry trigger");
        };
        let mut step = game
            .answer_internal_card(question.actor, question.token, Answer::Pending(options[0].handle))
            .unwrap();
        while let CardPlay::Awaiting(next) = &step {
            assert!(
                matches!(next.domain, InputDomain::OptionalCost),
                "only the unpayable discard cost may be asked, got {:?}",
                next.domain
            );
            step = game
                .answer_internal_card(next.actor, next.token, Answer::OptionalCost(false))
                .unwrap();
        }
    }
}

fn end_phase(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the controller reaches the end phase"
    );
    loop {
        while let Some(question) = game.internal_pending_choice().unwrap() {
            let InputDomain::Pending(options) = &question.domain else {
                panic!("end trigger");
            };
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap(),
            );
            game.resume_internal_card().unwrap();
        }
        let Some(quick) = game.internal_quick_choice().unwrap() else {
            break;
        };
        game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
            .unwrap();
    }
}

#[test]
fn bp07_037_puts_any_number_of_machines_whose_original_costs_total_at_most_six() {
    let deck = json!([
        {"id":"one","card":"BP17-052"},
        {"id":"two","card":"BP17-077"},
        {"id":"three","card":"BP07-076"},
        {"id":"four","card":"BP07-080"},
        {"id":"other","card":"BP01-037"},
        {"id":"tail","card":"BP01-037"}
    ]);
    for printing in ["BP07-037", "BP07-SL09"] {
        let mut game = game(&position(printing, &deck, &json!([])));
        assert!(
            matches!(
                game.play_internal_card("source", PLAYER).unwrap(),
                CardPlay::Entered(_)
            ),
            "{printing} enters before its fanfare"
        );
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("expected the fanfare of {printing}");
        };
        let question = awaiting(
            game.answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
            "the cost-capped Look choice must be offered",
        );
        assert_eq!(candidates(&question), 4, "only machine followers are offered");
        let (one, two, three, four) = (
            handle(&question, "BP17-052"),
            handle(&question, "BP17-077"),
            handle(&question, "BP07-076"),
            handle(&question, "BP07-080"),
        );
        rejects(&mut game, &question, vec![one, two, four]);
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![one, two, three]),
            )
            .unwrap();
        order_rest(&mut game, play);
        assert_eq!(field(&game), json!(["source", "one", "two", "three"]));
    }
}

#[test]
fn bp06_024_puts_royal_followers_costing_three_or_less_within_a_total_of_seven() {
    let deck = json!([
        {"id":"two","card":"BP01-046"},
        {"id":"three","card":"BP01-048"},
        {"id":"nightmare","card":"BP17-077"},
        {"id":"three_b","card":"CP02-030"},
        {"id":"five","card":"BP01-037"},
        {"id":"tail","card":"BP03-033"}
    ]);
    for printing in ["BP06-024", "BP06-P07", "PR-141"] {
        let mut game = game(&position(printing, &deck, &json!([])));
        let question = awaiting(
            game.play_internal_card("source", PLAYER).unwrap(),
            "the spell's Look choice must be offered",
        );
        assert_eq!(
            candidates(&question),
            3,
            "the cost-5 Royal follower and the cost-2 Nightmare follower are not offered"
        );
        let (two, three, three_b) = (
            handle(&question, "BP01-046"),
            handle(&question, "BP01-048"),
            handle(&question, "CP02-030"),
        );
        rejects(&mut game, &question, vec![two, three, three_b]);
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![three, three_b]),
            )
            .unwrap();
        order_rest(&mut game, play);
        assert_eq!(field(&game), json!(["three", "three_b"]));
    }
}

#[test]
fn bp12_075_returns_up_to_two_machines_whose_original_costs_total_at_most_five() {
    let cemetery = json!([
        {"id":"two","card":"BP17-077"},
        {"id":"three","card":"BP07-076"},
        {"id":"four","card":"BP07-080"},
        {"id":"other","card":"BP01-037"}
    ]);
    for printing in ["BP12-075", "BP12-SL21"] {
        let mut game = game(&position(printing, &json!([]), &cemetery));
        let question = awaiting(
            game.play_internal_card("source", PLAYER).unwrap(),
            "the spell must declare its cemetery targets",
        );
        assert_eq!(candidates(&question), 3, "only machine followers are targets");
        let (two, three, four) = (
            handle(&question, "BP17-077"),
            handle(&question, "BP07-076"),
            handle(&question, "BP07-080"),
        );
        rejects(&mut game, &question, vec![three, four]);
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![two, three]),
            )
            .unwrap(),
        );
        assert_eq!(field(&game), json!(["two", "three"]));
    }
}

#[test]
fn bp15_050_returned_followers_within_nine_are_destroyed_at_the_end_phase() {
    let cemetery = json!([
        {"id":"two","card":"BP05-046"},
        {"id":"three","card":"BP20-046"},
        {"id":"seven","card":"BP05-035"},
        {"id":"other","card":"BP17-077"}
    ]);
    for printing in ["BP15-050", "BP15-P12"] {
        let mut game = game(&position(printing, &json!([]), &cemetery));
        let question = awaiting(
            game.play_internal_card("source", PLAYER).unwrap(),
            "the spell must declare its cemetery targets",
        );
        assert_eq!(candidates(&question), 3, "only 絶傑 and 魔法使い followers are targets");
        let (two, three, seven) = (
            handle(&question, "BP05-046"),
            handle(&question, "BP20-046"),
            handle(&question, "BP05-035"),
        );
        rejects(&mut game, &question, vec![two, three, seven]);
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![two, seven]),
            )
            .unwrap(),
        );
        assert_eq!(field(&game), json!(["two", "seven"]));
        settle(&mut game);
        end_phase(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!([]));
        for id in ["two", "seven"] {
            assert!(
                state["P1"]["cemetery"].as_array().unwrap().contains(&json!(id)),
                "{id} is destroyed by its granted end-phase ability"
            );
        }
    }
}

#[test]
fn bp21_p64_puts_academy_or_beast_followers_within_five_then_buffs_every_ally() {
    let deck = json!([
        {"id":"beast","card":"BP12-076"},
        {"id":"student","card":"BP08-042"},
        {"id":"two","card":"BP21-055"},
        {"id":"three","card":"BP21-009"},
        {"id":"beast_b","card":"BP18-011"},
        {"id":"both","card":"BP21-103"},
        {"id":"other","card":"BP01-037"},
        {"id":"tail","card":"BP01-037"}
    ]);
    for printing in ["BP21-P64", "BP21-T02"] {
        let mut initial = position(printing, &deck, &json!([]));
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
        let mut game = game(&initial);
        let question = awaiting(
            game.play_internal_card("source", PLAYER).unwrap(),
            "the spell's Look choice must be offered",
        );
        assert_eq!(
            candidates(&question),
            6,
            "an academy beast is one candidate of the union, not two"
        );
        let (beast, student, two, three) = (
            handle(&question, "BP12-076"),
            handle(&question, "BP08-042"),
            handle(&question, "BP21-055"),
            handle(&question, "BP21-009"),
        );
        rejects(&mut game, &question, vec![beast, two, three]);
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![beast, student, three]),
            )
            .unwrap();
        order_rest(&mut game, play);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["ally", "beast", "student", "three"]));
        // Printed stats: ally 5/5, beast 1/1, student 1/1, three 2/3.
        for (id, power, hp) in [("ally", 6, 6), ("beast", 2, 2), ("student", 2, 2), ("three", 3, 4)] {
            assert_eq!(
                (
                    state["objects"][id]["power"].as_i64().unwrap(),
                    state["objects"][id]["hp"].as_i64().unwrap()
                ),
                (power, hp),
                "every allied follower, including the new ones, gets +1/+1"
            );
        }
        assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 5);
    }
}
