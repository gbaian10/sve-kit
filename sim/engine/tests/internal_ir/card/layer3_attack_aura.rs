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

#[test]
fn an_attack_limit_counts_each_follower_through_refresh_and_resets_next_turn() {
    let document = evolution_linked_movement::tree(&[
        (
            "static",
            json!({
                "while":{"cmp":{
                    "left":{"history_count":{"event":"attacked","window":"this_turn","where":{"ref":"this"}}},
                    "op":"ge","right":1
                }},
                "effects":[{"cannot":{"what":"attack","on":"this"}}]
            }),
        ),
        ("activate", json!({"cost":{"pp":0},"do":[{"stand":"this"}]})),
        (
            "activate",
            json!({"cost":{"pp":0},"do":[
                {"move":{"ref":"this","to":"hand"}},
                {"life":{"who":"self","delta":{"history_count":{
                    "event":"attacked","window":"this_turn","where":{"ref":"this"}
                }}}}
            ]}),
        ),
    ]);
    let program = lowerer()
        .lower(&document)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([]));
    for player in ["P1", "P2"] {
        initial["players"][player]["zones"]["hand"] = json!([]);
        initial["players"][player]["zones"]["deck"] =
            json!([{"id":format!("draw-{player}"),"card":"unit-spell"}]);
    }
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"first","card":"unit-follower"},
        {"id":"second","card":"unit-follower"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "attacks-by-source",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    let attack = |game: &mut Game, attacker: &str| {
        game.internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":"P2.leader"}))
            .unwrap()
            .outcome
    };
    assert_eq!(attack(&mut game, "first"), "resolved");
    quick::pass(&mut game);
    game.advance_player_input();
    let refresh = program.card().abilities[1].node.id;
    resolved(
        game.activate_internal_card("first", refresh, PlayerId { seat: 1 })
            .unwrap(),
    );
    game.advance_player_input();
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["first"]["acted"],
        false
    );
    assert_eq!(attack(&mut game, "first"), "cannot-attack");
    assert_eq!(attack(&mut game, "second"), "resolved");
    quick::pass(&mut game);
    game.advance_player_input();
    let return_source = program.card().abilities[2].node.id;
    let life_before = game.projection(View::Referee).unwrap()["P1"]["leader"]["life"].clone();
    resolved(
        game.activate_internal_card("first", return_source, PlayerId { seat: 1 })
            .unwrap(),
    );
    game.advance_player_input();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], life_before);
    assert_eq!(state["P1"]["hand"], json!(["first"]));
    game.play_internal_card("first", PlayerId { seat: 1 })
        .unwrap();
    game.advance_player_input();
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["first"]["generation"],
        3
    );
    boxed::end_turn(&mut game);
    boxed::end_turn(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P1"
    );
    assert_eq!(attack(&mut game, "first"), "resolved");
}

#[test]
fn source_attack_count_rejects_other_history_filters() {
    for (field, value) in [
        ("event", json!("evolved")),
        ("who", json!("self")),
        ("sel", json!({"kind":"follower"})),
        ("from", json!("field")),
        ("to", json!("cemetery")),
        ("at_least", json!(1)),
        ("exclude", json!("this_execution")),
        ("distinct", json!("subjects")),
        ("where", json!({"ref":"this","during":{"turn":"self"}})),
        ("where", json!({"ref":"chosen"})),
    ] {
        let mut query = json!({"event":"attacked","window":"this_turn","where":{"ref":"this"}});
        query[field] = value;
        let document = evolution_linked_movement::tree(&[(
            "static",
            json!({
                "while":{"cmp":{"left":{"history_count":query},"op":"ge","right":1}},
                "effects":[{"cannot":{"what":"attack","on":"this"}}]
            }),
        )]);
        assert!(
            lowerer().lower(&document).is_err(),
            "{field} must be rejected"
        );
    }
}

fn owner_attack_target() -> Value {
    json!({"who":{"owner_of":"this"},"kind":"follower","leader_or":true,"zone":"field"})
}

#[test]
fn a_follower_cannot_attack_its_owners_leader_or_their_field_after_transfer() {
    let input = evolution_linked_movement::tree(&[(
        "static",
        json!({"effects":[{"cannot":{"what":"attack","target":owner_attack_target()}}]}),
    )]);
    let program = boxed::program(&input);
    for seat in [1, 2] {
        let mut initial = setup(seat, json!([]));
        let (active, enemy) = if seat == 1 {
            ("P1", "P2")
        } else {
            ("P2", "P1")
        };
        initial["turn"]["active"] = json!(active);
        initial["players"][active]["zones"]["field"] = json!([
            {"id":"owner-test-source","card":"unit-follower","owner":"P1","state":{"entered_this_turn":false}},
            {"id":"owner-test-control","card":"unit-follower","owner":active,"state":{"entered_this_turn":false}}
        ]);
        initial["players"][enemy]["zones"]["field"] = json!([
            {"id":"owner-test-enemy","card":"unit-follower","owner":"P1","state":{"acted":true}}
        ]);
        let mut game = evolution_linked_movement::loaded(&initial, &program);
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        let state = game.projection(View::Referee).unwrap();
        let targets = state["legal"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|action| action["do"] == "attack" && action["attacker"] == "owner-test-source")
            .map(|action| action["target"].as_str().unwrap())
            .collect::<Vec<_>>();
        let available = state["legal"].as_array().unwrap();
        for target in [format!("{enemy}.leader"), "owner-test-enemy".to_owned()] {
            assert!(
                available.iter().any(|action| {
                    action["do"] == "attack"
                        && action["attacker"] == "owner-test-control"
                        && action["target"] == target
                }),
                "the ordinary follower can still attack {target}"
            );
        }
        if seat == 1 {
            assert!(
                targets.contains(&"P2.leader"),
                "the source can normally attack the other leader"
            );
            assert!(
                targets.contains(&"owner-test-enemy"),
                "a follower on the enemy field remains attackable even if its owner is the source's owner"
            );
        } else {
            assert!(
                targets.is_empty(),
                "the transferred source cannot attack its owner's leader or followers controlled by its owner"
            );
        }
    }
}

#[test]
fn the_owner_field_attack_restriction_only_applies_to_the_printed_source() {
    for effect in [
        json!({"cannot":{"what":"attack","on":{"who":"self","zone":"field","kind":"follower"},"target":owner_attack_target()}}),
        json!({"cannot":{"what":"attack","target":owner_attack_target(),"if_can_attack":"follower"}}),
    ] {
        let input = evolution_linked_movement::tree(&[("static", json!({"effects":[effect]}))]);
        lowerer().lower(&input).unwrap_err();
    }
    let input = evolution_linked_movement::tree(&[(
        "activate",
        json!({"cost":{"pp":0},"do":[{"apply":{"on":"this","until":"turn","effect":
            {"cannot":{"what":"attack","target":owner_attack_target()}}}}]}),
    )]);
    lowerer().lower(&input).unwrap_err();
}
