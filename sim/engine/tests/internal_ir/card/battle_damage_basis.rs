use super::*;

#[test]
fn battle_damage_basis_checks_selector_bindings_lifetime_and_private_zone_authority() {
    let tree = evolution_linked_movement::tree(&[(
        "static",
        json!({
            "effects":[{"rule_override":{"rule":"battle_damage_basis","on":{"who":"self","zone":"field","kind":"follower"},"basis":"hp"}}]
        }),
    )]);
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for case in 0_u8..3 {
        let mut raw = program.card().clone();
        let Effect::RuleModifier {
            modifier: RuleModifier::BattleDamageBasis { objects },
            duration,
            ..
        } = &mut raw.abilities[0].node.declarations[0]
        else {
            panic!("combat rule")
        };
        if case == 0 {
            *objects = Selector::Bound {
                reference: Binding::new(999, ScopeId(999)),
            };
        } else if case == 1 {
            duration.end = DurationEnd::CurrentTurnEnd {};
        } else if let Selector::Zone { zone, .. } = objects {
            *zone = ZoneKind::Deck;
        } else {
            panic!("the lowered combat rule has a field selector")
        }
        if let Ok(forged) = WellFormedCardProgram::check(raw) {
            assert_eq!(
                case, 2,
                "well-formedness must still check selector bindings and lifetime"
            );
            let initial = setup(1, json!([]));
            let mut game = Game::new(
                catalog(),
                &initial,
                &Value::Null,
                &Value::Null,
                "combat-rule-authority",
            )
            .unwrap();
            assert!(
                game.load_internal_card("unit-follower", &forged).is_err(),
                "a static combat rule cannot inspect unobserved private deck members"
            );
        }
    }
}
