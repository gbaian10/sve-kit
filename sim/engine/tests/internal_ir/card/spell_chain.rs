use super::*;
use sve_engine::game::internal::ResumeStage;

fn tree() -> Value {
    evolution_linked_movement::tree(&[(
        "static",
        json!({"effects":[{"rule_override":{"rule":"spellchain_population","who":"self","of":{"who":"self","zone":"cemetery","kind":"follower"}}}]}),
    )])
}

#[test]
fn spell_chain_population_checks_bindings_lifetime_and_private_zone_authority() {
    let program = lowerer()
        .lower(&tree())
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for case in 0_u8..3 {
        let mut raw = program.card().clone();
        let Effect::RuleModifier {
            modifier: RuleModifier::SpellChainPopulation { population, .. },
            duration,
            ..
        } = &mut raw.abilities[0].node.declarations[0]
        else {
            panic!("population rule")
        };
        if case == 0 {
            *population = Selector::Bound {
                reference: Binding::new(999, ScopeId(999)),
            };
        } else if case == 1 {
            duration.end = DurationEnd::CurrentTurnEnd {};
        } else {
            let Selector::Union { sets } = population else {
                panic!("population union")
            };
            let Selector::Zone { zone, .. } = &mut sets[0] else {
                panic!("grave population")
            };
            *zone = ZoneKind::Deck;
        }
        if let Ok(forged) = WellFormedCardProgram::check(raw) {
            assert_eq!(
                case, 2,
                "bindings and the complete lifetime remain structurally checked"
            );
            let mut game = Game::new(
                catalog(),
                &setup(1, json!([])),
                &Value::Null,
                &Value::Null,
                "population-authority",
            )
            .unwrap();
            assert!(
                game.load_internal_card("unit-follower", &forged).is_err(),
                "unobserved private deck members cannot supply a public rule population"
            );
        }
    }
}

#[test]
fn resolution_population_is_frozen_before_effects_and_default_returns_when_provider_leaves() {
    let mut input = tree();
    let ability_tree = evolution_linked_movement::tree(&[(
        "activate",
        json!({"cost":{"pp":0_u8},"do":[
            {"if":{"cond":{"spellchain":1_u8},"then":[{"life":{"who":"self","delta":1_u8}}]}},
            {"move":{"ref":"this","to":"cemetery"}},
            {"if":{"cond":{"spellchain":1_u8},"then":[{"life":{"who":"self","delta":1_u8}}]}}
        ]}),
    )]);
    let mut activation = ability_tree["cards"]["unit-follower"]["abilities"][0].clone();
    activation["line"] = json!(2_u8);
    input["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(activation);
    let provider = lowerer()
        .lower(&input)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let id = provider.card().abilities[1].node.id;
    let mut observer_tree = ability_tree;
    let mut observer = observer_tree["cards"]["unit-follower"].take();
    observer["abilities"][0]["activate"]["do"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    observer_tree["cards"] = json!({"unit-amulet":observer});
    let observer_program = lowerer()
        .lower(&observer_tree)
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let observer_id = observer_program.card().abilities[0].node.id;
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"unit-follower"},{"id":"observer","card":"unit-amulet"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"grave","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = evolution_linked_movement::loaded(&initial, &provider);
    game.load_internal_card("unit-amulet", &observer_program)
        .unwrap();
    let before = game.projection(View::Referee).unwrap()["P1"]["leader"]["life"]
        .as_i64()
        .unwrap();
    resolved(
        game.activate_internal_card("source", id, PlayerId { seat: 1 })
            .unwrap(),
    );
    let frozen = game.projection(View::Referee).unwrap();
    assert_eq!(
        frozen["P1"]["leader"]["life"],
        before + 2,
        "both conditions retain the resolution-entry population although the provider left between them"
    );
    assert_eq!(frozen["P1"]["cemetery"], json!(["grave", "source"]));
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    resolved(
        game.activate_internal_card("observer", observer_id, PlayerId { seat: 1 })
            .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        before + 2,
        "a subsequent resolution uses the default spell-only population"
    );
}
