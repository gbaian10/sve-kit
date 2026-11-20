use super::union_burst_batch1::{finish, pending, question};
use super::*;

#[test]
fn calamity_circle_uses_declared_counters_and_destroys_only_matching_original_cost_followers() {
    let mut initial = position_with_source("BP10-102", "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"sloth","card":"BP10-094"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"match-a","card":"BP01-018"},
        {"id":"match-b","card":"BP01-018"},
        {"id":"other-cost","card":"BP01-171"},
        {"id":"other-kind","card":"BP10-102"}
    ]);
    let mut board = game(&initial);
    let play = board.play_internal_card("source", actor()).unwrap();
    finish(&mut board, play);
    checks(&mut board);
    let declaration = question(pending(&mut board, "BP10-102"));
    assert_eq!(
        declaration.domain,
        InputDomain::Integer(Cardinality {
            minimum: Nat(0),
            maximum: Some(Nat(2)),
        })
    );
    let answer = board
        .answer_internal_card(
            declaration.actor,
            declaration.token,
            Answer::Integer(Nat(2)),
        )
        .unwrap();
    let result = resolved(finish(&mut board, answer));
    assert!(result.progress.facts.iter().any(|fact| {
        matches!(fact, Fact::Declared { player, value: Nat(2) } if *player == actor())
    }));
    checks(&mut board);
    assert_eq!(
        board.projection(View::Referee).unwrap()["objects"]["source"]["counters"]["災いカウンター"],
        2_u8
    );

    let ability = selection().current["BP10-102"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == 3)
        .unwrap()
        .node
        .id;
    let activation = board
        .activate_internal_card("source", ability, actor())
        .unwrap();
    drop(resolved(finish(&mut board, activation)));
    checks(&mut board);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P2"]["cemetery"], json!(["match-a", "match-b"]));
    assert_eq!(state["P2"]["field"], json!(["other-cost", "other-kind"]));
}
