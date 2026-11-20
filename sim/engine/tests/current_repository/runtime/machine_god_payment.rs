use super::*;

#[test]
#[expect(clippy::panic, reason = "The declared materials and the search use distinct questions.")]
fn machine_god_banishes_three_materials_then_resolves_two_selected_modes() {
    let mut initial = position_with_source("BP07-103", "hand");
    initial["players"]["P1"]["zones"]["cemetery"] = json!(cards("fee", "BP07-047", 3));
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"same-name","card":"BP07-103"},
        {"id":"found","card":"BP07-042"}
    ]);
    let mut game = game(&initial);
    let mode = await_card_choice(enter_with_fanfare(&mut game, "BP07-103"));
    let play = game.answer_internal_card(
        mode.actor, mode.token, Answer::Modes(vec![ModeId(3), ModeId(4)]),
    ).unwrap();
    let q = await_card_choice(accept(&mut game, play));
    let InputDomain::CostMaterials(domain) = &q.domain else {
        panic!("the chosen modes share the head cost");
    };
    let play = game.answer_internal_card(
        q.actor, q.token, Answer::CostMaterials(domain.members.iter().map(|member| member.handle).collect()),
    ).unwrap();
    let q = await_card_choice(play);
    let InputDomain::Search(domain) = &q.domain else {
        panic!("the fourth mode searches the deck");
    };
    assert_eq!(domain.cards.len(), 1);
    let play = game.answer_internal_card(q.actor, q.token, Answer::Search(search(domain))).unwrap();
    drop(play);
    checks(&mut game);

    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["banish"].as_array().unwrap().len(), 3);
    assert_eq!(after["P1"]["hand"], json!(["found"]));
    assert_eq!(after["P2"]["leader"]["life"], 17);
}
