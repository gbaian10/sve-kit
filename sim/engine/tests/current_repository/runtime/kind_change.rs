#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the real kind change scenarios."
)]
use super::event_cemetery::finish_checks;
use super::union_burst_batch1::{finish, pending};
use super::*;
use sve_engine::game::internal::SearchAnswer;
use sve_engine::game::player::Viewer;
use sve_engine::game::player::observation::{CardState, ObservedZone};

fn build(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "kind-change",
    )
    .unwrap()
}
fn next_turn(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    if game.internal_quick_choice().unwrap().is_none() {
        game.internal_rule_action(&json!({"do":"guard-act","select":[]}))
            .unwrap();
    }
    let active = game.projection(View::Referee).unwrap()["turn"]["active"].clone();
    for _ in 0..4_u8 {
        if game.projection(View::Referee).unwrap()["turn"]["active"] != active {
            return;
        }
        let Some(question) = game.internal_quick_choice().unwrap() else {
            return;
        };
        game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
    }
    panic!("end turn did not reach the next main phase");
}
fn initial() -> Value {
    let mut position = draw_position();
    position["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"draw","card":"BP01-171"},{"id":"seraph","card":"BP02-091"}]);
    position["players"]["P2"]["zones"]["deck"] = json!([{"id":"enemy-draw","card":"BP01-171"}]);
    position
}
fn field_state(game: &Game, player: Viewer, number: &str) -> CardState {
    let observed = game.observe(player).unwrap();
    let card = observed
        .players
        .iter()
        .find(|seat| seat.player == player)
        .unwrap()
        .zones
        .iter()
        .find(|zone| zone.kind == ObservedZone::Field)
        .unwrap()
        .cards
        .iter()
        .map(|entry| &entry.card.properties)
        .find(|properties| properties.card == number)
        .unwrap();
    card.state.as_ref().unwrap().clone()
}
fn field_stats(game: &Game, player: Viewer, number: &str) -> Option<(i64, i64)> {
    field_state(game, player, number)
        .stats
        .map(|stats| (stats.attack, stats.life))
}

#[test]
fn lapis_reaches_four_prayers_at_main_and_can_attack_to_find_its_real_seraph() {
    let mut position = initial();
    position["turn"]["active"] = json!("P2");
    position["players"]["P1"]["zones"]["field"] =
        json!([{"id":"lapis","card":"BP16-093","state":{"counters":{"祈りカウンター":3_u8}}}]);
    let mut game = build(&position);
    assert_eq!(field_stats(&game, Viewer::P1, "BP16-093"), None);
    next_turn(&mut game);
    let prayer = pending(&mut game, "BP16-093");
    resolved(finish(&mut game, prayer));
    finish_checks(&mut game);
    assert_eq!(field_stats(&game, Viewer::P1, "BP16-093"), Some((5, 5)));
    game.internal_rule_action(&json!({"do":"attack","attacker":"lapis","target":"P2.leader"}))
        .unwrap();
    let search = pending(&mut game, "BP16-093");
    let q = union_burst_batch1::question(search);
    let InputDomain::Search(domain) = &q.domain else {
        panic!("printed Seraph search")
    };
    let found = game
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
        )
        .unwrap();
    finish(&mut game, found);
    finish_checks(&mut game);
    assert!(
        game.projection(View::Referee).unwrap()["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("seraph"))
    );
}

#[test]
fn ezdia_keeps_a_converted_cards_lastword_and_resets_its_kind_on_reentry() {
    let mut position = initial();
    position["players"]["P1"]["zones"]["hand"] = json!([{"id":"ezdia","card":"BP05-001"}]);
    position["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..3_u8)
            .map(|i| json!({"id":format!("hunter-{i}"),"card":"BP01-018"}))
            .collect::<Vec<_>>()
    );
    position["players"]["P2"]["zones"]["field"] = json!([{"id":"mordecai","card":"BP01-107"}]);
    let mut game = build(&position);
    let played = game
        .play_internal_card("ezdia", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, played);
    finish_checks(&mut game);
    let fanfare = pending(&mut game, "BP05-001");
    resolved(finish(&mut game, fanfare));
    finish_checks(&mut game);
    assert_eq!(field_stats(&game, Viewer::P2, "BP01-107"), None);
    next_turn(&mut game);
    let abilities = game.internal_activated_sources("mordecai").unwrap();
    assert_eq!(abilities.len(), 1);
    let paid = game
        .activate_internal_card_instance("mordecai", abilities[0].instance, PlayerId { seat: 2 })
        .unwrap();
    resolved(finish(&mut game, paid));
    finish_checks(&mut game);
    let revived = pending(&mut game, "BP01-107");
    resolved(finish(&mut game, revived));
    finish_checks(&mut game);
    assert_eq!(field_stats(&game, Viewer::P2, "BP01-107"), Some((5, 5)));
}

#[test]
fn lapis_kind_is_live_at_the_printed_four_counter_threshold() {
    for prayers in [2_u8, 3, 4, 5] {
        let mut position = initial();
        position["players"]["P1"]["zones"]["field"] = json!([{"id":"lapis","card":"BP16-093","state":{"counters":{"祈りカウンター":prayers}}}]);
        let game = build(&position);
        assert_eq!(
            field_stats(&game, Viewer::P1, "BP16-093"),
            (prayers >= 4).then_some((5, 5))
        );
    }
}

#[test]
fn ezdia_with_two_hunters_does_not_change_an_opposing_follower() {
    let mut position = initial();
    position["players"]["P1"]["zones"]["hand"] = json!([{"id":"ezdia","card":"BP05-001"}]);
    position["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"hunter1","card":"BP01-018"},{"id":"hunter2","card":"BP01-018"}]);
    position["players"]["P2"]["zones"]["field"] = json!([{"id":"mordecai","card":"BP01-107"}]);
    let mut game = build(&position);
    let play = game
        .play_internal_card("ezdia", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, play);
    finish_checks(&mut game);
    let fanfare = pending(&mut game, "BP05-001");
    resolved(finish(&mut game, fanfare));
    finish_checks(&mut game);
    assert_eq!(field_stats(&game, Viewer::P2, "BP01-107"), Some((5, 5)));
    assert_eq!(
        game.internal_activated_sources("mordecai").unwrap(),
        Vec::new()
    );
}

fn nana_head(game: &mut Game, line: u64) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("Nana evolution trigger")
    };
    let option = options
        .iter()
        .find(|option| option.card == "ECP02-061" && option.line == line)
        .unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap()
}

#[test]
fn nana_normal_and_super_evolution_keep_both_heads_and_granted_abilities() {
    for (super_evolution, super_first) in [(false, false), (true, false), (true, true)] {
        let mut position = initial();
        position["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
        position["players"]["P1"]["leader"]["life"] = json!(10_u8);
        position["players"]["P1"]["sep"] = json!(1_u8);
        position["players"]["P1"]["zones"]["field"] = json!([{"id":"nana","card":"ECP02-060"}]);
        position["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"form","card":"ECP02-061"}]);
        position["players"]["P1"]["zones"]["hand"] = json!([{"id":"lesson","card":"ECP02-060"}]);
        position["players"]["P2"]["zones"]["field"] =
            json!([{"id":"lapis","card":"BP16-093","state":{"counters":{"祈りカウンター":4_u8}}}]);
        position["players"]["P2"]["zones"]["hand"] =
            json!([{"id":"discard1","card":"BP01-171"},{"id":"discard2","card":"BP01-171"}]);
        let mut game = build(&position);
        let payment = if super_evolution {
            json!({"pp":1_u8,"sep":1_u8})
        } else {
            json!({"pp":1_u8})
        };
        game.internal_rule_action(
            &json!({"do":"evolve","source":"nana","evolve_card":"form","pay":payment}),
        )
        .unwrap();
        for line in if super_evolution {
            if super_first { vec![2, 1] } else { vec![1, 2] }
        } else {
            vec![1]
        } {
            let play = nana_head(&mut game, line);
            resolved(finish(&mut game, play));
            finish_checks(&mut game);
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 12_u8);
        assert_eq!(state["P1"]["hand"], json!(["draw"]));
        assert_eq!(
            field_stats(&game, Viewer::P2, "BP16-093"),
            if super_evolution { None } else { Some((5, 5)) }
        );
        if super_evolution {
            next_turn(&mut game);
            // Lapis keeps its printed main-phase trigger as well as the newly granted trigger.
            let printed = pending(&mut game, "BP16-093");
            resolved(finish(&mut game, printed));
            finish_checks(&mut game);
            let granted = pending(&mut game, "ECP02-061");
            resolved(finish(&mut game, granted));
            finish_checks(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
                18_u8
            );
            assert_eq!(field_stats(&game, Viewer::P2, "BP16-093"), None);
            let sources = game.internal_activated_sources("lapis").unwrap();
            assert_eq!(sources.len(), 1);
            let play = game
                .activate_internal_card_instance("lapis", sources[0].instance, PlayerId { seat: 2 })
                .unwrap();
            resolved(finish(&mut game, play));
            finish_checks(&mut game);
            let after_sacrifice = game.projection(View::Referee).unwrap();
            assert_eq!(after_sacrifice["P2"]["field"], json!([]));
            assert_eq!(after_sacrifice["P2"]["hand"], json!(["enemy-draw"]));
        }
    }
}

fn lapis_selector(index: u64) -> Selector {
    let Action::Move { mut objects, .. } = event_cemetery::move_named(
        ZoneKind::Field,
        OwnerZone::Cemetery,
        "大いなる熾天使・ラピス",
    ) else {
        panic!("named external producer")
    };
    let Selector::Zone {
        binding, predicate, ..
    } = &mut objects
    else {
        panic!("named field selector")
    };
    *binding = Binding::new(index, ScopeId(1));
    let Expr::Compute(predicate) = predicate.as_mut() else {
        panic!("name predicate")
    };
    let Predicate::NamesIntersect { left, .. } = predicate.as_mut() else {
        panic!("name match")
    };
    *left = Expr::Live(Box::new(NamesRead::Names {
        object: ObjectView::Current {
            object: Box::new(ObjectOperand::Bound { binding: *binding }),
        },
    }));
    objects
}
const fn lifetime(end: DurationEnd) -> Duration {
    Duration {
        end,
        anchor: TimeAnchor::Installation {},
        usage: None,
        intervals: vec![],
    }
}
#[test]
fn temporary_kind_change_preserves_numeric_layers_and_an_existing_granted_keyword() {
    let mut position = initial();
    position["players"]["P1"]["zones"]["field"] =
        json!([{"id":"lapis","card":"BP16-093","state":{"counters":{"祈りカウンター":4_u8}}}]);
    let program = producer_steps(vec![
        Action::Modify {
            subjects: lapis_selector(51),
            patch: CharacteristicPatch::Combined {
                patches: vec![
                    CharacteristicPatch::Numeric {
                        stat: Stat::Attack,
                        change: NumericChange::Add {
                            amount: Expr::Literal(Int(2)),
                        },
                    },
                    CharacteristicPatch::Numeric {
                        stat: Stat::Life,
                        change: NumericChange::Add {
                            amount: Expr::Literal(Int(3)),
                        },
                    },
                ],
            },
            expires: lifetime(DurationEnd::NoTemporalExpiry {}),
            source: EffectReference::CurrentInstallation {},
        },
        Action::Grant {
            subjects: lapis_selector(52),
            abilities: vec![GrantedAbility::Keyword {
                keyword: KeywordId::Ward,
            }],
            duration: lifetime(DurationEnd::NoTemporalExpiry {}),
            provider: TextProvider {
                provider: ObjectView::Current {
                    object: Box::new(ObjectOperand::SourceHolder {}),
                },
                face: FaceId("0".into()),
                ability: AbilityId(1),
            },
        },
        Action::Modify {
            subjects: lapis_selector(53),
            patch: CharacteristicPatch::SetKind {
                kind: CardKind::Amulet,
            },
            expires: lifetime(DurationEnd::CurrentTurnEnd {}),
            source: EffectReference::CurrentInstallation {},
        },
    ]);
    let mut game = event_subject_capture::prepared_program(&position, &program);
    let play = game
        .activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(&mut game, play));
    finish_checks(&mut game);
    assert_eq!(field_stats(&game, Viewer::P1, "BP16-093"), None);
    let observed = game.observe(Viewer::P1).unwrap();
    let card = observed.players[0]
        .zones
        .iter()
        .find(|zone| zone.kind == ObservedZone::Field)
        .unwrap()
        .cards
        .iter()
        .find(|card| card.card.properties.card == "BP16-093")
        .unwrap();
    assert!(
        card.card
            .properties
            .state
            .as_ref()
            .unwrap()
            .keywords
            .contains(&"守護".to_owned())
    );
    next_turn(&mut game);
    assert_eq!(field_stats(&game, Viewer::P1, "BP16-093"), Some((7, 8)));
}

#[test]
fn a_converted_card_can_evolve_and_keeps_its_amulet_kind_and_earlier_activation() {
    let mut position = initial();
    position["players"]["P1"]["zones"]["hand"] = json!([{"id":"ezdia","card":"BP05-001"}]);
    position["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..3_u8)
            .map(|i| json!({"id":format!("hunter-{i}"),"card":"BP01-018"}))
            .collect::<Vec<_>>()
    );
    position["players"]["P2"]["zones"]["field"] = json!([{"id":"nana","card":"ECP02-060"}]);
    position["players"]["P2"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"ECP02-061"}]);
    position["players"]["P2"]["zones"]["hand"] = json!([{"id":"lesson","card":"ECP02-060"}]);
    position["players"]["P2"]["zones"]["deck"] = json!([{"id":"enemy-draw","card":"BP01-171"},{"id":"enemy-extra","card":"BP01-171"},{"id":"enemy-last","card":"BP01-171"}]);
    let mut game = build(&position);
    let entered = game
        .play_internal_card("ezdia", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, entered);
    finish_checks(&mut game);
    let fanfare = pending(&mut game, "BP05-001");
    resolved(finish(&mut game, fanfare));
    finish_checks(&mut game);
    assert_eq!(field_stats(&game, Viewer::P2, "ECP02-060"), None);
    next_turn(&mut game);
    let evolved = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"nana","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
    assert_eq!(evolved.outcome, "resolved");
    let trigger = pending(&mut game, "ECP02-061");
    resolved(finish(&mut game, trigger));
    finish_checks(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        22_u8
    );
    assert_eq!(field_stats(&game, Viewer::P2, "ECP02-061"), None);
    let sources = game.internal_activated_sources("nana").unwrap();
    assert_eq!(sources.len(), 1);
    let sacrificed = game
        .activate_internal_card_instance("nana", sources[0].instance, PlayerId { seat: 2 })
        .unwrap();
    resolved(finish(&mut game, sacrificed));
    finish_checks(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["field"],
        json!([])
    );
}

fn lower_stat_control(head: &str, body: Value) -> WellFormedCardProgram {
    let mut tree = expanded::tree().clone();
    let mut card = tree["cards"]["BP01-041"].clone();
    let mut ability = card["abilities"][0].clone();
    ability.as_object_mut().unwrap().remove("activate");
    ability[head] = body;
    card["abilities"] = json!([ability]);
    tree["cards"] = json!({"producer":card});
    lowerer().lower(&tree).unwrap().remove("producer").unwrap()
}
fn stat_position() -> Value {
    let mut position = initial();
    position["players"]["P1"]["zones"]["field"] = json!([]);
    position["players"]["P1"]["zones"]["deck"] = json!([{"id":"draw-a","card":"BP01-171"},{"id":"draw-b","card":"BP01-171"},{"id":"draw-c","card":"BP01-171"}]);
    position
}
fn activate_control(game: &mut Game) -> CardPlay {
    let sources = game.internal_activated_sources("producer").unwrap();
    game.activate_internal_card_instance("producer", sources[0].instance, PlayerId { seat: 1 })
        .unwrap()
}
#[test]
fn absent_source_stats_compare_false_but_read_as_zero_while_real_zero_still_compares() {
    for amulet in [false, true] {
        let mut steps = vec![json!({"set_stat":{"on":"this","power":0_u8}})];
        if amulet {
            steps.push(json!({"make_type":{"on":"this","type":"amulet"}}));
        }
        steps.extend([
            json!({"draw":{"n":{"power_of":"this"}}}),
            json!({"if":{"cond":{"cmp":{"left":{"power_of":"this"},"op":"eq","right":0_u8}},"then":[{"draw":{"n":1_u8}}],"else":[{"draw":{"n":2_u8}}]}}),
        ]);
        let program = lower_stat_control("activate", json!({"cost":{"pp":0_u8},"do":steps}));
        let mut game = event_subject_capture::prepared_program(&stat_position(), &program);
        let activated = activate_control(&mut game);
        resolved(finish(&mut game, activated));
        finish_checks(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"]
                .as_array()
                .unwrap()
                .len(),
            if amulet { 2 } else { 1 }
        );
    }
}
#[test]
fn a_selected_follower_changed_by_this_body_has_no_stat_comparison_information() {
    let program = lower_stat_control(
        "activate",
        json!({"cost":{"pp":0_u8},"targets":[{"bind":"victim","from":{"who":"self","zone":"field","kind":"follower","other":true},"n":1_u8}],"do":[
            {"make_type":{"on":"victim","type":"amulet"}},
            {"if":{"cond":{"cmp":{"left":{"power_of":"victim"},"op":"eq","right":0_u8}},"then":[{"draw":{"n":1_u8}}],"else":[{"draw":{"n":2_u8}}]}}
        ]}),
    );
    let mut position = stat_position();
    position["players"]["P1"]["zones"]["field"] =
        json!([{"id":"lapis","card":"BP16-093","state":{"counters":{"祈りカウンター":4_u8}}}]);
    let mut game = event_subject_capture::prepared_program(&position, &program);
    let activated = activate_control(&mut game);
    resolved(finish(&mut game, activated));
    finish_checks(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["draw-a", "draw-b"])
    );
    assert_eq!(field_stats(&game, Viewer::P1, "BP16-093"), None);
}
#[test]
fn event_subject_presence_uses_the_real_current_or_last_field_view() {
    let program = lower_stat_control(
        "auto",
        json!({"on":{"enter_field":{"who":"self","sel":{"kind":"amulet"}}},"do":[
            {"if":{"cond":{"cmp":{"left":{"power_of":"it"},"op":"eq","right":0_u8}},"then":[{"draw":{"n":1_u8}}],"else":[{"draw":{"n":2_u8}}]}},
            {"draw":{"n":{"power_of":"it"}}}
        ]}),
    );
    let mut position = stat_position();
    position["players"]["P1"]["zones"]["hand"] = json!([{"id":"lapis","card":"BP16-093"}]);
    let mut game = event_subject_capture::prepared_program(&position, &program);
    let entered = game
        .play_internal_card("lapis", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, entered);
    finish_checks(&mut game);
    let observed = pending(&mut game, "producer");
    resolved(finish(&mut game, observed));
    finish_checks(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["draw-a", "draw-b"])
    );
}
#[test]
fn a_new_numeric_modifier_on_an_amulet_is_skipped_but_the_following_grant_still_resolves() {
    let mut subjects = lapis_selector(51);
    let Selector::Zone { predicate, .. } = &mut subjects else {
        panic!("named source selector")
    };
    let Expr::Compute(predicate) = predicate.as_mut() else {
        panic!("name predicate")
    };
    let Predicate::NamesIntersect { right, .. } = predicate.as_mut() else {
        panic!("name match")
    };
    *right = Expr::Literal(vec![CardName("ペインレスサムライ".into())]);
    let program = producer(Action::Modify {
        subjects,
        patch: CharacteristicPatch::SetKind {
            kind: CardKind::Amulet,
        },
        expires: lifetime(DurationEnd::CurrentTurnEnd {}),
        source: EffectReference::CurrentInstallation {},
    });
    let mut position = stat_position();
    position["players"]["P1"]["zones"]["field"] = json!([{"id":"samurai","card":"BP06-021"}]);
    let mut game = event_subject_capture::prepared_program(&position, &program);
    let changed = activate_control(&mut game);
    resolved(finish(&mut game, changed));
    finish_checks(&mut game);
    let sources = game.internal_activated_sources("samurai").unwrap();
    let activated = game
        .activate_internal_card_instance("samurai", sources[0].instance, PlayerId { seat: 1 })
        .unwrap();
    let activation = resolved(finish(&mut game, activated));
    assert_eq!(
        activation
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::StatChange(_)))
            .count(),
        0
    );
    finish_checks(&mut game);
    assert_eq!(field_stats(&game, Viewer::P1, "BP06-021"), None);
    next_turn(&mut game);
    assert_eq!(field_stats(&game, Viewer::P1, "BP06-021"), Some((1, 1)));
    assert!(
        field_state(&game, Viewer::P1, "BP06-021")
            .keywords
            .contains(&"突進".to_owned())
    );
}
#[test]
fn stat_presence_does_not_hide_an_invalid_current_source_after_departure() {
    let mut program = producer(Action::Move {
        objects: Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        },
        movement: MoveSpec {
            to: MoveDestination::Owners {
                zone: OwnerZone::Cemetery,
            },
            initial_posture: None,
            entry_effects: vec![],
            position: None,
            order: None,
        },
        reason: MoveReason::Put,
    })
    .card()
    .clone();
    let source = program.abilities[0].source.clone();
    program.abilities[0]
        .node
        .body
        .as_mut()
        .unwrap()
        .steps
        .push(Located {
            source,
            node: Node::Control {
                control: Control::If {
                    condition: Expr::Compute(Box::new(Predicate::HasInfo {
                        information: NumberRead::Stats {
                            object: ObjectView::Current {
                                object: Box::new(ObjectOperand::SourceHolder {}),
                            },
                            property: Stat::Attack,
                            view: CharacteristicView::Effective,
                        },
                    })),
                    then_body: Program {
                        scope: ScopeId(2),
                        steps: vec![],
                    },
                    else_body: None,
                },
            },
        });
    let program = WellFormedCardProgram::check(program).unwrap();
    let mut game = event_subject_capture::prepared_program(&stat_position(), &program);
    let sources = game.internal_activated_sources("producer").unwrap();
    let error = game
        .activate_internal_card_instance("producer", sources[0].instance, PlayerId { seat: 1 })
        .unwrap_err();
    assert!(error.to_string().contains("stale Current information read"));
}

#[test]
fn a_stat_presence_guard_cannot_borrow_an_unrelated_event_or_lki_reason() {
    let program = lower_stat_control(
        "auto",
        json!({"on":{"enter_field":{"who":"self","sel":{"kind":"amulet"}}},"do":[
            {"if":{"cond":{"cmp":{"left":{"power_of":"it"},"op":"eq","right":0_u8}},"then":[{"draw":{"n":1_u8}}]}}
        ]}),
    );
    for change in 0_u8..3 {
        let mut changed = program.card().clone();
        let body = changed.abilities[0].node.body.as_mut().unwrap();
        let scope = body.scope;
        let Node::Control {
            control: Control::If { condition, .. },
        } = &mut body.steps[0].node
        else {
            panic!("comparison body")
        };
        let Expr::Compute(operation) = condition else {
            panic!("guarded comparison")
        };
        let Predicate::And { predicates } = operation.as_mut() else {
            panic!("presence guard")
        };
        let Expr::Compute(guard_operation) = &mut predicates[0] else {
            panic!("presence predicate")
        };
        let Predicate::HasInfo {
            information: NumberRead::Stats { object, .. },
        } = guard_operation.as_mut()
        else {
            panic!("stat presence information")
        };
        let ObjectView::LastKnown { object, reason } = object else {
            panic!("event information")
        };
        match change {
            0 => **object = ObjectOperand::SourceHolder {},
            1 => *reason = LkiReason::SourceDeparted,
            _ => {
                let ObjectOperand::FromExpression { value } = object.as_mut() else {
                    panic!("event subject")
                };
                let Expr::Frozen(value) = value.as_mut() else {
                    panic!("frozen event")
                };
                let ObjectFrozen::EnterFieldObject { event } = value.as_mut() else {
                    panic!("entry subject")
                };
                *event = Binding::new(999, scope);
            }
        }
        WellFormedCardProgram::check(changed).unwrap_err();
    }
}
