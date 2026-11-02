#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate grant lifetime tests."
)]
use super::*;
use sve_engine::game::internal::{Answer, InputDomain};

fn followers(scope: u64) -> Selector {
    let binding = Binding::new(50, ScopeId(scope));
    Selector::Zone {
        players: players(PlayerRelation::SelfPlayer),
        zone: ZoneKind::Field,
        view: SelectionView::Current {},
        binding,
        predicate: Box::new(Expr::Compute(Box::new(Predicate::KindEquals {
            left: Expr::Live(Box::new(KindRead::Kind {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::Bound { binding }),
                },
                view: CharacteristicView::Effective,
            })),
            right: Expr::Literal(CardKind::Follower),
        }))),
    }
}

fn grant(end: DurationEnd) -> Located<Node> {
    instruction(
        1,
        Action::Grant {
            subjects: followers(100),
            abilities: vec![GrantedAbility::Keyword {
                keyword: KeywordId::Drain,
            }],
            duration: Duration {
                end,
                anchor: TimeAnchor::Installation {},
                usage: None,
                intervals: vec![],
            },
            provider: TextProvider {
                provider: ObjectView::Current {
                    object: Box::new(ObjectOperand::SourceHolder {}),
                },
                face: FaceId("0".into()),
                ability: AbilityId(1),
            },
        },
    )
}

fn grant_program(mut abilities: Vec<Located<Ability>>) -> WellFormedCardProgram {
    for ability in &mut abilities {
        ability.source.card_id = "unit-amulet".into();
        ability.source.card_no = "unit-amulet".into();
        if let Some(body) = &mut ability.node.body {
            locate_steps(&mut body.steps, &ability.source, &ability.node.source);
        }
    }
    card(abilities)
}

fn attack_definition(line: u64) -> GrantedAbility {
    let mut nested = ability(
        99,
        vec![instruction(
            99,
            Action::Life {
                player: PlayerRef::Controller {},
                change: NumericChange::Add {
                    amount: Expr::Literal(Int(1)),
                },
            },
        )],
    );
    nested.source = source("unit-amulet");
    nested.source.fragment = None;
    nested.source.line = u32::try_from(line).unwrap();
    nested.node.source.origin = AbilityOrigin::Printed {
        face: FaceId("0".into()),
        line: Nat(line),
    };
    GrantedAbility::Definition {
        ability: Box::new(timing::on_attack(nested).node),
    }
}

fn definition_grant(end: DurationEnd) -> Located<Node> {
    let mut node = grant(end);
    let Node::Instruction { instruction } = &mut node.node else {
        panic!("grant")
    };
    let Action::Grant { abilities, .. } = &mut instruction.action else {
        panic!("grant")
    };
    *abilities = vec![attack_definition(1)];
    node
}

#[test]
fn general_granted_ability_uses_the_opposing_recipient_controller() {
    let mut grant = definition_grant(DurationEnd::NoTemporalExpiry {});
    let Node::Instruction { instruction } = &mut grant.node else {
        panic!("grant")
    };
    let Action::Grant { subjects, .. } = &mut instruction.action else {
        panic!("grant")
    };
    let Selector::Zone {
        players: selected_players,
        ..
    } = subjects
    else {
        panic!("zone")
    };
    *selected_players = players(PlayerRelation::Opponents);
    let mut game = position(&grant_program(vec![ability(1, vec![grant])]));
    activate_grant(&mut game);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    quick::pass(&mut game);
    game.internal_rule_action(&json!({"do":"end-discard","select":[]}))
        .unwrap();
    game.internal_rule_action(&json!({"do":"attack","attacker":"f","target":"P1.leader"}))
        .unwrap();
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("pending")
    };
    assert_eq!(q.actor, PlayerId { seat: 2 });
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "unit-amulet");
    let activation = resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap(),
    );
    assert_eq!(activation.source.controller, PlayerId { seat: 2 });
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P2"]["leader"]["life"], 21_u8);
    assert_eq!(view["P1"]["leader"]["life"], 20_u8);
}

#[test]
fn general_grant_preserves_public_query_and_private_read_boundaries() {
    for private in [false, true] {
        let mut node = definition_grant(DurationEnd::NoTemporalExpiry {});
        let Node::Instruction { instruction } = &mut node.node else {
            panic!("grant")
        };
        let Action::Grant { abilities, .. } = &mut instruction.action else {
            panic!("grant")
        };
        let GrantedAbility::Definition { ability: nested } = &mut abilities[0] else {
            panic!("definition")
        };
        let mut objects = followers(99);
        if private {
            let Selector::Zone { zone, .. } = &mut objects else {
                panic!("zone")
            };
            *zone = ZoneKind::Hand;
        }
        nested.body.as_mut().unwrap().steps =
            vec![damage(99, RecipientSet::Objects { objects }, 1)];
        let mut span = source("unit-amulet");
        span.fragment = None;
        locate_steps(
            &mut nested.body.as_mut().unwrap().steps,
            &span,
            &nested.source,
        );
        let program = grant_program(vec![ability(1, vec![node])]);
        let mut game = Game::new(
            static_keywords::keyword_catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "grant-query-boundary",
        )
        .unwrap();
        let result = game.load_internal_card("unit-amulet", &program);
        if private {
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("private observation")
            );
        } else {
            result.unwrap();
        }
    }
}

#[test]
fn definition_grant_lowering_keeps_multiple_targeted_definitions_in_disjoint_scopes() {
    let mut tree = fanfare::expanded::tree().clone();
    let mut template = tree["cards"]["BP08-038"].clone();
    let mut entry = template["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    let head = json!({"attack_time":{
        "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
        "do":[{"damage":{"to":"enemy","n":1}}]
    }});
    entry["activate"] =
        json!({"cost":{"pp":0},"do":[{"grant":{"on":"this","abilities":[head.clone(),head]}}]});
    template["abilities"] = json!([entry]);
    tree["cards"] = json!({"grant-provider":template});
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("grant-provider")
        .unwrap();
    let Node::Instruction { instruction } = &program.card().abilities[0]
        .node
        .body
        .as_ref()
        .unwrap()
        .steps[0]
        .node
    else {
        panic!("grant")
    };
    let Action::Grant { abilities, .. } = &instruction.action else {
        panic!("grant")
    };
    assert_eq!(
        abilities.len(),
        2,
        "both closed target declarations must pass structural validation"
    );
}

#[test]
fn general_grant_loader_rejects_unhandled_events_foreign_providers_and_nested_installations() {
    for case in 0..3 {
        let GrantedAbility::Definition { mut ability } = attack_definition(2) else {
            panic!("definition")
        };
        let expected = match case {
            0 => {
                let event = Binding::new(90, ability.scope);
                ability.source.origin = AbilityOrigin::Keyword {
                    keyword: KeywordId::OnEvolve,
                    origin: Box::new(AbilityOrigin::Printed {
                        face: FaceId("0".into()),
                        line: Nat(2),
                    }),
                };
                ability.trigger.as_mut().unwrap().on = TriggerCondition::Event {
                    pattern: EventPattern::Evolve {
                        binding: event,
                        predicate: Expr::Compute(Box::new(Predicate::ObjectEquals {
                            left: Expr::Frozen(Box::new(ObjectFrozen::EvolveSubject { event })),
                            right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
                        })),
                    },
                };
                "granted definition event"
            }
            1 => {
                ability.source.origin = AbilityOrigin::Keyword {
                    keyword: KeywordId::OnAttack,
                    origin: Box::new(AbilityOrigin::Printed {
                        face: FaceId("0".into()),
                        line: Nat(3),
                    }),
                };
                "granted definition has a foreign text provider"
            }
            _ => {
                let mut nested = grant(DurationEnd::NoTemporalExpiry {});
                let Node::Instruction { instruction } = &mut nested.node else {
                    panic!("grant")
                };
                instruction.id = InstructionId(98);
                instruction.receipt = ReceiptId(98);
                let Action::Grant { provider, .. } = &mut instruction.action else {
                    panic!("grant")
                };
                provider.ability = ability.id;
                ability.body.as_mut().unwrap().steps = vec![nested];
                "nested grant installation"
            }
        };
        let mut span = source("unit-amulet");
        span.fragment = None;
        span.line = 2;
        locate_steps(
            &mut ability.body.as_mut().unwrap().steps,
            &span,
            &ability.source,
        );
        let mut continuous = aura();
        let Effect::Continuous { patch, .. } = &mut continuous.node.declarations[0] else {
            panic!("aura")
        };
        *patch = CharacteristicPatch::GrantAbility {
            abilities: vec![GrantedAbility::Definition { ability }],
        };
        let program = grant_program(vec![continuous]);
        let mut game = Game::new(
            static_keywords::keyword_catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "rejected-grant",
        )
        .unwrap();
        let error = game
            .load_internal_card("unit-amulet", &program)
            .unwrap_err();
        assert!(error.to_string().contains(expected), "case {case}: {error}");
    }
}

fn attack_pending(game: &mut Game, holder: &str) -> Vec<Activation> {
    game.internal_rule_action(&json!({"do":"attack","attacker":holder,"target":"P2.leader"}))
        .unwrap();
    let mut results = Vec::new();
    while let Some(q) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &q.domain else {
            panic!("pending")
        };
        assert!(
            options.iter().all(|option| option.card == "unit-amulet"),
            "every granted occurrence retains its text provider"
        );
        results.push(resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        ));
        game.resume_internal_card().unwrap();
    }
    quick::pass(game);
    results
}

#[test]
fn general_grants_keep_recipient_and_installation_instances_after_the_provider_leaves() {
    let program = grant_program(vec![
        ability(1, vec![definition_grant(DurationEnd::NoTemporalExpiry {})]),
        ability(
            2,
            vec![movement::put(
                2,
                Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
                OwnerZone::Hand,
            )],
        ),
    ]);
    let mut game = position(&program);
    activate_grant(&mut game);
    activate_grant(&mut game);
    resolved(
        game.activate_internal_card("s", AbilityId(2), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let first = attack_pending(&mut game, "ally");
    let second = attack_pending(&mut game, "second");
    assert_eq!(first.len(), 2, "separate installations each trigger");
    assert_eq!(second.len(), 2);
    assert_ne!(first[0].source.instance, first[1].source.instance);
    assert_ne!(first[0].source.holder, second[0].source.holder);
    assert_ne!(first[0].source.instance, second[0].source.instance);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        24_u8
    );
    for _ in 0..2 {
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        quick::pass(&mut game);
        game.internal_rule_action(&json!({"do":"end-discard","select":[]}))
            .unwrap();
    }
    let later = attack_pending(&mut game, "ally");
    assert_eq!(later[0].source.instance, first[0].source.instance);
    assert_eq!(later[1].source.instance, first[1].source.instance);
}

#[test]
fn general_phase_grants_trigger_at_the_actual_end_before_their_turn_expiry() {
    let mut granted = definition_grant(DurationEnd::CurrentTurnEnd {});
    let Node::Instruction { instruction } = &mut granted.node else {
        panic!("grant")
    };
    let Action::Grant { abilities, .. } = &mut instruction.action else {
        panic!("grant")
    };
    let GrantedAbility::Definition {
        ability: definition,
    } = &mut abilities[0]
    else {
        panic!("definition")
    };
    definition.source.origin = AbilityOrigin::Printed {
        face: FaceId("0".into()),
        line: Nat(1),
    };
    let event = Binding::new(90, definition.scope);
    definition.trigger.as_mut().unwrap().on = TriggerCondition::Event {
        pattern: EventPattern::Phase {
            binding: event,
            predicate: Expr::Compute(Box::new(Predicate::And {
                predicates: vec![
                    Expr::Compute(Box::new(Predicate::PhaseEquals {
                        left: Expr::Frozen(Box::new(PhaseFields::PhasePhase { event })),
                        right: Expr::Literal(Phase::End),
                    })),
                    Expr::Compute(Box::new(Predicate::Equal {
                        operands: Box::new(ValueEquality::Edge {
                            left: Expr::Frozen(Box::new(EdgeFields::PhaseEdge { event })),
                            right: Expr::Literal(Edge::Begin),
                        }),
                    })),
                    Expr::Compute(Box::new(Predicate::PlayerEquals {
                        left: Expr::Frozen(Box::new(PlayerFrozen::PhasePlayer { event })),
                        right: Expr::Literal(PlayerRef::Controller {}),
                    })),
                ],
            })),
        },
    };
    let mut span = source("unit-amulet");
    span.fragment = None;
    locate_steps(
        &mut definition.body.as_mut().unwrap().steps,
        &span,
        &definition.source,
    );
    let mut game = position(&grant_program(vec![ability(1, vec![granted])]));
    activate_grant(&mut game);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let mut count = 0;
    while let Some(q) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &q.domain else {
            panic!("phase")
        };
        resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        count += 1;
    }
    assert_eq!(
        count, 2,
        "both recipients subscribe to the same real end edge"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        22_u8
    );
}

#[test]
fn continuous_general_grants_end_when_the_provider_leaves_or_is_silenced() {
    for (leave, silenced) in [(false, false), (true, false), (false, true)] {
        let mut continuous = aura();
        let Effect::Continuous { patch, .. } = &mut continuous.node.declarations[0] else {
            panic!("aura")
        };
        *patch = CharacteristicPatch::GrantAbility {
            abilities: vec![attack_definition(2)],
        };
        let program = grant_program(vec![
            ability(
                1,
                vec![movement::put(
                    1,
                    Selector::Objects {
                        objects: vec![ObjectOperand::SourceHolder {}],
                    },
                    OwnerZone::Hand,
                )],
            ),
            continuous,
        ]);
        let mut game = position_with_silence(&program, silenced, false);
        if leave {
            activate_grant(&mut game);
        }
        let pending = attack_pending(&mut game, "ally");
        assert_eq!(pending.len(), usize::from(!leave && !silenced));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            if leave || silenced { 20_u8 } else { 21_u8 }
        );
    }
}

#[test]
fn general_grants_do_not_follow_a_returning_recipient_generation() {
    let mut game = position(&grant_program(vec![ability(
        1,
        vec![
            definition_grant(DurationEnd::NoTemporalExpiry {}),
            movement::put(2, followers(101), OwnerZone::Hand),
        ],
    )]));
    activate_grant(&mut game);
    game.play_internal_card("ally", PlayerId { seat: 1 })
        .unwrap();
    game.resume_internal_card().unwrap();
    // The new follower must wait for its next turn before declaring an attack.
    for _ in 0..2 {
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        quick::pass(&mut game);
        game.internal_rule_action(&json!({"do":"end-discard","select":[]}))
            .unwrap();
    }
    assert_eq!(
        attack_pending(&mut game, "ally").as_slice(),
        [],
        "inactive or expired grants must not trigger"
    );
}

#[test]
fn turn_limited_general_grants_expire_at_the_turn_boundary() {
    let mut game = position(&grant_program(vec![ability(
        1,
        vec![definition_grant(DurationEnd::CurrentTurnEnd {})],
    )]));
    activate_grant(&mut game);
    for _ in 0..2 {
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        quick::pass(&mut game);
        game.internal_rule_action(&json!({"do":"end-discard","select":[]}))
            .unwrap();
    }
    assert_eq!(
        attack_pending(&mut game, "ally").as_slice(),
        [],
        "inactive or expired grants must not trigger"
    );
}

fn position(program: &WellFormedCardProgram) -> Game {
    position_with_silence(program, false, false)
}

fn position_with_silence(
    program: &WellFormedCardProgram,
    provider_silenced: bool,
    recipient_silenced: bool,
) -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"s","card":"unit-amulet","state":{"silenced":provider_silenced}}, {"id":"ally","card":"unit-follower","state":{"silenced":recipient_silenced}}, {"id":"second","card":"unit-follower"}
    ]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!(
            (0..3)
                .map(|index| json!({"id":format!("{seat}-deck-{index}"),"card":"unit-spell"}))
                .collect::<Vec<_>>()
        );
    }
    let mut game = Game::new(
        static_keywords::keyword_catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "dynamic-grant",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", program).unwrap();
    game.load_internal_card("unit-follower", &grant_program(vec![]))
        .unwrap();
    game.load_internal_card("unit-spell", &grant_program(vec![]))
        .unwrap();
    game
}

fn activate_grant(game: &mut Game) {
    resolved(
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
}

fn strike(game: &mut Game, holder: &str) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":holder,"target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "the recipient reaches a real attack window"
    );
    let q = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
}

fn heal(game: &mut Game) -> Activation {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("Drain")
    };
    assert_eq!(options.len(), 1, "multiple Drain grants recover only once");
    assert_eq!(
        options[0].card, "unit-amulet",
        "Drain keeps the granting card as its text provider"
    );
    let result = resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    result
}

#[test]
fn drain_grants_keep_distinct_recipient_instances_and_merge_repeated_grants() {
    let mut game = position(&grant_program(vec![
        ability(1, vec![grant(DurationEnd::NoTemporalExpiry {})]),
        ability(
            2,
            vec![instruction(
                2,
                Action::Posture {
                    objects: followers(101),
                    posture: Posture::Standing,
                },
            )],
        ),
    ]));
    activate_grant(&mut game);
    activate_grant(&mut game);
    strike(&mut game, "ally");
    let first = heal(&mut game);
    strike(&mut game, "second");
    let second = heal(&mut game);
    assert_ne!(first.source.holder.id, second.source.holder.id);
    assert_ne!(first.source.instance, second.source.instance);
    resolved(
        game.activate_internal_card("s", AbilityId(2), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    strike(&mut game, "ally");
    let repeated = heal(&mut game);
    assert_eq!(first.source.instance, repeated.source.instance);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        26_u8
    );
}

#[test]
fn installed_drain_survives_its_provider_leaving_before_or_after_installation() {
    for before in [false, true] {
        let leave = movement::put(
            2,
            Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            OwnerZone::Hand,
        );
        let steps = if before {
            vec![leave, grant(DurationEnd::NoTemporalExpiry {})]
        } else {
            vec![grant(DurationEnd::NoTemporalExpiry {}), leave]
        };
        let mut game = position(&grant_program(vec![ability(1, steps)]));
        activate_grant(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["s"])
        );
        strike(&mut game, "ally");
        heal(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            22_u8
        );
    }
}

#[test]
fn drain_does_not_follow_a_returning_recipient_generation() {
    let move_out = movement::put(2, followers(101), OwnerZone::Hand);
    let mut game = position(&grant_program(vec![ability(
        1,
        vec![grant(DurationEnd::NoTemporalExpiry {}), move_out],
    )]));
    activate_grant(&mut game);
    assert!(matches!(
        game.play_internal_card("ally", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["ally"]["keywords"],
        json!([])
    );
}

fn aura() -> Located<Ability> {
    let mut aura = static_keywords::conditional_keyword();
    if let Effect::Continuous {
        target,
        condition,
        patch,
        ..
    } = &mut aura.node.declarations[0]
    {
        *target = EffectTarget::Dynamic {
            selector: followers(200),
        };
        *condition = None;
        *patch = CharacteristicPatch::GrantAbility {
            abilities: vec![GrantedAbility::Keyword {
                keyword: KeywordId::Drain,
            }],
        };
    }
    aura
}

#[test]
fn continuous_drain_ends_when_the_provider_leaves() {
    for leave in [false, true] {
        let program = grant_program(vec![
            ability(
                1,
                vec![movement::put(
                    1,
                    Selector::Objects {
                        objects: vec![ObjectOperand::SourceHolder {}],
                    },
                    OwnerZone::Hand,
                )],
            ),
            aura(),
        ]);
        let mut game = position(&program);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["ally"]["keywords"],
            json!(["ドレイン"])
        );
        if leave {
            activate_grant(&mut game);
        }
        strike(&mut game, "ally");
        if leave {
            assert!(game.internal_pending_choice().unwrap().is_none());
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                20_u8
            );
        } else {
            heal(&mut game);
        }
    }
}

#[test]
fn turn_limited_drain_expires_at_the_actual_turn_boundary() {
    let mut game = position(&grant_program(vec![ability(
        1,
        vec![grant(DurationEnd::CurrentTurnEnd {})],
    )]));
    activate_grant(&mut game);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    quick::pass(&mut game);
    game.internal_rule_action(&json!({"do":"end-discard","select":[]}))
        .unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["ally"]["keywords"],
        json!([])
    );
}

#[test]
fn silencing_the_provider_removes_continuous_drain() {
    let mut game = position_with_silence(&grant_program(vec![aura()]), true, false);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["ally"]["keywords"],
        json!([])
    );
    strike(&mut game, "ally");
    assert!(game.internal_pending_choice().unwrap().is_none());
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        20_u8
    );
}

#[test]
fn a_later_grant_restores_drain_and_basic_keywords_after_printed_ability_loss() {
    let mut applied = grant(DurationEnd::NoTemporalExpiry {});
    if let Node::Instruction { instruction } = &mut applied.node
        && let Action::Grant { abilities, .. } = &mut instruction.action
    {
        abilities.push(GrantedAbility::Keyword {
            keyword: KeywordId::Rush,
        });
    }
    let mut game =
        position_with_silence(&grant_program(vec![ability(1, vec![applied])]), false, true);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["ally"]["keywords"],
        json!([])
    );
    activate_grant(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["ally"]["keywords"],
        json!(["ドレイン", "突進"])
    );
    strike(&mut game, "ally");
    heal(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        22_u8
    );
}
