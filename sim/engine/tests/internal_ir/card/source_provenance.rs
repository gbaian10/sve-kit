use super::*;
use sve_engine::EngineFailure;
use sve_engine::game::internal::InputDomain;
use sve_engine::game::player::{Input, Viewer, observation as native};

pub(super) fn granting(number: &str, hidden: bool) -> WellFormedCardProgram {
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
    nested.source = source(number);
    nested.source.fragment = None;
    nested.node.source.origin = AbilityOrigin::Printed {
        face: FaceId("0".into()),
        line: Nat(1),
    };
    let nested = timing::on_attack(nested).node;
    let grant = instruction(
        1,
        Action::Grant {
            subjects: Selector::Zone {
                players: players(PlayerRelation::SelfPlayer),
                zone: ZoneKind::Field,
                view: SelectionView::Current {},
                binding: Binding::new(50, ScopeId(100)),
                predicate: Box::new(Expr::Literal(Bool(true))),
            },
            abilities: vec![GrantedAbility::Definition {
                ability: Box::new(nested),
            }],
            duration: Duration {
                end: DurationEnd::NoTemporalExpiry {},
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
    );
    let mut root = ability(1, vec![grant]);
    root.source.card_id = number.into();
    root.source.card_no = number.into();
    if hidden {
        root.node.active_zones = vec![ZoneKind::Hand];
    }
    let span = root.source.clone();
    let origin = root.node.source.clone();
    locate_steps(&mut root.node.body.as_mut().unwrap().steps, &span, &origin);
    card(vec![root])
}

fn prepared(seat: u32, number: &str, hidden: bool) -> Game {
    let actor = if seat == 1 { "P1" } else { "P2" };
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(actor);
    for side in ["P1", "P2"] {
        initial["players"][side]["zones"]["field"] = json!([]);
        initial["players"][side]["zones"]["hand"] = json!([]);
    }
    initial["players"][actor]["zones"]["field"] =
        json!([{"id":"recipient","card":"unit-follower"}]);
    if hidden {
        initial["players"][actor]["zones"]["hand"] = json!([{"id":"giver","card":number}]);
    } else {
        initial["players"][actor]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"giver","card":number}));
    }
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "source-provenance",
    )
    .unwrap();
    for key in ["unit-follower", "unit-amulet", "unit-spell"] {
        let program = if key == number {
            granting(number, hidden)
        } else {
            card(vec![])
        };
        game.load_internal_card(key, &program).unwrap();
    }
    game.advance_player_input();
    resolved(
        game.activate_internal_card("giver", AbilityId(1), PlayerId { seat })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    game
}

fn attack(game: &mut Game, seat: u32, holder: &str) {
    let other = if seat == 1 { "P2" } else { "P1" };
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"attack","attacker":holder,"target":format!("{other}.leader")})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the actual attack commits before its trigger"
    );
    assert!(
        game.internal_pending_choice().unwrap().is_some(),
        "the granted attack ability is really pending"
    );
}

#[test]
fn source_provenance_same_printing_and_self_grants_remain_granted() {
    for seat in [1, 2] {
        for holder in ["recipient", "giver"] {
            let mut game = prepared(seat, "unit-follower", false);
            attack(&mut game, seat, holder);
            let before = exact_authority(&game);
            for viewer in [Viewer::P1, Viewer::P2] {
                let observed = game.observe(viewer).unwrap();
                assert_eq!(observed.pending.len(), 1);
                assert_eq!(
                    observed.pending[0].source.provenance,
                    native::TextProvenance::Granted {
                        by: native::CardFace {
                            card: "unit-follower".into(),
                            face: 0
                        }
                    }
                );
                assert_eq!(observed.pending[0].source.card, "unit-follower");
                assert!(observed.pending[0].source.holder.is_some());
            }
            assert_eq!(exact_authority(&game), before);
        }
    }
}

#[test]
fn source_provenance_public_same_printing_recipient_cannot_authorize_a_hidden_owner() {
    for seat in [1, 2] {
        let (actor, waiting) = if seat == 1 {
            (Viewer::P1, Viewer::P2)
        } else {
            (Viewer::P2, Viewer::P1)
        };
        let mut boards = Vec::new();
        let mut histories = Vec::new();
        for number in ["unit-follower", "unit-amulet"] {
            let mut game = prepared(seat, number, true);
            attack(&mut game, seat, "recipient");
            let before = exact_authority(&game);
            let acting = game.observe(actor).unwrap();
            assert_eq!(acting.pending.len(), 1);
            assert_eq!(acting.pending[0].source.card, number);
            assert_eq!(
                acting.pending[0].source.provenance,
                native::TextProvenance::Granted {
                    by: native::CardFace {
                        card: number.into(),
                        face: 0
                    }
                }
            );
            let observed = game.observe(waiting).unwrap();
            assert_eq!(observed.pending.len(), 0);
            assert_eq!(exact_authority(&game), before);
            let question = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = question.domain else {
                panic!("attack ability")
            };
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap(),
            );
            game.advance_player_input();
            let after = exact_authority(&game);
            assert_eq!(game.observe(actor).unwrap().pending.len(), 0);
            boards.push((observed, game.observe(waiting).unwrap()));
            let events = game
                .events(waiting, None)
                .unwrap()
                .cloned()
                .collect::<Vec<_>>();
            assert_ne!(
                events.len(),
                0,
                "the real committed journal participates in the differential"
            );
            histories.push(format!("{events:?}"));
            assert_eq!(exact_authority(&game), after);
        }
        assert_eq!(
            boards[0], boards[1],
            "every waiting field must ignore the private source, including same-printing identity"
        );
        assert_eq!(
            histories[0], histories[1],
            "committed events cannot reveal the private text owner"
        );
    }
}

fn kind_subjects(kind: CardKind, scope: u64) -> Selector {
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
            right: Expr::Literal(kind),
        }))),
    }
}

#[expect(
    clippy::panic,
    reason = "The fixture starts from one checked grant instruction."
)]
fn two_hops() -> WellFormedCardProgram {
    let mut raw = granting("unit-follower", false).card().clone();
    let mut leaf = ability(
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
    leaf.source = source("unit-follower");
    leaf.source.fragment = None;
    leaf.node.source.origin = AbilityOrigin::Printed {
        face: FaceId("0".into()),
        line: Nat(1),
    };
    let leaf = timing::on_attack(leaf).node;
    let mut second = ability(
        99,
        vec![instruction(
            99,
            Action::Grant {
                subjects: kind_subjects(CardKind::Follower, 200),
                abilities: vec![GrantedAbility::Definition {
                    ability: Box::new(leaf),
                }],
                duration: Duration {
                    end: DurationEnd::NoTemporalExpiry {},
                    anchor: TimeAnchor::Installation {},
                    usage: None,
                    intervals: vec![],
                },
                provider: TextProvider {
                    provider: ObjectView::Current {
                        object: Box::new(ObjectOperand::SourceHolder {}),
                    },
                    face: FaceId("0".into()),
                    ability: AbilityId(99),
                },
            },
        )],
    );
    second.source = source("unit-follower");
    second.source.fragment = None;
    second.node.source.origin = AbilityOrigin::Printed {
        face: FaceId("0".into()),
        line: Nat(1),
    };
    let span = second.source.clone();
    let origin = second.node.source.clone();
    locate_steps(
        &mut second.node.body.as_mut().unwrap().steps,
        &span,
        &origin,
    );
    let Node::Instruction { instruction: root } =
        &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("checked grant constructor")
    };
    let Action::Grant {
        subjects,
        abilities,
        ..
    } = &mut root.action
    else {
        panic!("checked grant constructor")
    };
    *subjects = kind_subjects(CardKind::Amulet, 100);
    *abilities = vec![GrantedAbility::Definition {
        ability: Box::new(second.node),
    }];
    WellFormedCardProgram::check(raw).unwrap()
}

#[test]
fn source_provenance_installed_chain_requires_an_admitted_producer() {
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let mut initial = setup(seat, json!([]));
        initial["turn"]["active"] = json!(actor);
        for side in ["P1", "P2"] {
            initial["players"][side]["zones"]["field"] = json!([]);
            initial["players"][side]["zones"]["hand"] = json!([]);
        }
        initial["players"][actor]["zones"]["field"] = json!([
            {"id":"giver","card":"unit-follower"},
            {"id":"middle","card":"unit-amulet"},
            {"id":"recipient","card":"unit-follower"}
        ]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "two-source-hops",
        )
        .unwrap();
        let before = exact_authority(&game);
        let error = game
            .load_internal_card("unit-follower", &two_hops())
            .unwrap_err();
        assert!(
            matches!(error, EngineFailure::Unsupported(message) if message.contains("nested grant installation"))
        );
        assert_eq!(exact_authority(&game), before);
    }
}

#[expect(
    clippy::panic,
    reason = "The fixture uses the checked static keyword and grant constructors."
)]
fn continuous_hops() -> WellFormedCardProgram {
    let mut raw = granting("unit-follower", false).card().clone();
    let mut continuous = static_keywords::conditional_keyword();
    continuous.source = source("unit-follower");
    continuous.source.fragment = None;
    continuous.node.id = AbilityId(99);
    continuous.node.source.origin = AbilityOrigin::Printed {
        face: FaceId("0".into()),
        line: Nat(1),
    };
    let Effect::Continuous {
        target,
        condition,
        patch,
        source: text,
        ..
    } = &mut continuous.node.declarations[0]
    else {
        panic!("static keyword")
    };
    *target = EffectTarget::Dynamic {
        selector: kind_subjects(CardKind::Follower, 200),
    };
    *condition = None;
    *text = continuous.node.source.clone();
    *patch = CharacteristicPatch::GrantAbility {
        abilities: vec![GrantedAbility::Keyword {
            keyword: KeywordId::Drain,
        }],
    };
    let Node::Instruction { instruction: root } =
        &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("grant")
    };
    let Action::Grant {
        subjects,
        abilities,
        ..
    } = &mut root.action
    else {
        panic!("grant")
    };
    *subjects = kind_subjects(CardKind::Amulet, 100);
    *abilities = vec![GrantedAbility::Definition {
        ability: Box::new(continuous.node),
    }];
    WellFormedCardProgram::check(raw).unwrap()
}

#[test]
fn source_provenance_continuous_chain_uses_the_outer_giver() {
    for seat in [1, 2] {
        let (actor, other) = if seat == 1 {
            ("P1", "P2")
        } else {
            ("P2", "P1")
        };
        let mut initial = setup(seat, json!([]));
        initial["turn"]["active"] = json!(actor);
        for side in ["P1", "P2"] {
            initial["players"][side]["zones"]["field"] = json!([]);
            initial["players"][side]["zones"]["hand"] = json!([]);
        }
        initial["players"][actor]["zones"]["field"] = json!([
            {"id":"giver","card":"unit-follower"},
            {"id":"middle","card":"unit-amulet"},
            {"id":"recipient","card":"unit-follower"}
        ]);
        let mut game = Game::new(
            static_keywords::keyword_catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "continuous-source-hops",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &continuous_hops())
            .unwrap();
        for number in ["unit-amulet", "unit-spell"] {
            game.load_internal_card(number, &card(vec![])).unwrap();
        }
        resolved(
            game.activate_internal_card("giver", AbilityId(1), PlayerId { seat })
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"recipient","target":format!("{other}.leader")})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        let quick = game.internal_quick_choice().unwrap().unwrap();
        game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
            .unwrap();
        assert!(game.internal_pending_choice().unwrap().is_some());
        let before = exact_authority(&game);
        for viewer in [Viewer::P1, Viewer::P2] {
            let observed = game.observe(viewer).unwrap();
            assert_eq!(observed.pending.len(), 1);
            assert_eq!(observed.pending[0].source.card, "unit-follower");
            assert_eq!(
                observed.pending[0].source.provenance,
                native::TextProvenance::Granted {
                    by: native::CardFace {
                        card: "unit-amulet".into(),
                        face: 0
                    }
                }
            );
        }
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn source_provenance_unbound_fragment_cannot_claim_printed_or_a_giver() {
    for seat in [1, 2] {
        let own = format!("P{seat}");
        let mut initial = setup(seat, json!([]));
        initial["players"][&own]["zones"]["hand"] = json!([]);
        initial["players"][&own]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "unbound-origin",
        )
        .unwrap();
        game.execute_internal(
            &checked(vec![numeric::buff(1, 1, 0)]),
            &source("unit-follower"),
            context(seat),
        )
        .unwrap();
        let before = exact_authority(&game);
        for viewer in [Viewer::P1, Viewer::P2] {
            let observed = game.observe(viewer).unwrap();
            let sources = observed
                .players
                .iter()
                .filter(|player| player.player.player().seat == seat)
                .flat_map(|player| &player.zones)
                .flat_map(|zone| &zone.cards)
                .filter_map(|member| member.card.properties.state.as_ref())
                .flat_map(|state| &state.effects)
                .filter_map(|effect| effect.source.as_ref())
                .collect::<Vec<_>>();
            assert_eq!(sources.len(), 1);
            assert_eq!(sources[0].provenance, native::TextProvenance::Unknown);
        }
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn source_provenance_fusion_follows_its_bound_information_without_losing_pending_or_events() {
    use sve_engine::game::player::events::{ActionKind, EventKind};
    for seat in [1, 2] {
        let (mut game, ability) = fusion_position(seat);
        let CardPlay::Awaiting(question) = game
            .activate_internal_card("giver", ability, PlayerId { seat })
            .unwrap()
        else {
            panic!("Fusion payment")
        };
        let InputDomain::CostMaterials(domain) = &question.domain else {
            panic!("Fusion material domain")
        };
        assert_eq!(domain.members.len(), 1);
        let before = exact_authority(&game);
        let observed = [Viewer::P1, Viewer::P2].map(|viewer| game.observe(viewer).unwrap());
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![u64::MAX]),
        )
        .unwrap_err();
        assert_eq!(exact_authority(&game), before);
        assert_eq!(
            [Viewer::P1, Viewer::P2].map(|viewer| game.observe(viewer).unwrap()),
            observed
        );
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let callback = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = callback.domain else {
            panic!("life callback")
        };
        for viewer in [Viewer::P1, Viewer::P2] {
            let observed = game.observe(viewer).unwrap();
            assert_eq!(
                observed.pending.len(),
                1,
                "the actual printed giver was publicly carried into EX"
            );
            assert_eq!(observed.pending[0].source.card, "unit-follower");
            assert_eq!(
                observed.pending[0].source.provenance,
                native::TextProvenance::Printed
            );
        }
        let actor = if seat == 1 { Viewer::P1 } else { Viewer::P2 };
        let observed = game.observe(actor).unwrap();
        assert_eq!(
            observed
                .players
                .iter()
                .find(|player| player.player == actor)
                .unwrap()
                .zones
                .iter()
                .find(|zone| zone.kind == native::ObservedZone::Ex)
                .unwrap()
                .cards
                .len(),
            1
        );
        resolved(
            game.answer_internal_card(
                callback.actor,
                callback.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        game.advance_player_input();
        let after = exact_authority(&game);
        for viewer in [Viewer::P1, Viewer::P2] {
            assert_eq!(game.events(viewer, None).unwrap().filter(|event|
                matches!(&event.kind, EventKind::Action(action) if action.kind == ActionKind::Life)
            ).count(), 2);
        }
        assert_eq!(exact_authority(&game), after);
    }
}

fn fusion_position(seat: u32) -> (Game, AbilityId) {
    let document = evolution_linked_movement::tree(&[(
        "activate",
        json!({
            "fusion":{"sel":{"who":"self","zone":["hand","ex"],"kind":"spell"},"n":1},
            "do":[
                {"delayed":{"on":{"life_changed":{"who":"self","direction":"down"}},
                    "until":"turn","next_only":true,"do":[{"life":{"who":"self","delta":1}}]}},
                {"life":{"who":"self","delta":-1}}
            ]
        }),
    )]);
    let program = lowerer()
        .lower(&document)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let own = format!("P{seat}");
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(own);
    initial["players"][&own]["zones"]["field"] = json!([]);
    initial["players"][&own]["zones"]["hand"] = json!([
        {"id":"giver","card":"unit-follower"},{"id":"material","card":"unit-spell"}
    ]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "followed-origin",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game.advance_player_input();
    let actor = if seat == 1 { Viewer::P1 } else { Viewer::P2 };
    assert!(
        matches!(game.input(actor), Input::Question(_)),
        "the initial real player window must be available before any IR source allocation"
    );
    (game, program.card().abilities[0].node.id)
}
