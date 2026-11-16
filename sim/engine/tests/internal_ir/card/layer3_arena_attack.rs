use super::*;
use sve_engine::EngineFailure;

fn input() -> Value {
    evolution_linked_movement::tree(&[(
        "static",
        json!({"effects":[{"cannot":{
            "what":"attack","on":{"who":"all","zone":"field","kind":"follower"},
            "target":{"leaders":"all"},"if_can_attack":"follower"
        }}]}),
    )])
}

#[test]
fn a_legal_follower_target_prevents_the_leader_attack_without_requiring_an_attack() {
    let programs = lowerer().lower(&input()).unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"attacker","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"unit-follower","state":{"acted":true}}]);
    let mut board = evolution_linked_movement::loaded(&initial, &programs["unit-follower"]);
    assert_eq!(
        board
            .internal_rule_action(
                &json!({"do":"attack","attacker":"attacker","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
        "cannot-attack"
    );
    assert_eq!(
        board
            .internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved"
    );
}

fn arena_program() -> WellFormedCardProgram {
    let mut value = input();
    let source = value["cards"]["unit-follower"].take();
    value["cards"] = json!({"unit-amulet":source});
    lowerer()
        .lower(&value)
        .unwrap()
        .remove("unit-amulet")
        .unwrap()
}

fn follower(number: &str, keys: &[&str]) -> WellFormedCardProgram {
    if keys.is_empty() {
        return card(vec![]);
    }
    let mut value = evolution_linked_movement::tree(&[(
        "static",
        json!({"effects":[{"grant":{"on":"this","keywords":keys}}]}),
    )]);
    let source = value["cards"]["unit-follower"].take();
    value["cards"] = json!({number:source});
    lowerer().lower(&value).unwrap().remove(number).unwrap()
}

fn arena_catalog() -> Arc<Catalog> {
    let (snapshot, registry, document) = catalog_documents(&json!("1"));
    let mut rows = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let mut extra = rows
        .iter()
        .find(|row| row["number"] == "unit-follower")
        .unwrap()
        .clone();
    extra["number"] = json!("plain");
    rows.push(extra);
    let mut document: Value = serde_json::from_str(&document).unwrap();
    document["cards"]["plain"] = json!({"status":"complete","abilities":[]});
    Arc::new(
        Catalog::from_documents(
            &rows
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
            &registry,
            &[("unit.yaml".into(), document.to_string())],
        )
        .unwrap(),
    )
}

fn initial(active: &str, acted: bool) -> Value {
    let mut initial = setup(1, json!([]));
    initial["turn"]["active"] = json!(active);
    for side in ["P1", "P2"] {
        initial["players"][side]["zones"]["hand"] = json!([]);
        initial["players"][side]["zones"]["field"] = json!([]);
    }
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"arena","card":"unit-amulet"}));
    initial["players"][active]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"attacker","card":"unit-follower"}));
    let other = if active == "P1" { "P2" } else { "P1" };
    initial["players"][other]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"enemy","card":"plain","state":{"acted":acted}}));
    initial
}

fn board(
    initial: &Value,
    arena: &WellFormedCardProgram,
    mine: &WellFormedCardProgram,
    enemy: &WellFormedCardProgram,
) -> Game {
    let mut board = Game::new(
        arena_catalog(),
        initial,
        &Value::Null,
        &Value::Null,
        "arena-attack",
    )
    .unwrap();
    for (number, program) in [
        ("unit-amulet", arena),
        ("unit-follower", mine),
        ("plain", enemy),
    ] {
        board.load_internal_card(number, program).unwrap();
    }
    board
}

#[test]
fn q871_uses_legal_targets_including_assail_and_intimidate_for_each_side() {
    let arena = arena_program();
    for (acted, assail, intimidate, can_hit_follower) in [
        (true, false, false, true),
        (false, false, false, false),
        (false, true, false, true),
        (true, true, true, false),
    ] {
        let mine = follower("unit-follower", if assail { &["assail"] } else { &[] });
        let enemy = follower("plain", if intimidate { &["intimidate"] } else { &[] });
        for active in ["P1", "P2"] {
            let target = if active == "P1" {
                "P2.leader"
            } else {
                "P1.leader"
            };
            let initial = initial(active, acted);
            for (target, allowed) in [(target, !can_hit_follower), ("enemy", can_hit_follower)] {
                let mut board = board(&initial, &arena, &mine, &enemy);
                assert_eq!(
                    board
                        .internal_rule_action(
                            &json!({"do":"attack","attacker":"attacker","target":target})
                        )
                        .unwrap()
                        .outcome,
                    if allowed { "resolved" } else { "cannot-attack" }
                );
            }
        }
    }
}

#[test]
fn one_attackers_assail_does_not_change_another_attackers_leader_permission() {
    let arena = arena_program();
    let mine = follower("unit-follower", &["assail"]);
    let empty = card(vec![]);
    let mut initial = initial("P1", false);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"plain-attacker","card":"plain"}));
    for (attacker, allowed) in [("attacker", false), ("plain-attacker", true)] {
        let mut board = board(&initial, &arena, &mine, &empty);
        assert_eq!(
            board
                .internal_rule_action(
                    &json!({"do":"attack","attacker":attacker,"target":"P2.leader"})
                )
                .unwrap()
                .outcome,
            if allowed { "resolved" } else { "cannot-attack" }
        );
    }
}

#[test]
fn separate_arenas_do_not_recurse_and_each_stops_applying_when_it_leaves_or_loses_abilities() {
    for effect in [
        json!({"apply":{"on":"this","effect":{"lose_abilities":{}},"until":"turn"}}),
        json!({"move":{"ref":"this","to":"hand"}}),
    ] {
        let mut value = input();
        let extra = evolution_linked_movement::tree(&[(
            "activate",
            json!({"cost":{"pp":0},"do":[effect]}),
        )]);
        let mut head = extra["cards"]["unit-follower"]["abilities"][0].clone();
        head["line"] = json!(2);
        value["cards"]["unit-follower"]["abilities"]
            .as_array_mut()
            .unwrap()
            .push(head);
        let source = value["cards"]["unit-follower"].take();
        value["cards"] = json!({"unit-amulet":source});
        let arena = lowerer()
            .lower(&value)
            .unwrap()
            .remove("unit-amulet")
            .unwrap();
        let activated = arena.card().abilities[1].node.id;
        let mut initial = initial("P1", true);
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"second","card":"unit-amulet"}));
        let empty = card(vec![]);
        let mut board = board(&initial, &arena, &empty, &empty);
        for source in ["arena", "second"] {
            assert_eq!(
                board
                    .internal_rule_action(
                        &json!({"do":"attack","attacker":"attacker","target":"P2.leader"})
                    )
                    .unwrap()
                    .outcome,
                "cannot-attack"
            );
            resolved(
                board
                    .activate_internal_card(source, activated, PlayerId { seat: 1 })
                    .unwrap(),
            );
            event_ability::finish(&mut board);
        }
        assert_eq!(
            board
                .internal_rule_action(
                    &json!({"do":"attack","attacker":"attacker","target":"P2.leader"})
                )
                .unwrap()
                .outcome,
            "resolved"
        );
    }
}

#[test]
fn the_conditional_shape_keeps_generic_predicates_and_installed_forms_closed() {
    let generic = card(vec![ability(
        1,
        vec![node(Node::Control {
            control: Control::If {
                condition: Expr::Live(Box::new(BoolRead::LegalAttack {
                    attacker: ObjectOperand::SourceHolder {},
                    ignore_querying_clause: EffectDefinitionId(1),
                })),
                then_body: Program {
                    scope: ScopeId(2),
                    steps: vec![],
                },
                else_body: None,
            },
        })],
    )]);
    let mut game = Game::new(
        arena_catalog(),
        &initial("P1", true),
        &Value::Null,
        &Value::Null,
        "generic-attack-query",
    )
    .unwrap();
    let before = game.projection(View::Referee).unwrap();
    let error = game
        .load_internal_card("unit-follower", &generic)
        .unwrap_err();
    assert!(matches!(error, EngineFailure::Unsupported(_)));
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    let mut value = evolution_linked_movement::tree(&[(
        "activate",
        json!({"cost":{"pp":0},"do":[{"apply":{
            "on":"this","until":"turn","effect":{"cannot":{"what":"attack","target":{"leaders":"all"},"if_can_attack":"follower"}}
        }}]}),
    )]);
    lowerer().lower(&value).unwrap_err();
    value = input();
    value["cards"]["unit-follower"]["abilities"][0]["static"]["effects"][0]["cannot"]["target"] =
        json!({"who":"opp","zone":"field","kind":"follower"});
    lowerer().lower(&value).unwrap_err();
}

#[test]
fn two_conditional_clauses_cannot_share_the_querying_clause_identity() {
    let mut value = input();
    let effects = value["cards"]["unit-follower"]["abilities"][0]["static"]["effects"]
        .as_array_mut()
        .unwrap();
    effects.push(effects[0].clone());
    let programs = lowerer().lower(&value).unwrap();
    let mut raw = programs["unit-follower"].card().clone();
    let Effect::Permission {
        subject:
            PermissionSubject::Objects {
                objects: Selector::Filter { predicate, .. },
            },
        ..
    } = &raw.abilities[0].node.declarations[0]
    else {
        panic!("first conditional clause");
    };
    let Expr::Live(read) = predicate.as_ref() else {
        panic!("query read");
    };
    let BoolRead::LegalAttack {
        ignore_querying_clause,
        ..
    } = read.as_ref()
    else {
        panic!("query identity");
    };
    let reused = *ignore_querying_clause;
    let Effect::Permission {
        subject: PermissionSubject::Objects { objects },
        action: ActionPattern::Attack { attacker, .. },
        ..
    } = &mut raw.abilities[0].node.declarations[1]
    else {
        panic!("second conditional clause");
    };
    for selector in [objects, attacker] {
        let Selector::Filter { predicate, .. } = selector else {
            panic!("query selector");
        };
        let Expr::Live(read) = predicate.as_mut() else {
            panic!("query read");
        };
        let BoolRead::LegalAttack {
            ignore_querying_clause,
            ..
        } = read.as_mut()
        else {
            panic!("query identity");
        };
        *ignore_querying_clause = reused;
    }
    WellFormedCardProgram::check(raw).unwrap_err();
}
