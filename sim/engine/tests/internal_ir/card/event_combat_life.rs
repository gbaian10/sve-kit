#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate native event scenarios."
)]
use super::*;
use sve_engine::game::internal::{Answer, InputDomain};

fn triggered(mut ability: Located<Ability>, on: EventPattern) -> Located<Ability> {
    ability.node.kind = AbilityKind::Triggered;
    ability.node.play = None;
    ability.node.trigger = Some(TriggerSpec {
        on: TriggerCondition::Event { pattern: on },
        active_zones: vec![ZoneKind::Field],
        view: EventSnapshotPolicy::AfterEntry,
        limit: None,
        aggregation: TriggerAggregation::PerFact {},
    });
    ability
}
fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn resolve(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("native event occurrence");
    };
    assert_eq!(
        options.len(),
        1,
        "one actual fact produces one matching occurrence"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    finish(game);
}
fn combat(game: &mut Game, target: &str) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"s","target":target}))
            .unwrap()
            .outcome,
        "resolved",
        "the source declares a real attack"
    );
    let quick = game.internal_quick_choice().unwrap().unwrap();
    assert!(
        matches!(
            game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
                .unwrap(),
            CardPlay::Continued(_)
        ),
        "the defender's real pass commits combat damage"
    );
    finish(game);
}
fn combat_position(program: &WellFormedCardProgram, state: &Value) -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower","state":state}]);
    initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "native-combat-event",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

fn life_header(up: bool, own: bool) -> Located<Ability> {
    let observer = ability(2, vec![]);
    let event = Binding::new(90, observer.node.scope);
    triggered(
        observer,
        EventPattern::LifeChange {
            binding: event,
            predicate: Expr::Compute(Box::new(Predicate::And {
                predicates: vec![
                    Expr::Compute(Box::new(Predicate::PlayerEquals {
                        left: Expr::Frozen(Box::new(PlayerFrozen::LifeChangeSubject { event })),
                        right: Expr::Literal(if own {
                            PlayerRef::Controller {}
                        } else {
                            PlayerRef::RequireOne {
                                players: Box::new(PlayerSet {
                                    relation: PlayerRelation::Opponents,
                                    relative_to: PlayerRef::Controller {},
                                    cardinality: Cardinality {
                                        minimum: Nat(1),
                                        maximum: Some(Nat(1)),
                                    },
                                    text_quantifier: TextQuantifier::One,
                                }),
                            }
                        }),
                    })),
                    Expr::Compute(Box::new(Predicate::CompareInt {
                        left: Expr::Frozen(Box::new(IntFrozen::LifeChangeDelta { event })),
                        comparison: if up {
                            Comparison::Greater
                        } else {
                            Comparison::Less
                        },
                        right: Expr::Literal(Int(0)),
                    })),
                ],
            })),
        },
    )
}

fn combat_observer() -> Located<Ability> {
    let observer = ability(
        2,
        vec![instruction(
            2,
            Action::Posture {
                objects: Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
                posture: Posture::Standing,
            },
        )],
    );
    let event = Binding::new(90, observer.node.scope);
    let flag = |flags| {
        Expr::Compute(Box::new(Predicate::Equal {
            operands: Box::new(ValueEquality::DamageFlags {
                left: Expr::Frozen(Box::new(DamageFlagsFields::DamageFlags { event })),
                right: Expr::Literal(flags),
            }),
        }))
    };
    triggered(
        observer,
        EventPattern::Damage {
            binding: event,
            predicate: Expr::Compute(Box::new(Predicate::And {
                predicates: vec![
                    Expr::Compute(Box::new(Predicate::And {
                        predicates: vec![Expr::Compute(Box::new(Predicate::ObjectEquals {
                            left: Expr::Frozen(Box::new(ObjectFrozen::SnapshotObject {
                                snapshot: Expr::Frozen(Box::new(SnapshotFields::DamageSource {
                                    event,
                                })),
                            })),
                            right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
                        }))],
                    })),
                    Expr::Compute(Box::new(Predicate::Or {
                        predicates: vec![
                            flag(DamageFlags::Combat),
                            flag(DamageFlags::AttackCombat),
                        ],
                    })),
                    Expr::Compute(Box::new(Predicate::PlayerEquals {
                        left: Expr::Literal(PlayerRef::Active {}),
                        right: Expr::Literal(PlayerRef::Controller {}),
                    })),
                ],
            })),
        },
    )
}

#[test]
fn outgoing_combat_observer_rejects_actual_ability_damage_from_the_same_source() {
    let observer = combat_observer();
    let program = card(vec![
        ability(
            1,
            vec![source_information::source_hit(1, Expr::Literal(Int(1)))],
        ),
        observer,
    ]);
    let mut game = combat_position(&program, &json!({"entered_this_turn":false}));
    let activation = activate(&mut game, 1);
    assert!(activation.progress.facts.iter().any(|fact| matches!(fact, Fact::Damage(event) if event.flags == DamageFlags::Ability && event.amount.0.get()==1)), "the negative case really deals ability damage from this source");
    finish(&mut game);
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "ability damage cannot satisfy combat damage"
    );
    combat(&mut game, "f");
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["s"]["acted"],
        true,
        "the real attack acts the source"
    );
    resolve(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["s"]["acted"],
        false,
        "actual attack-combat damage satisfies combat and stands the source"
    );

    let mut leader = combat_position(&program, &json!({"entered_this_turn":false}));
    combat(&mut leader, "P2.leader");
    assert_eq!(
        leader.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        18_u8,
        "the real leader attack deals attack damage"
    );
    assert!(
        leader.internal_pending_choice().unwrap().is_none(),
        "leader attack damage is not combat damage (Q282)"
    );
    assert_eq!(
        leader.projection(View::Referee).unwrap()["objects"]["s"]["acted"],
        true
    );

    let mut zero = combat_position(&program, &json!({"entered_this_turn":false,"power":0_u8}));
    combat(&mut zero, "f");
    assert_eq!(
        zero.projection(View::Referee).unwrap()["objects"]["f"]["damage"],
        0_u8,
        "the actual zero-power attack does not deal damage"
    );
    assert!(
        zero.internal_pending_choice().unwrap().is_none(),
        "zero combat damage creates no outgoing occurrence"
    );
    assert_eq!(
        zero.projection(View::Referee).unwrap()["objects"]["s"]["acted"],
        true,
        "without damage the source stays acted"
    );
}

#[test]
fn life_up_uses_the_actual_set_delta_without_a_twenty_life_cap() {
    for amount in [23_i64, 20, 17] {
        let program = card(vec![
            ability(
                1,
                vec![instruction(
                    1,
                    Action::Life {
                        player: PlayerRef::Controller {},
                        change: NumericChange::Set {
                            amount: Expr::Literal(Int(amount)),
                        },
                    },
                )],
            ),
            life_header(true, true),
        ]);
        let mut game = position_with_card(1, &program);
        let activation = activate(&mut game, 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            amount,
            "the actual instruction sets the life total without clamping"
        );
        let delta = amount.checked_sub(20).unwrap();
        assert_eq!(
            activation
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::LifeChange(_)))
                .count(),
            usize::from(delta != 0),
            "zero actual delta creates no life fact"
        );
        finish(&mut game);
        if delta > 0 {
            resolve(&mut game);
        } else {
            assert!(
                game.internal_pending_choice().unwrap().is_none(),
                "same or lower life does not satisfy up"
            );
        }
    }
}

#[test]
fn opposing_life_loss_from_real_combat_is_observed_once() {
    let program = card(vec![ability(1, vec![]), life_header(false, false)]);
    let mut game = position_with_state(1, &program, &json!({"entered_this_turn":false}));
    combat(&mut game, "P2.leader");
    assert!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"]
            .as_i64()
            .unwrap()
            < 20,
        "the battle really reduced the opposing leader's life"
    );
    resolve(&mut game);
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "the one life fact resolves exactly once"
    );
}

#[test]
fn attack_observer_can_watch_the_opponent_and_keeps_its_original_controller() {
    let observer = ability(2, vec![]);
    let event = Binding::new(90, observer.node.scope);
    let opponent = PlayerRef::RequireOne {
        players: Box::new(PlayerSet {
            relation: PlayerRelation::Opponents,
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
            text_quantifier: TextQuantifier::One,
        }),
    };
    let observer = triggered(
        observer,
        EventPattern::Attack {
            binding: event,
            predicate: Expr::Compute(Box::new(Predicate::And {
                predicates: vec![Expr::Compute(Box::new(Predicate::And {
                    predicates: vec![Expr::Compute(Box::new(Predicate::PlayerEquals {
                        left: Expr::Frozen(Box::new(PlayerFrozen::AttackController { event })),
                        right: Expr::Literal(opponent),
                    }))],
                }))],
            })),
        },
    );
    let program = card(vec![ability(1, vec![]), observer]);
    let mut initial = setup(2, json!([]));
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P2"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower","state":{"entered_this_turn":false}}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"opponent","card":"unit-follower","state":{"entered_this_turn":false}}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "opposing-attack-event",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"attack","attacker":"opponent","target":"P1.leader"})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the opposing player commits a real attack"
    );
    assert_eq!(
        game.internal_pending_choice().unwrap().unwrap().actor,
        PlayerId { seat: 1 },
        "the observing controller, not the attacker, owns the occurrence"
    );
    resolve(&mut game);
    let quick = game.internal_quick_choice().unwrap().unwrap();
    assert!(
        matches!(
            game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
                .unwrap(),
            CardPlay::Continued(_)
        ),
        "the observer's completed occurrence returns to actual combat"
    );
    assert!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"]
            .as_i64()
            .unwrap()
            < 20,
        "the attack continues against its declared opposing leader"
    );
}

fn author_event(on: &Value) -> WellFormedCardProgram {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["line"] = json!(1_u8);
    entry["auto"] = json!({"on":on,"do":[{"life":{"who":"self","delta":1_u8}}]});
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    lowerer().lower(&tree).unwrap()["unit-follower"].clone()
}
fn opposing_attack_position(program: &WellFormedCardProgram) -> Game {
    let mut initial = setup(2, json!([]));
    initial["turn"]["active"] = json!("P2");
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["hand"] = json!([]);
        initial["players"][seat]["zones"]["field"] = json!([{
            "id":if seat == "P1" {"s"} else {"opponent"},
            "card":"unit-follower","state":{"entered_this_turn":false}
        }]);
    }
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "author-attack-event",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

#[test]
fn author_attack_filters_read_the_actual_opposing_follower_name_class_and_token_status() {
    let (snapshot, _, _) = catalog_documents(&json!("1"));
    let name = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|entry| entry["number"] == "unit-follower")
        .unwrap()["faces"][0]["name"]
        .as_str()
        .unwrap()
        .to_owned();
    for (changed, value, expected) in [
        ("kind", json!("follower"), true),
        ("kind", json!("amulet"), false),
        ("name", json!("unmatched attacker"), false),
        ("class", json!("ウィッチ"), false),
        ("token", json!(true), false),
        ("who", json!("self"), false),
    ] {
        let mut selector = json!({"kind":"follower","name":name,"class":"ニュートラル","not_token":true,"other":true});
        let mut event = json!({"who":"opp","sel":selector});
        if changed == "who" {
            event["who"] = value;
        } else {
            if changed == "token" {
                selector.as_object_mut().unwrap().remove("not_token");
            }
            selector[changed] = value;
            event["sel"] = selector;
        }
        let program = author_event(&json!({"attacked":event}));
        let mut game = opposing_attack_position(&program);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"opponent","target":"P1.leader"})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        assert_eq!(
            game.internal_pending_choice().unwrap().is_some(),
            expected,
            "only the actual opposing follower matching every author filter subscribes"
        );
        if expected {
            assert_eq!(
                game.internal_pending_choice().unwrap().unwrap().actor,
                PlayerId { seat: 1 }
            );
            resolve(&mut game);
        }
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            20_u8 + u8::from(expected),
            "the observing controller receives its own effect before combat"
        );
        let quick = game.internal_quick_choice().unwrap().unwrap();
        game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
            .unwrap();
        finish(&mut game);
    }
}

#[test]
fn author_damage_and_life_headers_distinguish_real_leader_attack_and_turn_direction() {
    for (on, expected) in [
        (
            json!({"dealt_damage":{"ref":"this","recipient":{"leader":"opp"},"kind":"attack","during":{"turn":"self"}}}),
            true,
        ),
        (
            json!({"dealt_damage":{"ref":"this","recipient":{"leader":"opp"},"kind":"combat"}}),
            false,
        ),
        (
            json!({"dealt_damage":{"ref":"this","recipient":{"leader":"self"},"kind":"attack"}}),
            false,
        ),
        (
            json!({"dealt_damage":{"ref":"this","recipient":{"leader":"opp"},"kind":"ability"}}),
            false,
        ),
        (
            json!({"life_changed":{"who":"opp","direction":"down","during":{"turn":"self"}}}),
            true,
        ),
        (
            json!({"life_changed":{"who":"opp","direction":"down","during":{"turn":"opp"}}}),
            false,
        ),
        (
            json!({"life_changed":{"who":"opp","direction":"up","during":{"turn":"self"}}}),
            false,
        ),
    ] {
        let program = author_event(&on);
        let mut game = combat_position(&program, &json!({"entered_this_turn":false}));
        combat(&mut game, "P2.leader");
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            18_u8,
            "every case really commits two leader attack damage"
        );
        assert_eq!(
            game.internal_pending_choice().unwrap().is_some(),
            expected,
            "attack facet, actual recipient, life direction and observer-relative turn are independent constraints"
        );
        if expected {
            resolve(&mut game);
        }
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            20_u8 + u8::from(expected),
            "only the matching author header resolves its effect"
        );
    }
}

#[test]
fn noncanonical_attack_header_cannot_enable_generic_live_event_reads() {
    let mut program = author_event(&json!({"attacked":{"ref":"this"}}))
        .card()
        .clone();
    let TriggerCondition::Event {
        pattern: EventPattern::Attack { binding, predicate },
    } = &mut program.abilities[0].node.trigger.as_mut().unwrap().on
    else {
        panic!("the synthetic author declares an attack event");
    };
    *predicate = Expr::Compute(Box::new(Predicate::HasAbility {
        abilities: Expr::Live(Box::new(AbilitiesRead::Abilities {
            object: ObjectView::EventSnapshot {
                object: Box::new(ObjectOperand::FromExpression {
                    value: Box::new(Expr::Frozen(Box::new(ObjectFrozen::AttackAttacker {
                        event: *binding,
                    }))),
                }),
                event: EventContext::Attack { event: *binding },
                policy: EventSnapshotPolicy::AfterEntry,
            },
            view: CharacteristicView::Effective,
        })),
        predicate: Box::new(AbilityPredicate {
            kind: None,
            keyword: Some(KeywordId::Ward),
            except: vec![],
            holder: None,
            origin: None,
        }),
    }));
    assert!(
        WellFormedCardProgram::check(program).is_err(),
        "only the canonical committed header may use this bridge; generic live event reads stay rejected"
    );
}
