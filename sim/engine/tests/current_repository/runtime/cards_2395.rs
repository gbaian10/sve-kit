use super::*;

#[test]
fn jabberwock_uses_the_paid_followers_original_cost_as_a_strict_lower_bound() {
    let mut initial = position_with_source("BP03-055", "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"material","card":"BP03-056"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"equal","card":"BP05-057"},
        {"id":"greater","card":"BP03-055"},
        {"id":"tail","card":"BP18-005"}
    ]);
    let mut game = game(&initial);
    let play = enter_with_fanfare(&mut game, "BP03-055");
    let play = choose_card_by_number(&mut game, play, "BP03-056");
    drop(accept(&mut game, play));
    checks(&mut game);

    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["field"], json!(["source", "greater"]));
    assert_eq!(after["P1"]["cemetery"], json!(["material"]));
    assert_eq!(after["P1"]["deck"], json!(["tail", "equal"]));
}

#[test]
fn valdain_declares_trees_before_choosing_at_most_that_many_modes() {
    let mut initial = position_with_source("BP17-056", "hand");
    initial["players"]["P1"]["leader"]["life"] = json!(15_i64);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"tree-1","card":"BP17-T19"},
        {"id":"tree-2","card":"BP17-T19"},
        {"id":"tree-3","card":"BP17-T19"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
    let mut game = game(&initial);
    let materials = await_card_choice(enter_with_fanfare(&mut game, "BP17-056"));
    let InputDomain::CostMaterials(domain) = &materials.domain else {
        panic!("declare the materials before choosing modes");
    };
    let mode_play = game
        .answer_internal_card(
            materials.actor,
            materials.token,
            Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(2)
                    .map(|member| member.handle)
                    .collect(),
            ),
        )
        .unwrap();
    let mode = await_card_choice(mode_play);
    let InputDomain::Modes { cardinality, .. } = mode.domain else {
        panic!("choose modes after declaring the materials");
    };
    assert_eq!(cardinality.maximum, Some(Nat(2)));
    let play = game
        .answer_internal_card(
            mode.actor,
            mode.token,
            Answer::Modes(vec![ModeId(1), ModeId(3)]),
        )
        .unwrap();
    let play = choose_card_by_number(&mut game, play, "BP01-171");
    drop(accept(&mut game, play));
    checks(&mut game);

    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["field"].as_array().unwrap().len(), 2);
    assert_eq!(after["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(after["P1"]["leader"]["life"], 17_i64);
}
