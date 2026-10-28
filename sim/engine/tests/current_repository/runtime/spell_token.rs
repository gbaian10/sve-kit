use super::*;

#[test]
fn bp01_t12_spell_token_plays_from_ex_deals_damage_and_ceases_to_exist() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"mimi","card":"BP01-T12"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"victim","card":"BP01-037"}]);
    let mut game = game(&initial);
    let CardPlay::Awaiting(question) = game
        .play_internal_card("mimi", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("Mimi target");
    };
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["victim"]["damage"], 2_u8);
    assert_eq!(view["P1"]["pp"]["current"], 10_u8);
    assert_eq!(view["P1"]["ex"], json!([]));
    assert_eq!(view["P1"]["cemetery"], json!([]));
    assert!(view["objects"]["mimi"].is_null());
    game.resume_internal_card().unwrap();
    game.play_internal_card("mimi", PlayerId { seat: 1 })
        .unwrap_err();
}

#[test]
fn bp07_t02_spell_token_uses_authority_quick_window_and_intrinsic_cost() {
    let mut initial = draw_position();
    initial["players"]["P2"]["leader"]["life"] = json!(17_u8);
    initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"repair","card":"BP07-T02"}]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let question = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &question.domain else {
        panic!("Quick window");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "BP07-T02");
    assert_eq!(options[0].zone, "ex");
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Quick(Some(options[0].handle)),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P2"]["leader"]["life"], 18_u8);
    assert_eq!(view["P2"]["pp"]["current"], 9_u8);
    assert_eq!(view["P2"]["ex"], json!([]));
    assert_eq!(view["P2"]["cemetery"], json!([]));
    assert!(view["objects"]["repair"].is_null());
    game.resume_internal_card().unwrap();
    let next = game.internal_quick_choice().unwrap().unwrap();
    assert!(matches!(&next.domain, InputDomain::Quick { options: left, .. } if left.is_empty()));
}
