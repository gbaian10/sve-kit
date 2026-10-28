use super::*;

#[test]
fn sd03_008_discards_one_selected_spell_as_cost_before_drawing() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"payer","card":"SD03-008"}]);
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"spell","card":"BP01-168"},{"id":"other","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    let mut game = game(&initial);
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("payer", AbilityId(4), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("hand cost");
    };
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("hand materials");
    };
    assert_eq!(domain.members.len(), 1);
    assert!(game.internal_decision(View::P2).is_none());
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["payer"]["acted"],
        false
    );
    let result = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![0]),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["cemetery"], json!(["spell"]));
    assert_eq!(view["P1"]["hand"], json!(["other", "drawn"]));
    assert_eq!(view["objects"]["payer"]["acted"], true);
    assert!(
        result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Discard(_)))
    );
    game.resume_internal_card().unwrap();
    let before = game.projection(View::Referee).unwrap();
    game.activate_internal_card("payer", AbilityId(4), PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
}
