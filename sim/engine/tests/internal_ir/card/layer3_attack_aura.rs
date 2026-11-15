use super::*;

#[test]
fn a_group_attack_lock_checks_both_controllers_original_cost_and_ability_loss() {
    let tree = equipment_played::tree(
        &json!({"static":{"effects":[{"cannot":{"what":"attack","on":{
            "who":"all","zone":"field","kind":"follower","orig_cost_le":3
        }}}]}}),
        "unit-amulet",
    );
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    for (active, cost, silenced, expected) in [
        ("P1", 0_u8, false, "cannot-attack"),
        ("P2", 3, false, "cannot-attack"),
        ("P1", 4, false, "resolved"),
        ("P2", 3, true, "resolved"),
    ] {
        let mut initial = setup(1, json!([]));
        initial["turn"]["active"] = json!(active);
        initial["players"]["P1"]["zones"] = json!({"field":[
            {"id":"provider","card":"unit-amulet","state":{"silenced":silenced}}
        ]});
        initial["players"]["P2"]["zones"] = json!({"field":[]});
        initial["players"][active]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"attacker","card":"unit-follower"}));
        let mut game = Game::new(
            catalog_with_cost(&json!(cost.to_string())),
            &initial,
            &Value::Null,
            &Value::Null,
            "group-attack-lock",
        )
        .unwrap();
        game.load_internal_card("unit-amulet", &program).unwrap();
        game.load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
        let opponent = if active == "P1" { "P2" } else { "P1" };
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"attacker","target":format!("{opponent}.leader")})
            )
            .unwrap()
            .outcome,
            expected
        );
    }
}

#[test]
fn a_distinct_attack_selector_still_requires_its_own_valid_binding() {
    let mut located = ability(1, vec![]);
    let attacker = Selector::Bound {
        reference: Binding::new(999, located.node.scope),
    };
    located.node.kind = AbilityKind::Static;
    located.node.play = None;
    located.node.body = None;
    located.node.declarations = vec![Effect::Permission {
        subject: PermissionSubject::Objects {
            objects: Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
        },
        action: ActionPattern::Attack {
            attacker,
            target: None,
        },
        polarity: Polarity::Forbid,
        condition: Expr::Literal(Bool(true)),
        duration: Duration {
            end: DurationEnd::NoTemporalExpiry {},
            anchor: TimeAnchor::Installation {},
            usage: None,
            intervals: vec![],
        },
        rule: RuleId(133),
    }];
    let error = WellFormedCardProgram::check(CardProgram {
        abilities: vec![located],
        card_play_costs: vec![],
    })
    .unwrap_err();
    assert_eq!(error.reason, WellFormedReason::UnavailableBinding);
}
