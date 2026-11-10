use super::*;
use sve_engine::Result;
use sve_engine::game::Step;

fn aura(who: &str) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut source = tree["cards"]["synthetic-evolution"].clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["static"] = json!({"effects":[{"unlimited_evolve":{"who":who}}]});
    source["abilities"].as_array_mut().unwrap().push(entry);
    tree["cards"] = json!({"unit-follower":source});
    tree
}

#[test]
fn field_unlimited_evolution_aura_lowers_to_the_existing_rule_modifier() {
    let programs = lowerer().lower(&aura("self")).unwrap();
    let program = &programs["unit-follower"];
    assert!(
        program.card().abilities.iter().any(|ability| {
            ability.node.declarations.iter().any(|effect| {
                matches!(
                    effect,
                    Effect::RuleModifier {
                        modifier: RuleModifier::EvolutionPlayLimit {
                            player: PlayerRef::Controller {}
                        },
                        ..
                    }
                )
            })
        }),
        "the printed aura uses the existing typed evolution limit modifier"
    );
}

fn fixture(provider_zone: &str, seat: &str, silenced: bool) -> (Game, AbilityId) {
    let mut tree = aura("self");
    let mut base = tree["cards"]["unit-follower"].clone();
    let declaration = base["abilities"].as_array_mut().unwrap().pop().unwrap();
    let mut provider = base.clone();
    let mut removal = declaration.clone();
    removal.as_object_mut().unwrap().remove("static");
    removal["activate"] = json!({"cost":{"self_to_cemetery":true},"do":[]});
    removal["line"] = json!(3);
    provider["abilities"] = json!([declaration, removal]);
    tree["cards"] = json!({"unit-follower":base,"unit-amulet":provider});
    let programs = lowerer().lower(&tree).unwrap();
    let removal_id = programs["unit-amulet"].card().abilities[1].node.id;
    let snapshot = [
        ("unit-follower", "フォロワー", "1", "2", "3"),
        ("unit-form", "フォロワー・エボルヴ", "-", "4", "5"),
        ("unit-amulet", "アミュレット", "1", "-", "-"),
    ]
    .iter()
    .map(|(number, kind, cost, power, hp)| {
        json!({"number":number,"faces":[{
            "name":if *number=="unit-form" {"unit-follower"}else{number},
            "card_class":"ニュートラル","card_type":kind,"cost":cost,
            "power":power,"hp":hp,"traits":[],"text":null,"sections":[]
        }]})
        .to_string()
    })
    .collect::<Vec<_>>()
    .join("\n");
    let document = json!({"format":1,"kind":"effect_set","cards":{
        "unit-follower":{"status":"complete","abilities":[]},
        "unit-form":{"status":"complete","abilities":[]},
        "unit-amulet":{"status":"complete","abilities":[]}
    }})
    .to_string();
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("unlimited.yaml".into(), document)])
            .unwrap(),
    );
    let mut initial = setup(1, json!([]));
    for player in ["P1", "P2"] {
        initial["players"][player]["zones"] = json!({"hand":[],"deck":[],"field":[],"ex":[],"cemetery":[],"banish":[],"evolve_deck":[]});
    }
    initial["turn"]["elapsed_turns"]["P1"] = json!(7);
    initial["players"]["P1"]["pp"] = json!({"current":5,"max":5});
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"s","card":"unit-follower"},{"id":"u","card":"unit-follower"},
        {"id":"v","card":"unit-follower"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let provider = json!({"id":"aura","card":"unit-amulet","state":{"silenced":silenced}});
    if let Some(cards) = initial["players"][seat]["zones"][provider_zone].as_array_mut() {
        cards.push(provider);
    } else {
        initial["players"][seat]["zones"][provider_zone] = json!([provider]);
    }
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"e","card":"unit-form"},{"id":"e2","card":"unit-form"},
        {"id":"e3","card":"unit-form"}
    ]);
    let mut game = Game::new(catalog, &initial, &Value::Null, &Value::Null, "unlimited").unwrap();
    for (number, program) in programs {
        game.load_internal_card(&number, &program).unwrap();
    }
    game.load_internal_card("unit-form", &card(vec![])).unwrap();
    (game, removal_id)
}

fn evolve(game: &mut Game, source: &str, form: &str) -> Result<Step> {
    game.internal_rule_action(
        &json!({"do":"evolve","source":source,"evolve_card":form,"pay":{"pp":1}}),
    )
}

fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

#[test]
fn native_field_aura_allows_repeated_intrinsic_evolution_without_refunding_each_payment() {
    let (mut game, _) = fixture("field", "P1", false);
    for (source, form) in [("s", "e"), ("u", "e2")] {
        assert_eq!(evolve(&mut game, source, form).unwrap().outcome, "resolved");
        checks(&mut game);
    }
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["pp"]["current"], 3,
        "each play still pays its own PP"
    );
    assert_eq!(view["objects"]["s"]["evolved_with"], "e");
    assert_eq!(view["objects"]["u"]["evolved_with"], "e2");
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(
        evolve(&mut game, "s", "e3").unwrap().outcome,
        "cannot-evolve"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "an already evolved object cannot evolve again"
    );
}

#[test]
fn another_player_nonfield_or_silenced_aura_never_relaxes_the_limit() {
    for (zone, seat, silenced) in [
        ("field", "P2", false),
        ("ex", "P1", false),
        ("cemetery", "P1", false),
        ("field", "P1", true),
    ] {
        let (mut game, _) = fixture(zone, seat, silenced);
        evolve(&mut game, "s", "e").unwrap();
        checks(&mut game);
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            evolve(&mut game, "u", "e2").unwrap().outcome,
            "cannot-evolve"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            before,
            "the second play fails before payment or state changes"
        );
    }
}

#[test]
fn removing_the_provider_restores_the_limit_and_preserves_the_used_turn_count() {
    let (mut game, id) = fixture("field", "P1", false);
    evolve(&mut game, "s", "e").unwrap();
    checks(&mut game);
    resolved(
        game.activate_internal_card("aura", id, PlayerId { seat: 1 })
            .unwrap(),
    );
    checks(&mut game);
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(
        evolve(&mut game, "u", "e2").unwrap().outcome,
        "cannot-evolve"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "leaving cannot reset the already used shared quota"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["aura"])
    );
}

#[test]
fn unlimited_declarations_reject_other_players_zones_conditions_and_mixed_effects() {
    for who in ["opp", "all"] {
        lowerer().lower(&aura(who)).unwrap_err();
    }
    for zone in ["ex", "cemetery", "hand", "deck"] {
        let mut tree = aura("self");
        tree["cards"]["unit-follower"]["abilities"][1]["static"]["zone"] = json!(zone);
        lowerer().lower(&tree).unwrap_err();
    }
    for effects in [
        json!([{"unlimited_evolve":{"who":"self"}},{"ignore_ward":{"on":"this"}}]),
        json!([{"ignore_ward":{"on":"this"}},{"unlimited_evolve":{"who":"self"}}]),
    ] {
        let mut tree = aura("self");
        tree["cards"]["unit-follower"]["abilities"][1]["static"]["effects"] = effects;
        lowerer().lower(&tree).unwrap_err();
    }
    let mut tree = aura("self");
    tree["cards"]["unit-follower"]["abilities"][1]["static"]["while"] =
        json!({"cmp":{"left":1,"op":"eq","right":1}});
    lowerer().lower(&tree).unwrap_err();
}

#[test]
fn raw_rule_modifiers_cannot_bypass_the_closed_printed_aura_boundary() {
    let programs = lowerer().lower(&aura("self")).unwrap();
    for change in 0..4 {
        let mut candidate = programs["unit-follower"].card().clone();
        let ability = &mut candidate.abilities[1].node;
        match change {
            0 => ability.active_zones = vec![ZoneKind::Ex],
            1 => {
                if let Effect::RuleModifier { modifier, .. } = &mut ability.declarations[0] {
                    *modifier = RuleModifier::EvolutionPlayLimit {
                        player: PlayerRef::Owner {},
                    };
                }
            }
            2 => {
                if let Effect::RuleModifier { condition, .. } = &mut ability.declarations[0] {
                    *condition = Expr::Literal(Bool(false));
                }
            }
            _ => {
                if let Effect::RuleModifier { source, .. } = &mut ability.declarations[0] {
                    source.origin = AbilityOrigin::Printed {
                        face: FaceId("0".into()),
                        line: Nat(99),
                    };
                }
            }
        }
        WellFormedCardProgram::check(candidate).unwrap_err();
    }
}
