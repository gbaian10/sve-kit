#![expect(
    clippy::panic,
    reason = "Malformed constructors invalidate the cost boundary tests."
)]
use super::*;
use alloc::collections::BTreeSet;
use sve_engine::game::internal::{Answer, InputDomain};

const fn duration(end: DurationEnd) -> Duration {
    Duration {
        end,
        anchor: TimeAnchor::Installation {},
        usage: None,
        intervals: vec![],
    }
}

fn objects(zone: ZoneKind, scope: u64) -> Selector {
    Selector::Zone {
        players: players(PlayerRelation::SelfPlayer),
        zone,
        view: SelectionView::Current {},
        binding: Binding::new(50, ScopeId(scope)),
        predicate: Box::new(Expr::Literal(Bool(true))),
    }
}

fn fixed_cost(amount: i64) -> GrantedAbility {
    let mut definition = ability(99, vec![]).node;
    definition.kind = AbilityKind::Static;
    definition.source.origin = AbilityOrigin::Printed {
        face: FaceId("0".into()),
        line: Nat(1),
    };
    definition.play = None;
    definition.body = None;
    definition.active_zones = vec![ZoneKind::Ex, ZoneKind::Resolution];
    definition.declarations = vec![Effect::CostModifier {
        kind: CostKind::Play,
        subjects: Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        },
        change: CostChange::Delta {
            amount: Expr::Literal(Int(amount)),
        },
        duration: duration(DurationEnd::NoTemporalExpiry {}),
        usage: None,
    }];
    GrantedAbility::Definition {
        ability: Box::new(definition),
    }
}

fn moving(id: u64, objects: Selector, zone: OwnerZone) -> Located<Ability> {
    ability(
        id,
        vec![instruction(
            id,
            Action::Move {
                objects,
                movement: MoveSpec {
                    to: MoveDestination::Owners { zone },
                    position: None,
                    order: None,
                    initial_posture: None,
                    entry_effects: vec![],
                },
                reason: MoveReason::Put,
            },
        )],
    )
}

fn program(amount: i64, end: DurationEnd) -> WellFormedCardProgram {
    let granting = instruction(
        1,
        Action::Grant {
            subjects: objects(ZoneKind::Ex, 100),
            abilities: vec![fixed_cost(amount)],
            duration: duration(end),
            provider: TextProvider {
                provider: ObjectView::Current {
                    object: Box::new(ObjectOperand::SourceHolder {}),
                },
                face: FaceId("0".into()),
                ability: AbilityId(1),
            },
        },
    );
    let mut definitions = vec![
        ability(1, vec![granting]),
        moving(
            2,
            Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            OwnerZone::Cemetery,
        ),
        moving(3, objects(ZoneKind::Ex, 103), OwnerZone::Cemetery),
        moving(4, objects(ZoneKind::Cemetery, 104), OwnerZone::Hand),
    ];
    for definition in &mut definitions {
        definition.source.card_no = "unit-amulet".into();
        definition.source.card_id = "unit-amulet".into();
        if let Some(body) = &mut definition.node.body {
            locate_steps(&mut body.steps, &definition.source, &definition.node.source);
        }
    }
    card(definitions)
}

fn named_catalog() -> Arc<Catalog> {
    let (snapshot, _, document) = catalog_documents(&json!("5"));
    Arc::new(
        Catalog::from_documents(
            &snapshot,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../authored/rules/keywords.yaml"
            )),
            &[("unit.yaml".into(), document)],
        )
        .unwrap(),
    )
}

fn unprogrammed_game() -> Game {
    Game::new(
        named_catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "static-grant-admission",
    )
    .unwrap()
}

fn position(program: &WellFormedCardProgram) -> Game {
    position_in_zone(program, false)
}

fn field_position(program: &WellFormedCardProgram) -> Game {
    position_in_zone(program, true)
}

fn position_in_zone(program: &WellFormedCardProgram, field_receiver: bool) -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":10,"max":10});
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"s","card":"unit-amulet"},{"id":"t","card":"unit-amulet"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"r","card":"unit-follower"}]);
    if field_receiver {
        initial["players"]["P1"]["zones"]["ex"] = json!([]);
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"r","card":"unit-follower"}));
    }
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!(
            (0..3)
                .map(|i| json!({"id":format!("{seat}-d{i}"),"card":"unit-spell"}))
                .collect::<Vec<_>>()
        );
    }
    let mut game = Game::new(
        named_catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "static-grant",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", program).unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

fn activate(game: &mut Game, id: u64) {
    resolved(
        game.activate_internal_card("s", AbilityId(id), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
}

fn pay(game: &mut Game) -> i64 {
    let before = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
        .as_i64()
        .unwrap();
    game.play_internal_card("r", PlayerId { seat: 1 }).unwrap();
    before
        .checked_sub(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
                .as_i64()
                .unwrap(),
        )
        .unwrap()
}

#[test]
fn static_granted_costs_compose_after_the_provider_leaves_and_across_the_actual_play_moves() {
    let mut game = position(&program(-2, DurationEnd::NoTemporalExpiry {}));
    activate(&mut game, 1);
    activate(&mut game, 1);
    activate(&mut game, 2);
    assert_eq!(
        pay(&mut game),
        1,
        "two independent installed definitions reduce five PP to one after their provider leaves"
    );
    assert!(
        game.projection(View::Referee).unwrap()["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("r")),
        "payment and both play rule movements complete with the same carried effect instances"
    );
}

#[test]
fn static_granted_costs_do_not_follow_ordinary_moves_through_cemetery_to_hand() {
    let mut game = position(&program(-2, DurationEnd::NoTemporalExpiry {}));
    activate(&mut game, 1);
    activate(&mut game, 3);
    activate(&mut game, 4);
    assert_eq!(
        pay(&mut game),
        5,
        "a new Hand generation does not inherit an EX grant when subsequently played"
    );
}

#[test]
fn static_granted_costs_floor_the_real_payment_at_zero_without_restoring_pp() {
    let mut game = position(&program(-9, DurationEnd::NoTemporalExpiry {}));
    activate(&mut game, 1);
    assert_eq!(
        pay(&mut game),
        0,
        "an oversized negative delta reaches the common zero floor and never refunds PP"
    );
}

#[test]
fn static_granted_costs_expire_at_the_actual_turn_boundary() {
    let mut game = position(&program(-2, DurationEnd::CurrentTurnEnd {}));
    activate(&mut game, 1);
    for _ in 0..2 {
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        quick::pass(&mut game);
        game.internal_rule_action(&json!({"do":"end-discard","select":[]}))
            .unwrap();
    }
    assert_eq!(
        pay(&mut game),
        5,
        "the turn-scoped grant expires before the receiver is played on its controller's next turn"
    );
}

#[test]
fn static_granted_cost_declaration_rejects_unsupported_cost_rules_before_execution() {
    let GrantedAbility::Definition { ability: valid } = fixed_cost(-2) else {
        panic!("the cost helper constructs a static definition")
    };
    let candidate = |ability| CardProgram {
        abilities: vec![Located {
            source: source("unit-follower"),
            node: ability,
        }],
        card_play_costs: vec![],
    };
    assert!(
        WellFormedCardProgram::check(candidate(*valid.clone())).is_ok(),
        "the closed EX self discount is a valid declaration before each negative mutation"
    );
    let mut loader_cases = Vec::new();
    for wrong in 0..6 {
        let mut invalid = *valid.clone();
        if wrong == 5 {
            invalid.active_zones = vec![ZoneKind::Field];
        }
        let Effect::CostModifier {
            kind,
            subjects,
            change,
            duration,
            ..
        } = &mut invalid.declarations[0]
        else {
            panic!("the cost helper constructs a static definition")
        };
        match wrong {
            0 => {
                *change = CostChange::Delta {
                    amount: Expr::Literal(Int(1)),
                }
            }
            1 => {
                *change = CostChange::Set {
                    amount: Expr::Literal(Int(0)),
                }
            }
            2 => *kind = CostKind::Evolution,
            3 => *subjects = objects(ZoneKind::Ex, 199),
            4 => duration.end = DurationEnd::CurrentTurnEnd {},
            _ => {}
        }
        let mut tree = program(-2, DurationEnd::NoTemporalExpiry {}).card().clone();
        let Node::Instruction { instruction } =
            &mut tree.abilities[0].node.body.as_mut().unwrap().steps[0].node
        else {
            panic!("grant")
        };
        let Action::Grant { abilities, .. } = &mut instruction.action else {
            panic!("grant")
        };
        *abilities = vec![GrantedAbility::Definition {
            ability: Box::new(invalid),
        }];
        if let Ok(checked) = WellFormedCardProgram::check(tree) {
            loader_cases.push(wrong);
            let mut game = unprogrammed_game();
            assert!(
                game.load_internal_card("unit-amulet", &checked).is_err(),
                "structurally valid cost declarations still undergo the closed granted-cost admission boundary"
            );
        }
    }
    assert!(
        loader_cases.contains(&0) && loader_cases.contains(&1),
        "positive deltas and fixed prices must reach the granted-cost loader before being rejected"
    );
}

fn ex_phase_program() -> WellFormedCardProgram {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["activate"] = json!({"cost":{"pp":0},"do":[
        {"move":{"ref":{"who":"self","zone":"field","kind":"follower"},"to":"ex","result":"moved"}},
        {"grant":{"on":{"follow":"moved"},"abilities":[{"auto":{"zone":"ex","on":{"phase":{"which":"end","whose":"self"}},"do":[{"move":{"ref":"this","to":"cemetery"}}]}}]}}
    ]});
    let mut seed = tree["cards"]["BP08-038"].clone();
    seed["abilities"] = json!([entry]);
    tree["cards"] = json!({"unit-amulet":seed});
    lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-amulet")
        .unwrap()
}

#[test]
fn grants_follow_only_a_successful_new_ex_generation_and_trigger_at_the_actual_end() {
    let program = ex_phase_program();
    let id = program.card().abilities[0].node.id;
    for succeeds in [false, true] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-amulet"}]);
        initial["players"]["P1"]["zones"]["ex"] = json!([]);
        if succeeds {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"r","card":"unit-follower"}));
        } else {
            initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"r","card":"unit-follower"}]);
        }
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "ex-phase-grant",
        )
        .unwrap();
        game.load_internal_card("unit-amulet", &program).unwrap();
        game.load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
        resolved(
            game.activate_internal_card("s", id, PlayerId { seat: 1 })
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "moving to EX does not itself trigger an End ability"
        );
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let pending = game.internal_pending_choice().unwrap();
        assert_eq!(
            pending.is_some(),
            succeeds,
            "a failed movement receipt must not attach an ability to an old EX object with the same physical ID"
        );
        if let Some(question) = pending {
            let InputDomain::Pending(options) = &question.domain else {
                panic!("EX phase pending")
            };
            assert_eq!(
                options.len(),
                1,
                "the freshly granted EX End ability triggers exactly once"
            );
            assert_eq!(
                options[0].card, "unit-amulet",
                "the pending ability keeps its actual text provider"
            );
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap(),
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["cemetery"],
                json!(["r"]),
                "the granted EX ability moves its receiving generation to the cemetery"
            );
        }
    }
}

fn carried_program(pure_keywords: bool) -> WellFormedCardProgram {
    let mut candidate = program(-2, DurationEnd::CurrentTurnEnd {}).card().clone();
    let mut attack = ability(
        199,
        vec![instruction(
            199,
            Action::Life {
                player: PlayerRef::Controller {},
                change: NumericChange::Add {
                    amount: Expr::Literal(Int(1)),
                },
            },
        )],
    );
    attack.source = source("unit-amulet");
    attack.source.fragment = None;
    attack.source.line = 1;
    attack.node.source.origin = AbilityOrigin::Printed {
        face: FaceId("0".into()),
        line: Nat(1),
    };
    locate_steps(
        &mut attack.node.body.as_mut().unwrap().steps,
        &attack.source,
        &attack.node.source,
    );
    let Node::Instruction {
        instruction: grant_instruction,
    } = &mut candidate.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("grant")
    };
    let Action::Grant { abilities, .. } = &mut grant_instruction.action else {
        panic!("grant")
    };
    if pure_keywords {
        abilities.clear();
    } else {
        abilities.push(GrantedAbility::Definition {
            ability: Box::new(timing::on_attack(attack).node),
        });
    }
    abilities.extend([
        GrantedAbility::Keyword {
            keyword: KeywordId::Ward,
        },
        GrantedAbility::Keyword {
            keyword: KeywordId::Storm,
        },
    ]);
    let mut put = ability(
        5,
        vec![instruction(
            5,
            Action::Move {
                objects: objects(ZoneKind::Ex, 105),
                movement: MoveSpec {
                    to: MoveDestination::Shared {
                        zone: ZoneRef {
                            player: PlayerRef::Controller {},
                            kind: ZoneKind::Field,
                        },
                    },
                    position: None,
                    order: None,
                    initial_posture: None,
                    entry_effects: vec![],
                },
                reason: MoveReason::Put,
            },
        )],
    );
    put.source.card_no = "unit-amulet".into();
    put.source.card_id = "unit-amulet".into();
    locate_steps(
        &mut put.node.body.as_mut().unwrap().steps,
        &put.source,
        &put.node.source,
    );
    candidate.abilities.push(put);
    WellFormedCardProgram::check(candidate).unwrap()
}

#[test]
fn ex_rule_carry_keeps_keywords_definitions_ward_entry_and_the_original_expiry() {
    for pure_keywords in [false, true] {
        for by_play in [false, true] {
            let mut game = position(&carried_program(pure_keywords));
            activate(&mut game, 1);
            activate(&mut game, 2);
            let result = if by_play {
                game.play_internal_card("r", PlayerId { seat: 1 }).unwrap()
            } else {
                game.activate_internal_card("t", AbilityId(5), PlayerId { seat: 1 })
                    .unwrap()
            };
            let CardPlay::Awaiting(question) = result else {
                panic!("carried Ward must offer entry Act")
            };
            assert!(
                matches!(question.domain, InputDomain::WardEntry { .. }),
                "both ordinary keywords and definitions participate in prospective entering information"
            );
            let entered = game
                .answer_internal_card(question.actor, question.token, Answer::WardEntry(false))
                .unwrap();
            assert!(
                matches!(entered, CardPlay::Entered(_) | CardPlay::Resolved(_)),
                "play and direct EX entry both commit through the shared movement pipeline"
            );
            game.resume_internal_card().unwrap();
            let view = game.projection(View::Referee).unwrap();
            let expected_pp = if by_play {
                if pure_keywords { 5 } else { 7 }
            } else {
                10
            };
            assert_eq!(
                view["P1"]["pp"]["current"], expected_pp,
                "only an actual play pays its cost and each eligible delta is applied once"
            );
            for keyword in ["守護", "疾走"] {
                assert!(
                    view["objects"]["r"]["keywords"]
                        .as_array()
                        .unwrap()
                        .contains(&json!(keyword)),
                    "EX rule carry retains granted keywords after the provider leaves"
                );
            }
            game.internal_rule_action(&json!({"do":"attack","attacker":"r","target":"P2.leader"}))
                .unwrap();
            let pending = game.internal_pending_choice().unwrap();
            assert_eq!(
                pending.is_some(),
                !pure_keywords,
                "the field Attack definition becomes active on its carried receiving generation"
            );
            if let Some(question) = pending {
                let InputDomain::Pending(options) = &question.domain else {
                    panic!("Attack pending")
                };
                resolved(
                    game.answer_internal_card(
                        question.actor,
                        question.token,
                        Answer::Pending(options[0].handle),
                    )
                    .unwrap(),
                );
                game.resume_internal_card().unwrap();
            }
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                if pure_keywords { 20 } else { 21 },
                "the carried Attack body uses its receiver's controller"
            );
            quick::pass(&mut game);
            for _ in 0..2 {
                game.internal_rule_action(&json!({"do":"end-phase"}))
                    .unwrap();
                quick::pass(&mut game);
                game.internal_rule_action(&json!({"do":"end-discard","select":[]}))
                    .unwrap();
            }
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["r"]["keywords"],
                json!([]),
                "rule carry preserves the original installation deadline without reinstalling or refreshing it"
            );
        }
    }
}

fn field_followers(scope: u64) -> Selector {
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

fn aura_definition() -> Ability {
    let GrantedAbility::Definition { ability } = fixed_cost(-2) else {
        panic!("definition")
    };
    let mut definition = *ability;
    definition.active_zones = vec![ZoneKind::Field];
    definition.declarations = vec![Effect::Continuous {
        id: EffectDefinitionId(99),
        source: definition.source.clone(),
        target: EffectTarget::Dynamic {
            selector: field_followers(999),
        },
        condition: None,
        patch: CharacteristicPatch::GrantAbility {
            abilities: vec![
                GrantedAbility::Keyword {
                    keyword: KeywordId::Storm,
                },
                GrantedAbility::Keyword {
                    keyword: KeywordId::Drain,
                },
            ],
        },
        layer: ContinuousLayer::Ability,
        timestamp: TimestampBasis::ZoneEntry {
            object: ObjectOperand::SourceHolder {},
        },
        active_zones: vec![ZoneKind::Field],
        duration: duration(DurationEnd::NoTemporalExpiry {}),
    }];
    definition
}

fn cap_definition(id: u64, line: u64, threshold: u64, replacement: u64) -> Ability {
    let GrantedAbility::Definition { ability } = fixed_cost(-2) else {
        panic!("definition")
    };
    let mut definition = *ability;
    definition.id = AbilityId(id);
    definition.scope = ScopeId(id);
    definition.source.origin = AbilityOrigin::Printed {
        face: FaceId("0".into()),
        line: Nat(line),
    };
    definition.active_zones = vec![ZoneKind::Field];
    let binding = Binding::new(1, ScopeId(id.checked_add(100).unwrap()));
    definition.declarations = vec![Effect::Replacement {
        id: EffectDefinitionId(id),
        source: definition.source.clone(),
        pattern: ProposedEventPattern::Damage {
            recipient: Some(RecipientPredicate {
                binding,
                condition: Expr::Compute(Box::new(Predicate::RecipientIsFollower {
                    recipient: DamageRecipient::Bound { binding },
                    objects: Selector::Objects {
                        objects: vec![ObjectOperand::SourceHolder {}],
                    },
                })),
            }),
            source: None,
            kind: DamageKind::Any,
        },
        condition: Expr::Literal(Bool(true)),
        rewrite: EventRewrite::CapDamage {
            threshold: Expr::Literal(Nat(threshold)),
            replacement: Expr::Literal(Nat(replacement)),
        },
        scope: ReplacementScope::General,
        affected_player: PlayerRef::ProposedDamageRecipient {},
        once_per_event: OncePerEvent::Enforced,
        usage: None,
        prospective_chars: None,
        duration: duration(DurationEnd::NoTemporalExpiry {}),
    }];
    definition
}

fn with_static_definition(definition: Ability) -> WellFormedCardProgram {
    let mut candidate = program(-2, DurationEnd::NoTemporalExpiry {}).card().clone();
    let Node::Instruction {
        instruction: leaving,
    } = &mut candidate.abilities[1].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("movement")
    };
    let Action::Move { movement, .. } = &mut leaving.action else {
        panic!("movement")
    };
    movement.to = MoveDestination::Owners {
        zone: OwnerZone::Hand,
    };
    let Node::Instruction { instruction } =
        &mut candidate.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("grant")
    };
    let Action::Grant {
        subjects,
        abilities,
        ..
    } = &mut instruction.action
    else {
        panic!("grant")
    };
    *subjects = field_followers(100);
    *abilities = vec![GrantedAbility::Definition {
        ability: Box::new(definition),
    }];
    let mut hit = ability(
        6,
        vec![damage(
            6,
            RecipientSet::Objects {
                objects: field_followers(106),
            },
            6,
        )],
    );
    hit.source.card_no = "unit-amulet".into();
    hit.source.card_id = "unit-amulet".into();
    locate_steps(
        &mut hit.node.body.as_mut().unwrap().steps,
        &hit.source,
        &hit.node.source,
    );
    candidate.abilities.push(hit);
    WellFormedCardProgram::check(candidate).unwrap()
}

#[test]
fn granted_static_aura_tracks_its_recipient_and_drain_keeps_the_original_text_provider() {
    let program = with_static_definition(aura_definition());
    let mut game = field_position(&program);
    activate(&mut game, 1);
    activate(&mut game, 1);
    activate(&mut game, 2);
    let view = game.projection(View::Referee).unwrap();
    for keyword in ["疾走", "ドレイン"] {
        assert!(
            view["objects"]["r"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!(keyword)),
            "the installed aura survives its installer leaving"
        );
    }
    game.internal_rule_action(&json!({"do":"attack","attacker":"r","target":"P2.leader"}))
        .unwrap();
    quick::pass(&mut game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("Drain")
    };
    assert_eq!(
        options.len(),
        1,
        "repeated aura grants produce one Drain rule occurrence"
    );
    assert_eq!(
        options[0].card, "unit-amulet",
        "nested grants preserve the original text provider"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        22,
        "an installed static aura participates in the existing Drain damage observer"
    );
}

#[test]
fn installed_static_replacement_instances_keep_distinct_public_choices_after_the_installer_leaves()
{
    let mut candidate = with_static_definition(cap_definition(99, 1, 3, 1))
        .card()
        .clone();
    let mut second = candidate.abilities[0].clone();
    second.node.id = AbilityId(7);
    second.node.scope = ScopeId(7);
    second.source.line = 7;
    second.node.source.origin = AbilityOrigin::Printed {
        face: FaceId("0".into()),
        line: Nat(7),
    };
    second.node.body.as_mut().unwrap().scope = ScopeId(7);
    let Node::Instruction { instruction } = &mut second.node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("grant")
    };
    instruction.id = InstructionId(7);
    instruction.receipt = ReceiptId(7);
    let Action::Grant {
        subjects,
        abilities,
        provider,
        ..
    } = &mut instruction.action
    else {
        panic!("grant")
    };
    *subjects = field_followers(107);
    let alternate = cap_definition(299, 7, 5, 2);
    *abilities = vec![GrantedAbility::Definition {
        ability: Box::new(alternate),
    }];
    provider.ability = AbilityId(7);
    locate_steps(
        &mut second.node.body.as_mut().unwrap().steps,
        &second.source,
        &second.node.source,
    );
    candidate.abilities.push(second);
    let mut game = field_position(&WellFormedCardProgram::check(candidate).unwrap());
    activate(&mut game, 1);
    activate(&mut game, 1);
    activate(&mut game, 7);
    activate(&mut game, 2);
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("t", AbilityId(6), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("replacement ordering")
    };
    let InputDomain::DamageReplacements(options) = &question.domain else {
        panic!("replacement options")
    };
    assert_eq!(
        options.len(),
        3,
        "two installations of the same definition and another cap retain three effect instances"
    );
    assert_eq!(
        options
            .iter()
            .map(|option| option.handle)
            .collect::<BTreeSet<_>>()
            .len(),
        3
    );
    assert!(
        options.iter().all(|option| option.card == "unit-amulet"),
        "choices identify the text provider after the installer leaves"
    );
    let mut ordered = options
        .iter()
        .map(|option| (option.line, option.handle))
        .collect::<Vec<_>>();
    ordered.sort_by_key(|(line, _)| *line);
    assert_eq!(
        ordered.iter().map(|(line, _)| *line).collect::<Vec<_>>(),
        vec![Some(1), Some(1), Some(7)],
        "each replacement retains the original printed line of its granting ability"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Order(ordered.into_iter().map(|(_, handle)| handle).collect()),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["r"]["hp"],
        2,
        "all three instances resolve through the common replacement ordering pipeline"
    );
}

#[test]
fn granted_static_definitions_reject_private_predicates_and_recursive_definition_auras() {
    let mut public = cap_definition(99, 1, 3, 1);
    let Effect::Replacement { condition, .. } = &mut public.declarations[0] else {
        panic!("replacement")
    };
    *condition = Expr::Compute(Box::new(Predicate::Exists {
        objects: objects(ZoneKind::Field, 299),
    }));
    unprogrammed_game()
        .load_internal_card("unit-amulet", &with_static_definition(public.clone()))
        .unwrap();
    let mut private = public;
    let Effect::Replacement { condition, .. } = &mut private.declarations[0] else {
        panic!("replacement")
    };
    *condition = Expr::Compute(Box::new(Predicate::Exists {
        objects: objects(ZoneKind::Hand, 299),
    }));
    let mut recursive = aura_definition();
    let Effect::Continuous { patch, .. } = &mut recursive.declarations[0] else {
        panic!("aura")
    };
    *patch = CharacteristicPatch::GrantAbility {
        abilities: vec![GrantedAbility::Definition {
            ability: Box::new(cap_definition(299, 1, 3, 1)),
        }],
    };
    for (definition, expected) in [
        (
            private,
            "private observation outside an authorized selector spine",
        ),
        (recursive, "nested continuous definitions"),
    ] {
        let program = with_static_definition(definition);
        let mut game = unprogrammed_game();
        let error = game
            .load_internal_card("unit-amulet", &program)
            .unwrap_err();
        assert!(
            error.to_string().contains(expected),
            "nested definitions must fail at their intended admission boundary: {error}"
        );
    }
}

#[test]
fn direct_ex_entry_rejects_unimplemented_counter_carry_before_moving_the_object() {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"t","card":"unit-amulet"}]);
    initial["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"r","card":"unit-follower","state":{"counters":{"stack_counter":1}}}]);
    let mut game = Game::new(
        named_catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "counter-rule-carry",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &carried_program(true))
        .unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    let before = game.projection(View::Referee).unwrap();
    let error = game
        .activate_internal_card("t", AbilityId(5), PlayerId { seat: 1 })
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unimplemented counter rule carry")
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["P1"]["ex"], before["P1"]["ex"],
        "unimplemented carry must fail before a physical movement"
    );
    assert_eq!(after["P1"]["field"], before["P1"]["field"]);
}

#[test]
fn c0_grants_query_ex_through_public_filters_and_only_numeric_patches_reach_ex() {
    let mut tree = fanfare::expanded::tree().clone();
    let mut seed = tree["cards"]["BP01-041"].clone();
    let query = json!({"who":"self","zone":"ex","kind":"follower"});
    seed["abilities"][0]["activate"] = json!({"cost":{"pp":0},"do":[{"grant":{"on":query,"abilities":[{"play":{"cost_delta":-2}}]}}]});
    tree["cards"] = json!({"unit-amulet":seed});
    assert!(
        lowerer().lower(&tree).is_ok(),
        "the same C0 public EX selector used by real cost grants lowers successfully"
    );
    tree["cards"]["unit-amulet"]["abilities"][0]["activate"]["do"][0]["grant"]["abilities"][0]["play"]
        ["additional_cost"] = json!({"pp":1});
    assert_eq!(
        lowerer().lower(&tree).unwrap_err().code,
        "unsupported_construct",
        "a cost grant must reject mandatory additions rather than discard their payment"
    );
    tree["cards"]["unit-amulet"]["abilities"][0]["activate"]["do"] =
        json!([{"buff":{"on":query,"power":1}}]);
    assert!(
        lowerer().lower(&tree).is_ok(),
        "numeric modification reaches EX followers (CR 4.8.3.3)"
    );
    tree["cards"]["unit-amulet"]["abilities"][0]["activate"]["do"] =
        json!([{"damage":{"to":query,"n":1}}]);
    assert_eq!(
        lowerer().lower(&tree).unwrap_err().code,
        "unsupported_construct",
        "the EX buff entry does not widen other actions to EX"
    );
}
