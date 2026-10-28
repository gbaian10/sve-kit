use super::*;

#[test]
fn cp01_015_storm_can_attack_the_turn_it_enters() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"storm","card":"CP01-015","state":{"entered_this_turn":true}},
        {"id":"ordinary","card":"BP01-037","state":{"entered_this_turn":true}}
    ]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"storm","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    let quick = game.internal_quick_choice().unwrap().unwrap();
    assert!(matches!(
        game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
            .unwrap(),
        CardPlay::Continued(_)
    ));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        15_u8
    );
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"attack","attacker":"ordinary","target":"P2.leader"})
        )
        .unwrap()
        .outcome,
        "cannot-attack"
    );
}
