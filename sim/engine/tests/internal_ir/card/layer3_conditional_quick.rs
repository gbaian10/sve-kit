#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate the conditional Quick scenarios."
)]
use super::*;
use sve_engine::game::internal::InputDomain;

fn permission() -> Located<Ability> {
    let mut tree = equipment_played::tree(
        &json!({"static":{
            "zone":["hand","ex"],"while":{"awakened":true},
            "effects":[{"grant":{"on":"this","keywords":["quick"]}}]
        }}),
        "unit-spell",
    );
    tree["cards"]["unit-spell"]["abilities"][0]["line"] = json!(2_u8);
    let mut permission = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-spell")
        .unwrap()
        .card()
        .abilities[0]
        .clone();
    permission.node.id = AbilityId(2);
    permission.node.scope = ScopeId(2);
    permission
}

#[test]
fn conditional_quick_uses_own_maximum_pp_in_hand_and_ex_without_restricting_normal_play() {
    let program = card(vec![play::spell(vec![draw(1, 1)]), permission()]);
    for (zone, maximum, own_turn, available) in [
        ("hand", 6_u8, false, false),
        ("hand", 7, false, true),
        ("ex", 6, false, false),
        ("ex", 7, false, true),
        ("hand", 6, true, true),
    ] {
        let mut initial = setup(1, json!([]));
        initial["turn"]["active"] = json!(if own_turn { "P2" } else { "P1" });
        initial["players"]["P1"]["pp"] = json!({"current":10_u8,"max":10_u8});
        initial["players"]["P1"]["zones"] = json!({});
        initial["players"]["P2"]["pp"] = json!({"current":2_u8,"max":maximum});
        initial["players"]["P2"]["zones"] = json!({"deck":[{"id":"drawn","card":"unit-follower"}]});
        initial["players"]["P2"]["zones"][zone] = json!([{"id":"spell","card":"unit-spell"}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "conditional-quick",
        )
        .unwrap();
        game.load_internal_card("unit-spell", &program).unwrap();
        game.load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
        if own_turn {
            resolved(
                game.play_internal_card("spell", PlayerId { seat: 2 })
                    .unwrap(),
            );
        } else {
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            let question = game.internal_quick_choice().unwrap().unwrap();
            let InputDomain::Quick { options, .. } = &question.domain else {
                panic!("Quick window");
            };
            assert_eq!(options.len(), usize::from(available));
            if !available {
                continue;
            }
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Quick(Some(options[0].handle)),
                )
                .unwrap(),
            );
        }
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P2"]["pp"]["current"], 1_u8);
        assert_eq!(after["P2"]["hand"], json!(["drawn"]));
        assert_eq!(after["P2"]["cemetery"], json!(["spell"]));
    }
}

#[test]
fn a_quick_permission_condition_cannot_read_an_undefined_binding() {
    let mut located = permission();
    let Effect::Permission { condition, .. } = &mut located.node.declarations[0] else {
        panic!("Quick permission");
    };
    *condition = Expr::Bound(Binding::new(999, located.node.scope));
    let error = WellFormedCardProgram::check(CardProgram {
        abilities: vec![located],
        card_play_costs: vec![],
    })
    .unwrap_err();
    assert_eq!(error.reason, WellFormedReason::UnavailableBinding);
}
