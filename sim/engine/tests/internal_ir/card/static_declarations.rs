use super::*;
use alloc::collections::BTreeSet;
use core::slice::from_ref;
use sve_engine::game::internal::InputDomain;

fn effects() -> [Value; 3] {
    [
        json!({"grant":{"keywords":["storm"]}}),
        json!({"prevent":{"on":{"leader":"self"},"kind":"ability"}}),
        json!({"cost_mod":{"when_playing":{"who":"self","kind":"spell"},"delta":-1}}),
    ]
}

fn input(effects: &[Value], conditional: bool) -> Value {
    let mut head = json!({"static":{"effects":effects}});
    if conditional {
        head["static"]["while"] = json!({"awakened":true});
    }
    equipment_played::tree(&head, "unit-follower")
}

fn program(effects: &[Value], conditional: bool) -> WellFormedCardProgram {
    lowerer()
        .lower(&input(effects, conditional))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn initial(maximum: u8) -> Value {
    let mut position = setup(1, json!([]));
    position["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
    position["players"]["P1"]["zones"]["field"] = json!([{"id":"provider","card":"unit-follower"}]);
    position["players"]["P2"]["zones"]["field"] = json!([]);
    for player in ["P1", "P2"] {
        position["players"][player]["zones"]["deck"] = json!([
            {"id":format!("deck-1-{player}"),"card":"unit-spell"},
            {"id":format!("deck-2-{player}"),"card":"unit-spell"}
        ]);
    }
    position
}

fn unloaded(position: &Value) -> Game {
    let (snapshot, _, document) = catalog_documents(&json!("3"));
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../authored/rules/keywords.yaml"
            )),
            &[("unit.yaml".into(), document)],
        )
        .unwrap(),
    );
    Game::new(
        catalog,
        position,
        &Value::Null,
        &Value::Null,
        "mixed-static",
    )
    .unwrap()
}

#[expect(
    clippy::panic,
    reason = "The synthetic spell must retain its ordered intrinsic price."
)]
fn spell() -> WellFormedCardProgram {
    let mut ability = play::spell(vec![damage(
        1,
        RecipientSet::Leaders {
            players: players(PlayerRelation::SelfPlayer),
        },
        2,
    )]);
    let CostSpec::Ordered { steps } = &mut ability.node.play.as_mut().unwrap().costs else {
        panic!("ordered spell price")
    };
    steps[0].instruction.action = payment::pp(3);
    card(vec![ability])
}

fn board(program: &WellFormedCardProgram, maximum: u8) -> Game {
    let mut game = unloaded(&initial(maximum));
    game.load_internal_card("unit-follower", program).unwrap();
    game.load_internal_card("unit-spell", &spell()).unwrap();
    game
}

fn keywords(game: &Game) -> BTreeSet<String> {
    serde_json::from_value::<Vec<String>>(
        game.projection(View::Referee).unwrap()["objects"]["provider"]["keywords"].clone(),
    )
    .unwrap()
    .into_iter()
    .collect()
}

fn assert_effects(game: &Game, active: bool, later_bane: bool) {
    let mut expected = BTreeSet::new();
    if active {
        expected.insert("疾走".to_owned());
    }
    if later_bane {
        expected.insert("必殺".to_owned());
    }
    assert_providers(
        game,
        &expected,
        if active { 2 } else { 3 },
        if active { 0 } else { 2 },
    );
}

fn assert_providers(game: &Game, expected: &BTreeSet<String>, price: u64, damage: i64) {
    assert_eq!(
        &keywords(game),
        expected,
        "only the active source declarations grant keywords"
    );
    let mut paid = game.clone();
    let before = paid.projection(View::Referee).unwrap();
    let life = before["P1"]["leader"]["life"].as_i64().unwrap();
    let pp = before["P1"]["pp"]["current"].as_u64().unwrap();
    resolved(paid.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
    let after = paid.projection(View::Referee).unwrap();
    assert_eq!(
        after["P1"]["pp"]["current"],
        pp.checked_sub(price).unwrap(),
        "the three-PP spell receives exactly one discount when active"
    );
    assert_eq!(
        after["P1"]["leader"]["life"],
        life.checked_sub(damage).unwrap(),
        "the source's prevention independently cancels two ability damage"
    );
}

#[test]
fn all_three_static_providers_keep_each_declaration_in_every_input_order() {
    let effects = effects();
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let selected = order.map(|index| effects[index].clone());
        let program = program(&selected, true);
        assert_eq!(
            program.card().abilities.len(),
            1,
            "one printed ability remains one ability"
        );
        let ability = &program.card().abilities[0].node;
        assert_eq!(
            ability.declarations.len(),
            3,
            "no provider loses a declaration"
        );
        for (declaration, expected) in ability.declarations.iter().zip(order) {
            match declaration {
                Effect::Continuous { source, .. } => {
                    assert_eq!(expected, 0);
                    assert_eq!(source, &ability.source);
                }
                Effect::Replacement { source, .. } => {
                    assert_eq!(expected, 1);
                    assert_eq!(source, &ability.source);
                }
                Effect::CostModifier { .. } => assert_eq!(expected, 2),
                Effect::Permission { .. }
                | Effect::RuleModifier { .. }
                | Effect::DeckRule { .. } => {
                    panic!("only the three declared families are expected")
                }
            }
        }
        for maximum in [6, 7] {
            assert_effects(&board(&program, maximum), maximum == 7, false);
        }
    }
}

#[expect(
    clippy::panic,
    reason = "Any question other than the declared public target invalidates the fixture."
)]
fn activate_emitter(game: &mut Game, program: &WellFormedCardProgram, line: u32) {
    let ability = program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == line)
        .unwrap();
    let mut result = game
        .activate_internal_card("emitter", ability.node.id, PlayerId { seat: 1 })
        .unwrap();
    while let CardPlay::Awaiting(question) = result {
        let InputDomain::Targets(targets) = question.domain else {
            panic!("one public target")
        };
        assert_eq!(
            targets.members.len(),
            1,
            "the emitter has exactly one public follower recipient"
        );
        result = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![targets.members[0].handle]),
            )
            .unwrap();
    }
    resolved(result);
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

#[test]
fn suppression_stops_all_three_providers_but_preserves_a_later_grant_and_restores_once() {
    let provider = program(&effects(), false);
    let target =
        json!({"bind":"recipient","from":{"who":"self","zone":"field","kind":"follower"},"n":1});
    let mut tree = equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0},"targets":[target],"do":[
            {"apply":{"on":"recipient","effect":{"lose_abilities":{}},"until":"turn"}}
        ]}}),
        "unit-amulet",
    );
    tree["cards"]["unit-amulet"]["abilities"][0]["line"] = json!(1);
    let mut grant = tree["cards"]["unit-amulet"]["abilities"][0].clone();
    grant["line"] = json!(2);
    grant["activate"]["do"] =
        json!([{"grant":{"on":"recipient","keywords":["bane"],"until":"game"}}]);
    tree["cards"]["unit-amulet"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(grant);
    let emitter = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let mut position = initial(7);
    position["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"emitter","card":"unit-amulet"}));
    let mut game = unloaded(&position);
    game.load_internal_card("unit-follower", &provider).unwrap();
    game.load_internal_card("unit-amulet", &emitter).unwrap();
    game.load_internal_card("unit-spell", &spell()).unwrap();
    assert_effects(&game, true, false);
    activate_emitter(&mut game, &emitter, 1);
    assert_effects(&game, false, false);
    activate_emitter(&mut game, &emitter, 2);
    assert_effects(&game, false, true);
    for _ in 0..2 {
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        quick::pass(&mut game);
    }
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P1"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["provider"]["generation"],
        0
    );
    assert_effects(&game, true, true);
}

fn private_condition(scope: ScopeId) -> Expr<Bool> {
    let binding = Binding::new(99, scope);
    Expr::Compute(Box::new(Predicate::Exists {
        objects: Selector::Zone {
            players: players(PlayerRelation::Opponents),
            zone: ZoneKind::Hand,
            view: SelectionView::Current {},
            binding,
            predicate: Box::new(Expr::Compute(Box::new(Predicate::NamesIntersect {
                left: Expr::Live(Box::new(NamesRead::Names {
                    object: ObjectView::Current {
                        object: Box::new(ObjectOperand::Bound { binding }),
                    },
                })),
                right: Expr::Literal(vec![CardName("private".into())]),
            }))),
        },
    }))
}

#[test]
fn every_mixed_provider_still_rejects_a_private_read_after_a_valid_prefix() {
    let effects = effects();
    for order in [[1, 2, 0], [2, 0, 1], [0, 1, 2]] {
        let selected = order.map(|index| effects[index].clone());
        let mut raw = program(&selected, false).card().clone();
        let private = private_condition(raw.abilities[0].node.scope);
        match raw.abilities[0].node.declarations.last_mut().unwrap() {
            Effect::Continuous { condition, .. } => *condition = Some(private),
            Effect::Replacement { condition, .. } => *condition = private,
            Effect::CostModifier { duration, .. } => {
                duration.end = DurationEnd::While { predicate: private }
            }
            Effect::Permission { .. } | Effect::RuleModifier { .. } | Effect::DeckRule { .. } => {
                panic!("three providers")
            }
        }
        let checked = WellFormedCardProgram::check(raw).unwrap();
        unloaded(&initial(7))
            .load_internal_card("unit-follower", &checked)
            .unwrap_err();
    }
}

#[test]
fn wellformed_checks_later_mixed_declarations_and_cross_family_effect_ids() {
    let mut raw = program(&effects(), false).card().clone();
    let scope = raw.abilities[0].node.scope;
    let Effect::CostModifier { subjects, .. } =
        raw.abilities[0].node.declarations.last_mut().unwrap()
    else {
        panic!("cost")
    };
    *subjects = Selector::Bound {
        reference: Binding::new(99, scope),
    };
    WellFormedCardProgram::check(raw).unwrap_err();
    let mut raw = program(&effects(), false).card().clone();
    let Effect::Continuous { id: first, .. } = raw.abilities[0].node.declarations[0] else {
        panic!("grant")
    };
    let Effect::Replacement { id, .. } = &mut raw.abilities[0].node.declarations[1] else {
        panic!("prevention")
    };
    *id = first;
    WellFormedCardProgram::check(raw).unwrap_err();
}

#[test]
fn mixed_providers_do_not_add_families_or_relax_family_lifetime_checks() {
    let effects = effects();
    let alias = json!({"also_named":{"name":"alias"}});
    program(from_ref(&alias), false);
    for selected in [
        vec![effects[0].clone(), alias.clone()],
        vec![alias, effects[0].clone()],
    ] {
        lowerer().lower(&input(&selected, false)).unwrap_err();
    }
    let mut raw = program(&effects, false).card().clone();
    let Effect::Continuous { duration, .. } = &mut raw.abilities[0].node.declarations[0] else {
        panic!("grant")
    };
    duration.end = DurationEnd::CurrentTurnEnd {};
    WellFormedCardProgram::check(raw).unwrap_err();
}

#[test]
fn equipment_mixes_only_existing_unconditional_host_grants_and_damage_modifiers() {
    let grant = json!({"grant":{"on":"host","keywords":["storm"]}});
    let reduction = json!({"damage_mod":{"on":"host","delta":-1}});
    let (snapshot, _, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "unit-amulet" {
                row["faces"][0]["card_type"] = json!("イクイップメント・トークン");
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../authored/rules/keywords.yaml"
            )),
            &[("unit.yaml".into(), document)],
        )
        .unwrap(),
    );
    for pair in [[grant.clone(), reduction.clone()], [reduction, grant]] {
        let head = json!({"static":{"zone":"equipment","effects":pair}});
        let program = lowerer()
            .lower(&equipment_played::tree(&head, "unit-amulet"))
            .unwrap()
            .remove("unit-amulet")
            .unwrap();
        let mut position = initial(7);
        position["players"]["P1"]["zones"]["equipment"] =
            json!([{"id":"equipment","card":"unit-amulet","state":{"equipped_to":"provider"}}]);
        let mut game = Game::new(
            Arc::clone(&catalog),
            &position,
            &Value::Null,
            &Value::Null,
            "mixed-equipment",
        )
        .unwrap();
        game.load_internal_card("unit-amulet", &program).unwrap();
        game.load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
        game.load_internal_card(
            "unit-spell",
            &card(vec![play::spell(vec![damage(
                1,
                RecipientSet::Objects {
                    objects: zone(ZoneKind::Field, 99),
                },
                2,
            )])]),
        )
        .unwrap();
        assert_eq!(keywords(&game), BTreeSet::from(["疾走".to_owned()]));
        resolved(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["provider"]["damage"],
            1
        );
        let mut conditional = head.clone();
        conditional["static"]["while"] = json!({"awakened":true});
        lowerer()
            .lower(&equipment_played::tree(&conditional, "unit-amulet"))
            .unwrap_err();
        let mut cost = head;
        cost["static"]["effects"]
            .as_array_mut()
            .unwrap()
            .push(effects()[2].clone());
        lowerer()
            .lower(&equipment_played::tree(&cost, "unit-amulet"))
            .unwrap_err();
    }
}

#[test]
fn a_grant_branch_condition_does_not_gate_the_other_providers_in_the_same_ability() {
    let mut effects = effects();
    effects[0] = json!({"if":{"cond":{"leader_life":{"who":"self","le":19}},"then":[effects[0]]}});
    let program = program(&effects, true);
    let game = board(&program, 7);
    assert_providers(&game, &BTreeSet::new(), 2, 0);
}

#[test]
fn a_mixed_static_source_changes_its_actual_evolution_payment() {
    let mut effects = effects();
    effects[2] = json!({"evolve_cost_mod":{"on":"this","delta":-1}});
    let mut tree = input(&effects, false);
    let intrinsic =
        fanfare::expanded::tree()["cards"]["synthetic-evolution"]["abilities"][0].clone();
    tree["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .insert(0, intrinsic);
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let (mut snapshot, _, document) = catalog_documents(&json!("1"));
    snapshot.push('\n');
    snapshot.push_str(&json!({"number":"unit-form","faces":[{"name":"unit-follower","card_class":"ニュートラル","card_type":"フォロワー・エボルヴ","cost":"-","power":"4","hp":"5","traits":[],"text":null,"sections":[]}]}).to_string());
    let mut document: Value = serde_json::from_str(&document).unwrap();
    document["cards"]["unit-form"] = json!({"status":"complete","abilities":[]});
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../authored/rules/keywords.yaml"
            )),
            &[("unit.yaml".into(), document.to_string())],
        )
        .unwrap(),
    );
    let mut position = initial(7);
    position["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"unit-form"}]);
    let mut game = Game::new(
        catalog,
        &position,
        &Value::Null,
        &Value::Null,
        "mixed-evolution-cost",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    game.load_internal_card("unit-form", &card(vec![])).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    assert_eq!(game.internal_rule_action(&json!({"do":"evolve","source":"provider","evolve_card":"form","pay":{"pp":0,"ep":0,"sep":0}})).unwrap().outcome, "resolved");
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["pp"]["current"], 7);
    assert_eq!(after["objects"]["provider"]["evolved_with"], "form");
}

#[test]
fn repeated_public_selectors_keep_distinct_bindings_in_one_mixed_ability() {
    let mut effects = effects().to_vec();
    let recipients = json!({"who":"self","zone":"field","name":"unit-follower"});
    effects[0]["grant"]["on"] = recipients.clone();
    effects.push(json!({"grant":{"on":recipients,"keywords":["bane"]}}));
    let program = program(&effects, false);
    assert_eq!(program.card().abilities.len(), 1);
    assert_eq!(program.card().abilities[0].node.declarations.len(), 4);
    assert_effects(&board(&program, 7), true, true);
}

#[path = "provider_reads.rs"]
mod provider_reads;
