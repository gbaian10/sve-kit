use super::*;

#[test]
fn awakened_dragonic_rage_can_be_played_in_quick_and_creates_its_real_tree() {
    let mut initial = draw_position();
    initial["players"]["P2"]["pp"] = json!({"current":2_u8,"max":7_u8});
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"spell","card":"BP12-058"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"hp":10_u8}}
    ]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let quick = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &quick.domain else {
        panic!("Quick window");
    };
    assert_eq!(quick.actor, PlayerId { seat: 2 });
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "BP12-058");
    let CardPlay::Awaiting(target) = game
        .answer_internal_card(
            quick.actor,
            quick.token,
            Answer::Quick(Some(options[0].handle)),
        )
        .unwrap()
    else {
        panic!("formal target");
    };
    resolved(
        game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["objects"]["enemy"]["hp"], 6_u8);
    assert_eq!(after["P2"]["pp"]["current"], 0_u8);
    assert_eq!(after["P2"]["cemetery"], json!(["spell"]));
    let field = after["P2"]["field"].as_array().unwrap();
    assert_eq!(field.len(), 1);
    let tree = field[0].as_str().unwrap();
    assert_eq!(after["objects"][tree]["card"], "BP07-T03");
}
