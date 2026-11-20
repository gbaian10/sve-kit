use super::union_burst_batch1::{pending, question};
use super::*;

#[test]
#[expect(
    clippy::panic,
    reason = "The matching roll must open its printed search."
)]
fn evolved_mitra_declares_before_rolling_and_gets_all_three_rewards_on_a_match() {
    let mut initial = position_with_source("BP06-110", "field");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["pp"]["current"] = json!(3_u8);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP06-111"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"prize","card":"BP01-171"}]);
    let mut board = Game::from_selected_effects(
        selection().clone(),
        &initial,
        &Value::Null,
        &json!({"dice":[4_u8]}),
        "mitra-evolution",
    )
    .unwrap();
    assert_eq!(
        board
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
        "resolved"
    );
    let declaration = question(pending(&mut board, "BP06-111"));
    assert_eq!(
        declaration.domain,
        InputDomain::Integer(Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(6)),
        })
    );
    let before = board.projection(View::Referee).unwrap();
    assert_eq!(before["P1"]["leader"]["life"], 10_u8);
    assert_eq!(before["P1"]["pp"]["current"], 2_u8);

    let find = question(
        board
            .answer_internal_card(
                declaration.actor,
                declaration.token,
                Answer::Integer(Nat(4)),
            )
            .unwrap(),
    );
    let InputDomain::Search(domain) = &find.domain else {
        panic!("a correct declaration must lead to a search")
    };
    let result = resolved(
        board
            .answer_internal_card(find.actor, find.token, Answer::Search(search(domain)))
            .unwrap(),
    );
    let facts = result
        .progress
        .facts
        .iter()
        .filter(|fact| matches!(fact, Fact::Declared { .. } | Fact::Dice(_)))
        .collect::<Vec<_>>();
    assert!(
        matches!(facts.as_slice(), [Fact::Declared { value: Nat(4), .. }, Fact::Dice(event)] if event.value == DieFace::Four)
    );
    checks(&mut board);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 15_u8);
    assert_eq!(state["P1"]["ex"], json!(["prize"]));
    assert_eq!(state["P1"]["pp"]["current"], state["P1"]["pp"]["max"]);
}
