use super::*;
use sve_engine::game::internal::InputDomain;

fn tree(condition: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut seed = tree["cards"]["BP01-041"].clone();
    let mut header = seed["abilities"][0].clone();
    header.as_object_mut().unwrap().remove("activate");
    header["line"] = json!(2);
    header["play"] = json!({"cannot_if":condition});
    seed["abilities"] = json!([header]);
    tree["cards"] = json!({"unit-spell":seed});
    tree
}

fn restriction(condition: &Value) -> Located<Ability> {
    let lowered = lowerer().lower(&tree(condition)).unwrap();
    let mut ability = lowered["unit-spell"].card().abilities[0].clone();
    ability.node.id = AbilityId(3);
    ability.node.scope = ScopeId(3);
    ability
}

#[test]
fn printed_play_restrictions_check_origin_and_turn_before_any_authority_change() {
    for (condition, from_ex) in [
        (json!({"in_zone":{"ref":"this","zone":"ex"}}), true),
        (json!({"my_turn":true}), false),
    ] {
        let program = card(vec![play::spell(vec![]), restriction(&condition)]);
        for zone in ["hand", "ex"] {
            let mut game = play::position(&program, zone, 0);
            let before = format!("{game:?}");
            let result = game.play_internal_card("s", PlayerId { seat: 1 });
            if !from_ex || zone == "ex" {
                result.unwrap_err();
                assert_eq!(format!("{game:?}"), before);
            } else {
                resolved(result.unwrap());
                let view = game.projection(View::Referee).unwrap();
                assert_eq!(view["P1"]["pp"]["current"], 3);
                assert_eq!(view["P1"]["cemetery"], json!(["s"]));
            }
        }
    }
}

#[test]
fn printed_play_restrictions_filter_quick_options_but_allow_opponent_turn() {
    for (condition, forbidden_ex) in [
        (json!({"in_zone":{"ref":"this","zone":"ex"}}), true),
        (json!({"my_turn":true}), false),
    ] {
        let spell = card(vec![
            play::spell(vec![]),
            quick::permit(),
            restriction(&condition),
        ]);
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P2"]["pp"] = json!({"current":4,"max":4});
        initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"h","card":"unit-spell"}]);
        initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"e","card":"unit-spell"}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "restrictions",
        )
        .unwrap();
        game.load_internal_card("unit-spell", &spell).unwrap();
        game.load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let question = game.internal_quick_choice().unwrap().unwrap();
        let InputDomain::Quick { options, .. } = &question.domain else {
            panic!("Quick");
        };
        assert_eq!(options.len(), if forbidden_ex { 1 } else { 2 });
        let hand = options.iter().find(|option| option.zone == "hand").unwrap();
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Quick(Some(hand.handle)),
            )
            .unwrap(),
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P2"]["pp"]["current"], 3);
        assert_eq!(view["P2"]["cemetery"], json!(["h"]));
    }
}

#[test]
fn printed_play_restrictions_reject_other_conditions_and_forged_permissions_atomically() {
    for condition in [
        json!({"opp_turn":true}),
        json!({"in_zone":{"ref":"this","zone":"hand"}}),
        json!({"in_zone":{"ref":"target","zone":"ex"}}),
        json!({"all":[{"my_turn":true}]}),
    ] {
        lowerer().lower(&tree(&condition)).unwrap_err();
    }
    let valid = restriction(&json!({"my_turn":true}));
    for variant in 0..5 {
        let mut forged = valid.clone();
        match variant {
            0 => forged.node.active_zones = vec![ZoneKind::Field],
            1 => forged.node.source.controller = PlayerRef::Active {},
            2 => {
                forged.node.source.holder = ObjectOperand::Bound {
                    binding: Binding::new(1, ScopeId(3)),
                }
            }
            3 => {
                let Effect::Permission { polarity, .. } = &mut forged.node.declarations[0] else {
                    panic!("permission");
                };
                *polarity = Polarity::Allow;
            }
            _ => {
                let Effect::Permission { rule, .. } = &mut forged.node.declarations[0] else {
                    panic!("permission");
                };
                *rule = RuleId(1232);
            }
        }
        let program = CardProgram {
            abilities: vec![play::spell(vec![]), forged],
            card_play_costs: vec![],
        };
        if let Ok(checked) = WellFormedCardProgram::check(program) {
            let mut game = play::position(&card(vec![play::spell(vec![])]), "hand", 0);
            let before = format!("{game:?}");
            game.load_internal_card("unit-spell", &checked).unwrap_err();
            assert_eq!(format!("{game:?}"), before);
        }
    }
}
