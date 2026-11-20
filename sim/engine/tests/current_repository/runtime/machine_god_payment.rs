use super::*;

#[test]
fn machine_god_banishes_three_materials_then_resolves_two_selected_modes() {
    let mut initial = position_with_source("BP07-103", "hand");
    initial["players"]["P1"]["zones"]["cemetery"] = json!(cards("fee", "BP07-047", 3));
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"same-name","card":"BP07-103"},
        {"id":"found","card":"BP07-042"}
    ]);
    let mut game = game(&initial);
    let mode = await_card_choice(enter_with_fanfare(&mut game, "BP07-103"));
    let play = game
        .answer_internal_card(
            mode.actor,
            mode.token,
            Answer::Modes(vec![ModeId(3), ModeId(4)]),
        )
        .unwrap();
    let materials = await_card_choice(accept(&mut game, play));
    let InputDomain::CostMaterials(material_domain) = &materials.domain else {
        panic!("the chosen modes share the head cost");
    };
    let paid = game
        .answer_internal_card(
            materials.actor,
            materials.token,
            Answer::CostMaterials(
                material_domain
                    .members
                    .iter()
                    .map(|member| member.handle)
                    .collect(),
            ),
        )
        .unwrap();
    let search_question = await_card_choice(paid);
    let InputDomain::Search(domain) = &search_question.domain else {
        panic!("the fourth mode searches the deck");
    };
    assert_eq!(domain.cards.len(), 1);
    drop(
        game.answer_internal_card(
            search_question.actor,
            search_question.token,
            Answer::Search(search(domain)),
        )
        .unwrap(),
    );
    checks(&mut game);

    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["banish"].as_array().unwrap().len(), 3);
    assert_eq!(after["P1"]["hand"], json!(["found"]));
    assert_eq!(after["P2"]["leader"]["life"], 17_i64);
}
