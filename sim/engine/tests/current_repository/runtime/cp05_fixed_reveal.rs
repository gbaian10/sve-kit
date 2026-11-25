#![expect(
    clippy::panic,
    reason = "A different decision invalidates the printed-card scenario."
)]

use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn answer(game: &mut Game, question: &DecisionView, answer: Answer) -> CardPlay {
    game.answer_internal_card(question.actor, question.token, answer)
        .unwrap()
}

fn answer_single_target(game: &mut Game, play: CardPlay) -> CardPlay {
    let question = awaiting(play);
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected one target");
    };
    assert_eq!(
        domain.members.len(),
        1,
        "the test position has one eligible card"
    );
    answer(
        game,
        &question,
        Answer::Targets(vec![domain.members[0].handle]),
    )
}

fn first_trigger(game: &mut Game, number: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected a pending ability");
    };
    let option = options.iter().find(|option| option.card == number).unwrap();
    answer(game, &question, Answer::Pending(option.handle))
}

fn fanfare(game: &mut Game, number: &str) -> CardPlay {
    assert!(
        matches!(
            game.play_internal_card("source", PLAYER).unwrap(),
            CardPlay::Entered(_)
        ),
        "the card entered before its fanfare"
    );
    first_trigger(game, number)
}

fn enemy_position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn accept_reveal(game: &mut Game, card: &str) -> DecisionView {
    let decision = awaiting(fanfare(game, card));
    assert_eq!(decision.domain, InputDomain::OptionalCost);
    let target = answer(game, &decision, Answer::OptionalCost(true));
    awaiting(answer_single_target(game, target))
}

fn material_handles(question: &DecisionView) -> Vec<u64> {
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("expected a material payment");
    };
    domain.members.iter().map(|member| member.handle).collect()
}

#[test]
fn colossus_magna_reveals_three_fire_cards_without_discarding_them() {
    let mut initial = enemy_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"CP05-044"},
        {"id":"fire1","card":"CP05-051"},
        {"id":"fire2","card":"CP05-051"},
        {"id":"fire3","card":"CP05-051"},
        {"id":"ordinary","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    let materials = accept_reveal(&mut game, "CP05-044");
    let handles = material_handles(&materials);
    assert_eq!(handles.len(), 3);
    assert!(game.internal_decision(View::P2).is_none());
    resolved(answer(
        &mut game,
        &materials,
        Answer::CostMaterials(handles),
    ));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 4);
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
}

#[test]
fn elmott_deals_three_after_the_printed_reveal_payment() {
    let mut initial = enemy_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"CP05-051"},
        {"id":"fire1","card":"CP05-044"},
        {"id":"fire2","card":"CP05-044"},
        {"id":"fire3","card":"CP05-044"}
    ]);
    let mut game = game(&initial);
    let materials = accept_reveal(&mut game, "CP05-051");
    let handles = material_handles(&materials);
    assert_eq!(handles.len(), 3);
    assert!(game.internal_decision(View::P2).is_none());
    resolved(answer(
        &mut game,
        &materials,
        Answer::CostMaterials(handles),
    ));
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        3_u8
    );
}
