#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "mode-conditional",
    )
    .unwrap()
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"d1","card":"BP01-171"},{"id":"d2","card":"BP01-171"},{"id":"d3","card":"BP01-171"}
    ]);
    initial
}

/// Plays the spell and returns the evaluated mode count.
fn declared_count(game: &mut Game) -> (u64, DecisionView) {
    let CardPlay::Awaiting(choice) = game.play_internal_card("source", actor()).unwrap() else {
        panic!("mode declaration expected")
    };
    let InputDomain::Modes { count, .. } = &choice.domain else {
        panic!("modes expected: {:?}", choice.domain)
    };
    (count.0, choice)
}

fn choose(game: &mut Game, choice: &DecisionView, modes: Vec<ModeId>) {
    let mut step = game
        .answer_internal_card(choice.actor, choice.token, Answer::Modes(modes))
        .unwrap();
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => {
                Answer::Targets(domain.members.iter().take(1).map(|m| m.handle).collect())
            }
            other => panic!("unexpected decision: {other:?}"),
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    resolved(step);
}

#[test]
fn bp16_074_offers_two_modes_only_after_an_own_follower_evolved_this_turn() {
    let mut plain = game(&position("BP16-074"));
    let (count, choice) = declared_count(&mut plain);
    assert_eq!(count, 1, "no evolution this turn keeps one mode");
    assert!(
        plain
            .answer_internal_card(choice.actor, choice.token, Answer::Modes(vec![ModeId(1), ModeId(2)]))
            .is_err(),
        "two modes exceed the unmet cap"
    );

    let mut initial = position("BP16-074");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"base","card":"BP01-092"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP01-093"}]);
    let mut evolved = game(&initial);
    assert_eq!(
        evolved
            .internal_rule_action(
                &json!({"do":"evolve","source":"base","evolve_card":"form","pay":{"pp":0_u8}})
            )
            .unwrap()
            .outcome,
        "resolved",
        "the base pays its printed evolution cost"
    );
    loop {
        if evolved.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            evolved.resume_internal_card().unwrap();
        }
        let Some(question) = evolved.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending ability expected")
        };
        let mut step = evolved
            .answer_internal_card(question.actor, question.token, Answer::Pending(options[0].handle))
            .unwrap();
        while let CardPlay::Awaiting(ward) = step {
            let InputDomain::WardEntry { .. } = &ward.domain else {
                panic!("unexpected evolution decision: {:?}", ward.domain)
            };
            step = evolved
                .answer_internal_card(ward.actor, ward.token, Answer::WardEntry(false))
                .unwrap();
        }
    }
    let (evolved_count, evolved_choice) = declared_count(&mut evolved);
    assert_eq!(evolved_count, 2, "an own evolution this turn allows up to two modes");
    choose(&mut evolved, &evolved_choice, vec![ModeId(1), ModeId(2)]);
    let state = evolved.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 13_u8, "mode one heals three");
    assert_eq!(
        state["P1"]["hand"].as_array().unwrap().len(),
        3,
        "the awakened evolution draws one and mode two draws two"
    );
}

#[test]
fn bp10_052_counts_chess_cards_in_its_own_cemetery() {
    let chess = |n: usize| {
        let mut initial = position("BP10-052");
        initial["players"]["P1"]["zones"]["cemetery"] = Value::Array(
            (0..n)
                .map(|i| json!({"id":format!("chess{i}"),"card":"BP03-044"}))
                .chain([json!({"id":"other","card":"BP01-171"})])
                .collect(),
        );
        // The opponent's chess cards never count toward this printed threshold.
        initial["players"]["P2"]["zones"]["cemetery"] = json!([{"id":"foreign","card":"BP03-044"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
        initial
    };
    let mut four = game(&chess(4));
    assert_eq!(declared_count(&mut four).0, 1, "four chess cards keep one mode");

    let mut five = game(&chess(5));
    let (count, choice) = declared_count(&mut five);
    assert_eq!(count, 2, "five chess cards allow up to two modes");
    choose(&mut five, &choice, vec![ModeId(1), ModeId(3)]);
    let state = five.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["foreign", "enemy"]), "the opponent sacrifices");
    assert_eq!(state["P1"]["hand"], json!(["d1", "d2"]), "mode three draws two");
    assert_eq!(state["P1"]["leader"]["life"], 10_u8, "mode two was not chosen");
}
