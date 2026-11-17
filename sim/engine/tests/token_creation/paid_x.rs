#![expect(
    clippy::indexing_slicing,
    clippy::default_numeric_fallback,
    clippy::panic,
    reason = "Fixed synthetic coordinates and questions must fail immediately when changed."
)]

use super::*;
use sve_engine::EngineFailure;
use sve_engine::game::internal::{Activation, DecisionView};
use sve_engine::ir::Nat;

fn input() -> (String, String, Value) {
    let (_, keywords, mut input) = selection(1);
    let (creator, source) = author(
        "creator",
        "11111111111111111111111111111111",
        "フォロワー",
        vec![
            json!({"fanfare":{"x":{},"optional_cost":{"pp":"x","result":"paid"},
            "do":[{"create":{"token":"token","to":"field","n":"x","result":"made"}}]}}),
        ],
    );
    let (token, _) = author(
        "token",
        "22222222222222222222222222222222",
        "フォロワー・トークン",
        vec![],
    );
    input["authors"][0] = source;
    (format!("{creator}\n{token}\n"), keywords, input)
}

fn position(occupied: usize) -> Value {
    let mut position = initial(occupied);
    position["players"]["P1"]["zones"]["field"] = position["players"]["P1"]["zones"]["ex"].take();
    position["players"]["P1"]["zones"]["ex"] = json!([]);
    position
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected paid X question")
    };
    question
}

fn resolved(play: CardPlay) -> Activation {
    let CardPlay::Resolved(result) = play else {
        panic!("one template truncates without a capacity question")
    };
    result
}

fn optional(game: &mut Game) -> DecisionView {
    assert!(
        matches!(
            game.play_internal_card("creator", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the follower enters before its Fanfare"
    );
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("fanfare occurrence")
    };
    assert_eq!(options.len(), 1, "only the quantity fragment triggers");
    let optional = awaiting(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    assert_eq!(
        optional.domain,
        InputDomain::OptionalCost,
        "payment is declared before choosing X"
    );
    optional
}

fn created(result: &Activation) -> usize {
    result
        .progress
        .receipts
        .values()
        .filter_map(|outcome| {
            if let InstructionOutcome::Token(outcome) = outcome {
                Some(outcome.created.len())
            } else {
                None
            }
        })
        .sum()
}

#[test]
fn paid_x_count_composition_does_not_authorize_each_player_scopes() {
    for (count, expected) in [
        (json!({"plus":["x",0]}), 3),
        (json!({"per":{"of":"x","unit":2}}), 1),
    ] {
        let (snapshot, keywords, mut input) = input();
        let body = &mut input["authors"][0]["input"]["tree"]["cards"]["creator"]["faces"]["f:11111111111111111111111111111111"]
            ["abilities"][0]["fanfare"];
        body["do"][0]["create"]["n"] = count;
        let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
        let mut game = Game::from_selected_effects(
            selected,
            &position(0),
            &Value::Null,
            &Value::Null,
            "X-composition",
        )
        .unwrap();
        let question = optional(&mut game);
        let question = awaiting(
            game.answer_internal_card(question.actor, question.token, Answer::OptionalCost(true))
                .unwrap(),
        );
        let result = resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Integer(Nat(3)))
                .unwrap(),
        );
        assert_eq!(created(&result), expected);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            6
        );
    }
    let (snapshot, keywords, mut input) = input();
    input["authors"][0]["input"]["tree"]["cards"]["creator"]["faces"]["f:11111111111111111111111111111111"]
        ["abilities"][0]["fanfare"]["do"] = json!([
        {"each_player":{"as":"p","stages":[[{"create":{"token":"token","to":"field","by":"p","n":"x"}}]]}}
    ]);
    let rejected_selection = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    assert!(matches!(
        Game::from_selected_effects(
            rejected_selection,
            &position(0),
            &Value::Null,
            &Value::Null,
            "X-stage"
        ),
        Err(EngineFailure::Unsupported(_))
    ));
    input["authors"][0]["input"]["tree"]["cards"]["creator"]["faces"]["f:11111111111111111111111111111111"]
        ["abilities"][0]["fanfare"]["do"][0]["each_player"]["stages"][0][0]["create"]["n"] =
        json!(1);
    let accepted_selection = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    Game::from_selected_effects(
        accepted_selection,
        &position(0),
        &Value::Null,
        &Value::Null,
        "literal-stage",
    )
    .unwrap();
}
