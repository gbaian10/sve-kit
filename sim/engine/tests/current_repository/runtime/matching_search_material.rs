use super::*;

#[test]
#[expect(clippy::panic, reason = "The printed effect offers a private search.")]
fn magical_replication_matches_both_the_printed_kind_and_original_cost() {
    let mut initial = position_with_source("BP13-052", "hand");
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"material","card":"BP01-089"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"wrong-kind","card":"BP01-037"},
        {"id":"wrong-cost","card":"BP13-052"},
        {"id":"matching","card":"BP01-089"}
    ]);
    let mut board = game(&initial);
    let play = board.play_internal_card("source", PlayerId { seat: 1 }).unwrap();
    let search = await_card_choice(choose_card_by_number(&mut board, play, "BP01-089"));
    let InputDomain::Search(domain) = &search.domain else {
        panic!("private deck search");
    };
    assert!(board.internal_decision(View::P2).is_none());
    assert_eq!(domain.cards.len(), 1);
    let answer = Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle]));
    drop(resolved(board.answer_internal_card(search.actor, search.token, answer).unwrap()));
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["banish"], json!(["material"]));
    assert_eq!(state["P1"]["hand"], json!(["matching"]));
}
