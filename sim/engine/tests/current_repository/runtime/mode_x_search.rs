use super::*;

#[test]
#[expect(clippy::panic, reason = "The printed modes supply their own declarations.")]
fn phoenix_howl_pays_for_its_selected_search_and_grants_rush_to_each_entrant() {
    let mut initial = position_with_source("BP12-P12", "hand");
    initial["players"]["P1"]["zones"]["deck"] = json!(cards("phoenix-", "BP04-064", 3));
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
    let mut board = game(&initial);
    let choice = await_card_choice(board.play_internal_card("source", PlayerId { seat: 1 }).unwrap());
    assert!(matches!(choice.domain, InputDomain::Modes { .. }));
    let x = await_card_choice(board.answer_internal_card(
        choice.actor, choice.token, Answer::Modes(vec![ModeId(1), ModeId(2)])
    ).unwrap());
    let InputDomain::Integer(bounds) = x.domain else {
        panic!("X follows the mode choice");
    };
    assert_eq!(bounds.maximum, Some(Nat(4)));
    let target = board.answer_internal_card(x.actor, x.token, Answer::Integer(Nat(2))).unwrap();
    let search = await_card_choice(choose_card_by_number(&mut board, target, "BP01-171"));
    let InputDomain::Search(domain) = &search.domain else {
        panic!("the paid mode searches the deck");
    };
    assert_eq!(domain.cardinality.maximum, Some(Nat(2)));
    let chosen = domain.cards.iter().take(2).map(|card| card.handle).collect();
    let answer = Answer::Search(SearchAnswer::Grouped(vec![chosen]));
    drop(resolved(board.answer_internal_card(search.actor, search.token, answer).unwrap()));
    checks(&mut board);

    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 5_u8);
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    let field = state["P1"]["field"].as_array().unwrap();
    assert_eq!(field.len(), 2);
    for id in field {
        assert!(state["objects"][id.as_str().unwrap()]["keywords"].as_array().unwrap().contains(&json!("突進")));
    }
    assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 1);
}
