use super::*;

#[test]
fn colossus_shuffles_five_graveyard_cards_into_the_whole_deck_before_dealing_damage() {
    let mut initial = position_with_source("BP05-067", "hand");
    initial["players"]["P1"]["zones"]["cemetery"] = json!(cards("fee", "BP01-171", 5));
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"old-top","card":"BP18-005"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP03-055"}]);
    let shuffled = json!(["fee4", "fee3", "fee2", "old-top", "fee1", "fee0"]);
    let random = json!({"shuffles":[{"player":"P1","result":shuffled}]});
    let mut game = Game::from_selected_effects(
        selection().clone(),
        &initial,
        &Value::Null,
        &random,
        "colossus-payment",
    )
    .unwrap();
    let play = enter_with_fanfare(&mut game, "BP05-067");
    let play = choose_card_by_number(&mut game, play, "BP03-055");
    let q = await_card_choice(play);
    let InputDomain::CostMaterials(domain) = &q.domain else {
        panic!("the cost requires five cards from the cemetery");
    };
    let paid = game
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::CostMaterials(domain.members.iter().map(|member| member.handle).collect()),
        )
        .unwrap();
    drop(accept(&mut game, paid));
    checks(&mut game);

    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["cemetery"], json!([]));
    assert_eq!(after["P1"]["deck"], shuffled);
    assert_eq!(after["objects"]["enemy"]["damage"], 5_i64);
}
