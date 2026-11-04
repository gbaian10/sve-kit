#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate the nested ability scenarios."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, InputDomain};

fn build(pp: u8) -> (Game, AbilityId) {
    let mut tree = fanfare::expanded::tree().clone();
    let mut card = tree["cards"]["BP01-041"].clone();
    let mut parent = card["abilities"][0].clone();
    let mut child = parent.clone();
    parent["activate"] = json!({"cost":{"pp":1},"do":[{"life":{"who":"self","delta":1}},{"play_ability":{"of":"this","trigger":"evolve_time"}},{"life":{"who":"self","delta":10}}]});
    child.as_object_mut().unwrap().remove("activate");
    child["line"] = json!(4);
    child["evolve_time"] = json!({"cost":{"pp":2},"do":[{"life":{"who":"self","delta":2}}]});
    card["abilities"] = json!([parent, child]);
    tree["cards"] = json!({"unit-follower":card});
    let programs = lowerer().lower(&tree).unwrap();
    let definition = programs["unit-follower"].card().abilities[0].node.id;
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":pp,"max":pp});
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "nested-ability",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    (game, definition)
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected a formal ability question");
    };
    question
}

fn choose(game: &mut Game, definition: AbilityId) -> CardPlay {
    let question = awaiting(
        game.activate_internal_card("source", definition, PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::Abilities(options) = &question.domain else {
        panic!("expected effective abilities");
    };
    assert_eq!(
        options.len(),
        1,
        "the source has one current evolution-time ability"
    );
    let before = game.projection(View::Referee).unwrap();
    game.answer_internal_card(PlayerId { seat: 2 }, question.token, Answer::Ability(0))
        .unwrap_err();
    game.answer_internal_card(question.actor, question.token, Answer::Ability(999))
        .unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "invalid answers must preserve the pending ability choice"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Ability(options[0].handle),
    )
    .unwrap()
}

#[test]
fn normal_ability_replay_pays_the_original_head_and_resumes_the_parent_without_evolving() {
    let (mut game, definition) = build(5);
    let payment = awaiting(choose(&mut game, definition));
    assert!(matches!(payment.domain, InputDomain::OptionalCost));
    let result = resolved(
        game.answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(true))
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 2_u8);
    assert_eq!(state["P1"]["leader"]["life"], 33_u8);
    assert_eq!(result.source.definition, definition);
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
            .count(),
        2
    );
    assert!(
        !result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Evolved(_) | Fact::CardPlayed { .. }))
    );
}

#[test]
fn declining_or_failing_the_child_head_keeps_the_mother_payment_and_earlier_body() {
    for (pp, accepted) in [(5_u8, false), (2_u8, true)] {
        let (mut game, definition) = build(pp);
        let payment = awaiting(choose(&mut game, definition));
        let result = resolved(
            game.answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(accepted))
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], pp - 1);
        assert_eq!(state["P1"]["leader"]["life"], 31_u8);
        assert_eq!(result.source.definition, definition);
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
                .count(),
            1
        );
    }
}
