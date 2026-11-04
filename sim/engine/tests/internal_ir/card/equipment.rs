use super::*;

fn equip_action() -> Action {
    Action::Equip {
        holder: ObjectOperand::SourceHolder {},
        template: TokenTemplateId("unit-spell".into()),
        count: Nat(1),
    }
}

pub(super) fn equipment_catalog(kind: &str) -> Arc<Catalog> {
    let (snapshot, keywords, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "unit-spell" {
                row["faces"][0]["card_type"] = json!(kind);
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    Arc::new(
        Catalog::from_documents(&snapshot, &keywords, &[("unit.yaml".into(), document)]).unwrap(),
    )
}

fn board(kind: &str) -> Game {
    board_with_token(kind, true)
}

fn board_with_token(kind: &str, admitted: bool) -> Game {
    let catalog = equipment_catalog(kind);
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"s","card":"unit-follower"}, {"id":"a","card":"unit-amulet"},
        {"id":"b","card":"unit-amulet"}, {"id":"c","card":"unit-amulet"}, {"id":"d","card":"unit-amulet"}
    ]);
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "equipment-seed",
    )
    .unwrap();
    if admitted {
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
    }
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    game
}

#[test]
fn equipment_uses_its_own_zone_without_a_field_capacity_limit_and_cleans_up_with_its_host() {
    let mut game = board("イクイップメント・トークン");
    let leave = Action::Move {
        objects: Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        },
        movement: MoveSpec {
            to: MoveDestination::Owners {
                zone: OwnerZone::Cemetery,
            },
            position: None,
            order: None,
            initial_posture: None,
            entry_effects: vec![],
        },
        reason: MoveReason::Put,
    };
    game.load_internal_card(
        "unit-follower",
        &card(vec![
            ability(1, vec![instruction(1, equip_action())]),
            ability(2, vec![instruction(2, leave)]),
        ]),
    )
    .unwrap();
    for _ in 0..2 {
        let result = activate(&mut game, 1);
        assert!(result.progress.receipts.values().any(
            |receipt| matches!(receipt, InstructionOutcome::Equipped(events) if events.len() == 1)
        ));
        assert!(
            result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Equipped(_)))
        );
        game.resume_internal_card().unwrap();
    }
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"].as_array().unwrap().len(), 5);
    let equipment = view["P1"]["equipment"].as_array().unwrap().clone();
    assert_eq!(equipment.len(), 2);
    resolved(
        game.activate_internal_card("s", AbilityId(2), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let departed = game.projection(View::Referee).unwrap();
    assert_eq!(departed["P1"]["equipment"], json!([]));
    for id in equipment {
        assert!(departed["objects"][id.as_str().unwrap()].is_null());
    }
}

#[test]
fn ordinary_tokens_and_arbitrary_equipment_counts_remain_rejected() {
    let mut game = board("フォロワー・トークン");
    assert!(
        game.load_internal_card(
            "unit-follower",
            &card(vec![ability(1, vec![instruction(1, equip_action())])])
        )
        .is_err()
    );
    let mut equipment = board("イクイップメント・トークン");
    let invalid = Action::Equip {
        holder: ObjectOperand::SourceHolder {},
        template: TokenTemplateId("unit-spell".into()),
        count: Nat(2),
    };
    assert!(
        equipment
            .load_internal_card(
                "unit-follower",
                &card(vec![ability(1, vec![instruction(1, invalid)])])
            )
            .is_err()
    );
}

fn lowering_tree(head: &Value, number: &str) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry
        .as_object_mut()
        .unwrap()
        .extend(head.as_object().unwrap().clone());
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({number:{"meta":meta,"abilities":[entry]}});
    tree
}

fn lowered(head: &Value, number: &str) -> WellFormedCardProgram {
    lowerer()
        .lower(&lowering_tree(head, number))
        .unwrap()
        .remove(number)
        .unwrap()
}

#[test]
fn multiple_equipment_providers_grant_distinct_activated_instances_to_the_host() {
    let mut game = board_with_token("イクイップメント・トークン", false);
    let equipment = lowered(
        &json!({"static":{"zone":"equipment","effects":[{"grant":{"on":"host","abilities":[{"activate":{"cost":{"act_self":true},"do":[{"buff":{"on":"this","power":1,"hp":1}}]}}]}}]}}),
        "unit-spell",
    );
    let mut unsupported = equipment.card().clone();
    if let Effect::Continuous {
        target:
            EffectTarget::Dynamic {
                selector: Selector::Relation { relation, .. },
            },
        ..
    } = &mut unsupported.abilities[0].node.declarations[0]
    {
        *relation = ObjectRelation::Provider;
    }
    assert!(
        WellFormedCardProgram::check(unsupported).is_err(),
        "equipment does not open unrelated relation selectors"
    );
    game.load_internal_card("unit-spell", &equipment).unwrap();
    let leave = Action::Move {
        objects: Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        },
        movement: MoveSpec {
            to: MoveDestination::Owners {
                zone: OwnerZone::Cemetery,
            },
            position: None,
            order: None,
            initial_posture: None,
            entry_effects: vec![],
        },
        reason: MoveReason::Put,
    };
    game.load_internal_card(
        "unit-follower",
        &card(vec![
            ability(1, vec![instruction(1, equip_action())]),
            ability(2, vec![instruction(2, leave)]),
        ]),
    )
    .unwrap();
    for _ in 0..2 {
        activate(&mut game, 1);
        game.resume_internal_card().unwrap();
    }
    let granted = game
        .internal_activated_sources("s")
        .unwrap()
        .into_iter()
        .filter(|source| ![AbilityId(1), AbilityId(2)].contains(&source.definition))
        .collect::<Vec<_>>();
    assert_eq!(
        granted.len(),
        2,
        "each equipped token supplies its own activated ability"
    );
    assert_ne!(granted[0].instance, granted[1].instance);
    let before = exact_authority(&game);
    assert!(
        game.activate_internal_card("s", granted[0].definition, PlayerId { seat: 1 })
            .is_err(),
        "a duplicate definition requires selection by instance"
    );
    assert_eq!(exact_authority(&game), before);
    resolved(
        game.activate_internal_card_instance("s", granted[0].instance, PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["objects"]["s"]["acted"], true,
        "the follower pays its own posture cost"
    );
    assert_eq!(view["objects"]["s"]["power"], 3);
    let paid = exact_authority(&game);
    assert!(
        game.activate_internal_card_instance("s", granted[1].instance, PlayerId { seat: 1 })
            .is_err(),
        "the second provider cannot pay an already acted host"
    );
    assert_eq!(exact_authority(&game), paid);
    resolved(
        game.activate_internal_card("s", AbilityId(2), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    assert!(
        game.activate_internal_card_instance("s", granted[0].instance, PlayerId { seat: 1 })
            .is_err(),
        "a departed provider no longer grants a usable instance"
    );
}

#[test]
fn activated_definitions_admit_installed_grants_but_keep_field_auras_closed() {
    let definition =
        json!({"activate":{"cost":{"act_self":true},"do":[{"life":{"who":"self","delta":1}}]}});
    for (installed, positive, definition_grant) in [
        (
            false,
            json!({"static":{"effects":[{"grant":{"on":"this","keywords":["ward"]}}]}}),
            json!({"static":{"effects":[{"grant":{"on":"this","abilities":[definition]}}]}}),
        ),
        (
            true,
            json!({"activate":{"cost":{"pp":0},"do":[{"grant":{"on":"this","keywords":["ward"]}}]}}),
            json!({"activate":{"cost":{"pp":0},"do":[{"grant":{"on":"this","abilities":[definition]}}]}}),
        ),
    ] {
        let mut game = board("イクイップメント・トークン");
        game.load_internal_card("unit-follower", &lowered(&positive, "unit-follower"))
            .unwrap();
        let mut candidate = board("イクイップメント・トークン");
        let before = exact_authority(&candidate);
        let result = candidate.load_internal_card(
            "unit-follower",
            &lowered(&definition_grant, "unit-follower"),
        );
        if installed {
            result.unwrap();
        } else {
            assert!(result.is_err());
            assert_eq!(
                exact_authority(&candidate),
                before,
                "field activation auras remain closed"
            );
        }
    }
}

#[test]
fn an_equipment_template_must_be_admitted_before_the_source_can_create_it() {
    let mut game = board_with_token("イクイップメント・トークン", false);
    let program = card(vec![ability(1, vec![instruction(1, equip_action())])]);
    let before = exact_authority(&game);
    assert!(game.load_internal_card("unit-follower", &program).is_err());
    assert_eq!(exact_authority(&game), before);
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    assert!(
        activate(&mut game, 1)
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Equipped(_)))
    );
}

#[test]
fn equipment_follows_its_hosts_controller_and_dangling_equipment_is_erased() {
    for attached in [true, false] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"]["P2"]["zones"]["equipment"] = json!([{"id":"e","card":"unit-spell","state":{"equipped_to":if attached {"s"} else {"missing"}}}]);
        let mut game = Game::new(
            equipment_catalog("イクイップメント・トークン"),
            &initial,
            &Value::Null,
            &Value::Null,
            "equipment-rules",
        )
        .unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        game.load_internal_card("unit-follower", &card(vec![ability(1, vec![])]))
            .unwrap();
        activate(&mut game, 1);
        game.resume_internal_card().unwrap();
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P2"]["equipment"], json!([]));
        if attached {
            assert_eq!(view["P1"]["equipment"], json!(["e"]));
            assert_eq!(view["P1"]["field"], json!(["s"]));
        } else {
            assert_eq!(view["P1"]["equipment"], json!([]));
            assert!(view["objects"]["e"].is_null());
        }
    }
}

fn sword_program() -> WellFormedCardProgram {
    let head = json!({"static":{"zone":"equipment","effects":[{"damage_mod":{"source":"host","delta":2}},{"damage_mod":{"on":"host","delta":-2}}]}});
    lowered(&head, "unit-spell")
}

#[test]
fn equipment_damage_modifiers_belong_to_the_equipment_and_modify_both_combat_directions() {
    for (host_silenced, equipment_silenced) in [(false, false), (true, false), (false, true)] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"s","card":"unit-follower","state":{"silenced":host_silenced}}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"t","card":"unit-follower","state":{"acted":true,"hp":10,"max_hp":10}}]);
        initial["players"]["P1"]["zones"]["equipment"] = json!([{"id":"e","card":"unit-spell","state":{"equipped_to":"s","silenced":equipment_silenced}}]);
        let mut game = Game::new(
            equipment_catalog("イクイップメント・トークン"),
            &initial,
            &Value::Null,
            &Value::Null,
            "equipment-damage",
        )
        .unwrap();
        game.load_internal_card("unit-spell", &sword_program())
            .unwrap();
        game.load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
        game.internal_rule_action(&json!({"do":"attack","attacker":"s","target":"t"}))
            .unwrap();
        let question = game.internal_quick_choice().unwrap().unwrap();
        game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["objects"]["t"]["damage"],
            if equipment_silenced { 2_u8 } else { 4_u8 }
        );
        assert_eq!(
            view["objects"]["s"]["damage"],
            if equipment_silenced { 2_u8 } else { 0_u8 }
        );
    }
}

#[test]
fn equipment_damage_modifiers_do_not_open_ordinary_host_names_or_non_host_targets() {
    let field = lowering_tree(
        &json!({"static":{"effects":[{"damage_mod":{"source":"host","delta":2}}]}}),
        "unit-spell",
    );
    assert!(
        lowerer().lower(&field).is_err(),
        "host is scoped to an equipment static head"
    );
    let mut supported = board_with_token("イクイップメント・トークン", false);
    supported
        .load_internal_card("unit-spell", &sword_program())
        .unwrap();
    for effect in [
        json!({"damage_mod":{"on":"this","delta":-2}}),
        json!({"damage_mod":{"on":{"leader":"self"},"delta":-2}}),
        json!({"damage_mod":{"source":"host","delta":2,"kind":"ability"}}),
    ] {
        let program = lowered(
            &json!({"static":{"zone":"equipment","effects":[effect]}}),
            "unit-spell",
        );
        let mut game = board_with_token("イクイップメント・トークン", false);
        assert!(game.load_internal_card("unit-spell", &program).is_err());
    }
}
