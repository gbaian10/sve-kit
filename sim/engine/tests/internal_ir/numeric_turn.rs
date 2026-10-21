use super::*;

fn position(seat: u32) -> Value {
    let mut position = setup(seat, json!([]));
    position["turn"]["active"] = json!(seat_name(seat));
    for player in ["P1", "P2"] {
        position["players"][player]["zones"]["deck"] = json!(
            (0..12)
                .map(|index| json!({"id":format!("{player}-d{index}"),"card":"unit-follower"}))
                .collect::<Vec<_>>()
        );
        position["players"][player]["pp"] = json!({"current":10,"max":10});
    }
    position
}

fn loaded(spell: Value, abilities: Value) -> Arc<Catalog> {
    let snapshot = ["unit-spell", "unit-follower", "evolved"].iter().map(|card| {
        let (kind, power, hp) = if *card == "unit-spell" { ("スペル", "2", "3") }
            else if *card == "evolved" { ("フォロワー・エボルヴ", "4", "5") }
            else { ("フォロワー", "2", "3") };
        json!({"number":card,"faces":[{"name":"unit-follower","card_class":"ニュートラル","card_type":kind,"cost":"1","power":power,"hp":hp,"traits":[],"text":null,"sections":[]}]}).to_string()
    }).collect::<Vec<_>>().join("\n");
    let mut document = json!({"format":1,"kind":"effect_set","cards":{
        "unit-spell":{"status":"complete","review":"synthetic","abilities":[{"kind":"spell","line":1,"body":null}]},
        "unit-follower":{"status":"complete","review":"synthetic","abilities":[]},
        "evolved":{"status":"complete","review":"synthetic","abilities":[]}
    }});
    document["cards"]["unit-spell"]["abilities"][0]["body"] = spell;
    document["cards"]["unit-follower"]["abilities"] = abilities;
    Arc::new(
        Catalog::from_documents(
            &snapshot,
            &json!({"format":1,"kind":"keyword_registry","keywords":{"guard":{"ja":"守護","rule":"12.8","expansion":{"op":"keyword","name":"guard"}}}}).to_string(),
            &[("turn.yaml".into(), document.to_string())],
        )
        .unwrap(),
    )
}

fn turn_game(seat: u32) -> Game {
    Game::new(
        catalog(),
        &position(seat),
        &Value::Null,
        &Value::Null,
        "turn",
    )
    .unwrap()
}

fn hit(id: u64, amount: i64) -> Located<Node> {
    damage(
        id,
        RecipientSet::Objects {
            objects: zone(ZoneKind::Field, 520_u64.saturating_add(id)),
        },
        amount,
    )
}

const fn seat_name(seat: u32) -> &'static str {
    if seat == 1 { "P1" } else { "P2" }
}

fn current(patch: CharacteristicPatch) -> Located<Node> {
    modify(
        1,
        zone(ZoneKind::Field, 501),
        patch,
        DurationEnd::CurrentTurnEnd {},
    )
}

fn next(player: PlayerRef) -> Located<Node> {
    modify(
        1,
        zone(ZoneKind::Field, 502),
        pair(1, 2),
        DurationEnd::NextTurnEnd { player },
    )
}

fn stats(board: &Game, attack: i64, hp: i64, maximum: i64) {
    let before = exact_authority(board);
    for view in [View::Referee, View::P1, View::P2] {
        let projection = board.projection(view).unwrap();
        let object = &projection["objects"]["f"];
        assert_eq!(object["power"], attack, "effective attack in every view");
        assert_eq!(object["hp"], hp, "effective life in every view");
        assert_eq!(object["max_hp"], maximum, "undamaged life in every view");
        assert_eq!(
            object["damage"],
            maximum.checked_sub(hp).unwrap(),
            "current damage is preserved"
        );
    }
    assert_eq!(
        exact_authority(board),
        before,
        "projection cannot consume occurrences"
    );
}

fn end(board: &mut Game) -> Vec<Value> {
    let mut events = board
        .decide(&json!({"do":"end-phase"}), "end")
        .unwrap()
        .events;
    events.extend(
        board
            .decide(&json!({"do":"pass"}), "quick-pass")
            .unwrap()
            .events,
    );
    events
}

fn continuous(id: u64, enabled: bool, end: DurationEnd) -> Located<Node> {
    let affected = Binding::new(id.saturating_add(700), ScopeId(id.saturating_add(700)));
    let duration = Duration {
        end,
        anchor: TimeAnchor::Installation {},
        usage: None,
        intervals: vec![],
    };
    let origin = AbilitySource {
        holder: ObjectOperand::SourceHolder {},
        origin: AbilityOrigin::Printed {
            face: FaceId("0".into()),
            line: Nat(1),
        },
        controller: PlayerRef::Controller {},
    };
    instruction(
        id,
        Action::InstallEffect {
            subjects: InstallationSubjects::Objects {
                objects: opponents_field(id.saturating_add(800)),
                affected,
            },
            capture: InstallationTime::InstructionStart,
            effect: Box::new(Effect::Continuous {
                id: EffectDefinitionId(id),
                source: origin,
                target: EffectTarget::Dynamic {
                    selector: Selector::Bound {
                        reference: affected,
                    },
                },
                condition: Some(Expr::Literal(Bool(enabled))),
                patch: pair(1, 2),
                layer: ContinuousLayer::Numeric,
                timestamp: TimestampBasis::ProperPlay {
                    execution: ExecutionRef::Current {},
                },
                active_zones: vec![],
                duration: duration.clone(),
            }),
            duration,
        },
    )
}

#[test]
fn continuous_source_exit_preserves_damage_and_false_instances_without_stat_facts() {
    for seat in [1, 2] {
        let mut initial = position(seat);
        initial["players"][seat_name(seat)]["zones"]["hand"] = json!([]);
        initial["players"][seat_name(seat)]["zones"]["field"] =
            json!([{"id":"s","card":"unit-follower"}]);
        let mut board = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "continuous",
        )
        .unwrap();
        let mut inactive = board.clone();
        let result = run(
            &mut inactive,
            vec![continuous(1, false, DurationEnd::NoTemporalExpiry {})],
            seat,
        );
        assert_eq!(changes(&result).len(), 0);
        stats(&inactive, 2, 3, 3);
        assert_eq!(
            inactive.projection(View::Referee).unwrap()["semantic_state"]["typed_numeric_layers"],
            true
        );
        let result = run(
            &mut board,
            vec![
                continuous(
                    1,
                    true,
                    DurationEnd::SourceZoneExit {
                        source: ObjectOperand::SourceHolder {},
                        zone: ZoneKind::Field,
                    },
                ),
                hit(2, 1),
            ],
            seat,
        );
        assert_eq!(changes(&result).len(), 0);
        stats(&board, 3, 4, 5);
        let result = run(
            &mut board,
            vec![movement::put(
                1,
                Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
                OwnerZone::Cemetery,
            )],
            seat,
        );
        assert_eq!(changes(&result).len(), 0);
        stats(&board, 2, 2, 3);
    }
}

#[test]
fn added_turn_interval_is_inactive_in_the_gap_and_resumes_for_the_actual_opponent_turn() {
    for seat in [1, 2] {
        let mut board = Game::new(
            loaded(
                json!({"op":"extra_turn","side":"self","count":1}),
                json!([]),
            ),
            &position(seat),
            &Value::Null,
            &Value::Null,
            "interval",
        )
        .unwrap();
        let mut step = current(pair(1, 2));
        if let Node::Instruction { instruction } = &mut step.node
            && let Action::Modify { expires, .. } = &mut instruction.action
        {
            expires.intervals.push(Interval::DuringNextTurn {
                player: PlayerRef::RequireOne {
                    players: Box::new(players(PlayerRelation::Opponents)),
                },
            });
        }
        run(&mut board, vec![step], seat);
        board.submit(&json!({"do":"play","card":"s"})).unwrap();
        stats(&board, 3, 5, 5);
        end(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()["turn"]["active"],
            seat_name(seat)
        );
        stats(&board, 2, 3, 3);
        let mut departed = board.clone();
        departed
            .execute_internal(
                &checked(vec![instruction(
                    1,
                    Action::Destroy {
                        objects: opponents_field(980),
                    },
                )]),
                &source("synthetic"),
                Context {
                    generation: Generation(2),
                    ..context(seat)
                },
            )
            .unwrap();
        end(&mut departed);
        assert_eq!(
            departed.projection(View::Referee).unwrap()["objects"]["f"]["generation"],
            1
        );
        stats(&departed, 2, 3, 3);
        end(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()["turn"]["active"],
            seat_name(3 - seat)
        );
        stats(&board, 3, 5, 5);
        end(&mut board);
        stats(&board, 2, 3, 3);
    }
}

#[test]
fn current_turn_end_remains_through_end_triggers_quick_and_discard_checks() {
    for seat in [1, 2] {
        let mut position = position(seat);
        let actor = seat_name(seat);
        position["players"][actor]["zones"]["hand"] = json!((0..8)
            .map(|index| json!({"id":if index == 0 { "s".into() } else { format!("h{index}") },"card":"unit-spell"}))
            .collect::<Vec<_>>());
        let abilities = json!([{"kind":"trigger","line":1,"event":"end","side":"both","body":{"op":"draw","side":"self","count":1}}]);
        let mut board = Game::new(
            loaded(json!({"op":"draw","count":1,"side":"self"}), abilities),
            &position,
            &Value::Null,
            &Value::Null,
            "end",
        )
        .unwrap();
        let progress = run(&mut board, vec![current(pair(1, 2))], seat);
        assert_eq!(changes(&progress).len(), 1, "one explicit numeric fact");
        board.decide(&json!({"do":"end-phase"}), "end").unwrap();
        stats(&board, 3, 5, 5);
        let pending = board
            .legal()
            .unwrap()
            .into_iter()
            .find(|choice| choice["do"] == "choose-pending")
            .unwrap();
        board.decide(&pending, "end-trigger").unwrap();
        stats(&board, 3, 5, 5);
        board.decide(&json!({"do":"pass"}), "quick").unwrap();
        stats(&board, 3, 5, 5);
        let discard = board
            .legal()
            .unwrap()
            .into_iter()
            .find(|choice| choice["do"] == "end-discard")
            .unwrap();
        let mut wrong = discard.clone();
        wrong["by"] = json!(seat_name(3 - seat));
        assert_eq!(
            board.decide(&wrong, "bad-actor").unwrap().outcome,
            "cannot-play"
        );
        stats(&board, 3, 5, 5);
        board.decide(&discard, "discard").unwrap();
        stats(&board, 2, 3, 3);
        assert_eq!(
            board.projection(View::Referee).unwrap()["turn"]["active"],
            seat_name(3 - seat)
        );
        assert_eq!(
            changes(&progress).len(),
            1,
            "expiry creates no additional typed fact"
        );
    }
}

#[test]
fn next_turn_end_excludes_the_current_turn_and_survives_save_restore() {
    for seat in [1, 2] {
        let mut board = turn_game(seat);
        run(&mut board, vec![next(PlayerRef::Controller {})], seat);
        end(&mut board);
        stats(&board, 3, 5, 5);
        let saved = serde_json::to_value(&board).unwrap();
        let mut board: Game = serde_json::from_value(saved).unwrap();
        end(&mut board);
        stats(&board, 3, 5, 5);
        end(&mut board);
        stats(&board, 2, 3, 3);
    }
}

#[test]
fn skipped_turns_have_no_occurrence_and_extra_turns_bind_only_when_they_begin() {
    for seat in [1, 2] {
        let mut position = position(seat);
        position["semantic_state"]["turn_schedule"] = json!({
            "extra":[{"player":seat_name(seat),"count":2}],
            "skipped":[{"player":seat_name(3-seat),"count":1}]
        });
        let mut board = Game::new(
            catalog(),
            &position,
            &Value::Null,
            &Value::Null,
            "skip-extra",
        )
        .unwrap();
        run(
            &mut board,
            vec![next(PlayerRef::RequireOne {
                players: Box::new(players(PlayerRelation::Opponents)),
            })],
            seat,
        );
        for _ in 0..3 {
            end(&mut board);
            assert_eq!(
                board.projection(View::Referee).unwrap()["turn"]["active"],
                seat_name(seat)
            );
            stats(&board, 3, 5, 5);
        }
        end(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()["turn"]["active"],
            seat_name(3 - seat)
        );
        stats(&board, 3, 5, 5);
        end(&mut board);
        stats(&board, 2, 3, 3);

        let mut board = Game::new(
            catalog(),
            &position,
            &Value::Null,
            &Value::Null,
            "first-extra",
        )
        .unwrap();
        run(&mut board, vec![next(PlayerRef::Controller {})], seat);
        end(&mut board);
        stats(&board, 3, 5, 5);
        end(&mut board);
        stats(&board, 2, 3, 3);
    }
}

#[test]
fn schedule_changes_after_installation_move_next_turn_expiry() {
    for seat in [1, 2] {
        // Each case: spell body, waited player, and the active player after each end before expiry.
        for (body, waiting, actives) in [
            (
                json!({"op":"extra_turn","side":"self","count":1}),
                PlayerRef::Controller {},
                vec![seat],
            ),
            (
                json!({"op":"extra_turn","side":"self","count":1}),
                PlayerRef::RequireOne {
                    players: Box::new(players(PlayerRelation::Opponents)),
                },
                vec![seat, 3 - seat],
            ),
            (
                json!({"op":"skip_turn","side":"opponent","count":1}),
                PlayerRef::RequireOne {
                    players: Box::new(players(PlayerRelation::Opponents)),
                },
                vec![seat, 3 - seat],
            ),
        ] {
            let mut board = Game::new(
                loaded(body, json!([])),
                &position(seat),
                &Value::Null,
                &Value::Null,
                "late-schedule",
            )
            .unwrap();
            run(&mut board, vec![next(waiting)], seat);
            let play = board
                .legal()
                .unwrap()
                .into_iter()
                .find(|choice| choice["do"] == "play")
                .unwrap();
            board.decide(&play, "schedule").unwrap();
            stats(&board, 3, 5, 5);
            for active in actives {
                end(&mut board);
                assert_eq!(
                    board.projection(View::Referee).unwrap()["turn"]["active"],
                    seat_name(active),
                    "the schedule changed after installation decides the next actual turn"
                );
                stats(&board, 3, 5, 5);
            }
            end(&mut board);
            stats(&board, 2, 3, 3);
        }
    }
}

#[test]
fn expiry_lethal_is_destroyed_before_main_start_triggers_wait_for_a_choice() {
    for seat in [1, 2] {
        let opponent = seat_name(3 - seat);
        let mut position = position(seat);
        position["players"][opponent]["zones"]["field"] = json!([
            {"id":"f","card":"unit-follower"},
            {"id":"g","card":"unit-follower","state":{"hp":9,"max_hp":9}}
        ]);
        let abilities = json!([{"kind":"trigger","line":1,"event":"main_start","side":"self","body":{"op":"draw","side":"self","count":1}}]);
        let mut board = Game::new(
            loaded(json!({"op":"draw","side":"self","count":1}), abilities),
            &position,
            &Value::Null,
            &Value::Null,
            "main-start",
        )
        .unwrap();
        run(&mut board, vec![current(pair(0, 2)), hit(2, 4)], seat);
        stats(&board, 2, 1, 5);
        let hand = |board: &Game| {
            board.projection(View::Referee).unwrap()[opponent]["hand"]
                .as_array()
                .unwrap()
                .len()
        };
        let before = hand(&board);
        end(&mut board);
        let projection = board.projection(View::Referee).unwrap();
        assert_eq!(projection["turn"]["active"], opponent);
        assert_eq!(projection["turn"]["phase"], "main");
        assert_eq!(
            projection["objects"]["f"]["generation"], 1,
            "the expired follower is destroyed at 7.2.5"
        );
        assert_eq!(
            projection["objects"]["g"]["hp"], 5,
            "the survivor expires too"
        );
        let pending = board
            .legal()
            .unwrap()
            .into_iter()
            .filter(|choice| choice["do"] == "choose-pending")
            .collect::<Vec<_>>();
        assert_eq!(
            pending.len(),
            1,
            "only the survivor's main_start trigger waits"
        );
        assert_eq!(pending[0]["pending"]["ability"]["source"], "g");
        assert_eq!(
            hand(&board),
            before + 1,
            "only the 7.2.4 draw has happened before the choice"
        );
        board.decide(&pending[0], "main-start").unwrap();
        assert_eq!(
            hand(&board),
            before + 2,
            "the trigger resolves after it is chosen"
        );
    }
}

#[test]
fn expiry_lethal_waits_for_start_stand_and_draw_before_rule_destruction() {
    for seat in [1, 2] {
        let mut position = position(seat);
        position["players"][seat_name(3 - seat)]["zones"]["field"][0]["state"] =
            json!({"acted":true});
        let mut board =
            Game::new(catalog(), &position, &Value::Null, &Value::Null, "lethal").unwrap();
        run(&mut board, vec![current(pair(0, 2)), hit(2, 4)], seat);
        stats(&board, 2, 1, 5);
        let events = end(&mut board);
        let stand = events
            .iter()
            .position(|event| event["kind"] == "スタンド" && event["object"] == "f")
            .unwrap();
        let draw = events
            .iter()
            .position(|event| event["kind"] == "引く")
            .unwrap();
        let death = events
            .iter()
            .position(|event| event["kind"] == "移動" && event["object"] == "f")
            .unwrap();
        assert!(
            stand < draw && draw < death,
            "7.4.8 has no check: stand, draw, then destruction"
        );
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["f"]["generation"],
            1
        );
    }
}

#[test]
fn numeric_expiry_overflow_rolls_back_the_pass_and_turn_schedule() {
    for seat in [1, 2] {
        let mut board = turn_game(seat);
        run(
            &mut board,
            vec![set::fixed(set::set_patch(Stat::Attack, i64::MAX))],
            seat,
        );
        run(&mut board, vec![current(pair(-1, 0))], seat);
        run(&mut board, vec![buff(1, 1, 0)], seat);
        board.decide(&json!({"do":"end-phase"}), "end").unwrap();
        let before = exact_authority(&board);
        board.decide(&json!({"do":"pass"}), "overflow").unwrap_err();
        assert_eq!(
            exact_authority(&board),
            before,
            "rollback includes closed flag, IDs, layers, RNG and input"
        );
        stats(&board, i64::MAX, 3, 3);
    }
}

#[test]
fn turn_expiry_recomputes_remaining_set_and_preserves_new_damage() {
    for seat in [1, 2] {
        let mut board = turn_game(seat);
        run(&mut board, vec![current(pair(2, 2))], seat);
        run(
            &mut board,
            vec![set::fixed(set::set_pair(2)), hit(2, 1)],
            seat,
        );
        end(&mut board);
        stats(&board, 2, 1, 2);
        let mut board = turn_game(seat);
        run(&mut board, vec![hit(1, 1)], seat);
        run(&mut board, vec![current(set::set_pair(4))], seat);
        run(&mut board, vec![hit(1, 1)], seat);
        end(&mut board);
        stats(&board, 2, 1, 3);
    }
}

#[test]
fn installation_captures_controller_instead_of_owner_or_a_future_controller() {
    // The owner is the other player; the controller waits past that player's turn.
    for (role, ends_before_expiry) in [(Role::Controller, 2), (Role::Owner, 1)] {
        for seat in [1, 2] {
            let mut position = position(seat);
            position["players"][seat_name(seat)]["zones"]["hand"] = json!([]);
            position["players"][seat_name(seat)]["zones"]["field"] =
                json!([{"id":"s","card":"unit-follower","owner":seat_name(3-seat)}]);
            let abilities = json!([{"kind":"activated","line":1,"body":{"op":"control","subjects":"self","side":"opponent"}}]);
            let mut board = Game::new(
                loaded(json!({"op":"draw","side":"self","count":1}), abilities),
                &position,
                &Value::Null,
                &Value::Null,
                "capture",
            )
            .unwrap();
            let captured = CaptureRef {
                binding: Binding::new(33, ScopeId(1)),
            };
            let capture = node(Node::Capture {
                capture: Capture::Player {
                    at: CaptureTime::AtResolution { scope: ScopeId(1) },
                    value: Expr::Live(Box::new(PlayerRead::Role {
                        object: ObjectView::Current {
                            object: Box::new(ObjectOperand::SourceHolder {}),
                        },
                        role,
                    })),
                    into: captured.clone(),
                },
            });
            let player = PlayerRef::FromExpression {
                value: Box::new(Expr::Captured(captured)),
            };
            run(
                &mut board,
                vec![
                    capture,
                    modify(
                        1,
                        zone(ZoneKind::Field, 530),
                        pair(1, 2),
                        DurationEnd::NextTurnEnd { player },
                    ),
                ],
                seat,
            );
            let activation = board
                .legal()
                .unwrap()
                .into_iter()
                .find(|choice| choice["do"] == "activate" && choice["ability"]["source"] == "s")
                .unwrap();
            board.decide(&activation, "control").unwrap();
            assert!(
                board.projection(View::Referee).unwrap()[seat_name(3 - seat)]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("s")),
                "the target transfers to the other player's field"
            );
            assert_eq!(
                board.projection(View::Referee).unwrap()["objects"]["s"]["power"],
                3,
                "a transferred target keeps its numeric layer and generation"
            );
            for _ in 0..ends_before_expiry {
                end(&mut board);
                stats(&board, 3, 5, 5);
                assert_eq!(
                    board.projection(View::Referee).unwrap()["objects"]["s"]["power"],
                    3,
                    "only the captured player's next actual turn can expire the layer"
                );
            }
            end(&mut board);
            stats(&board, 2, 3, 3);
            assert_eq!(
                board.projection(View::Referee).unwrap()["objects"]["s"]["power"],
                2,
                "expiry follows the captured player"
            );
        }
    }
}

#[test]
fn untimed_add_evolution_updates_the_base_and_timed_or_set_evolution_rolls_back() {
    for seat in [1, 2] {
        let mut position = position(seat);
        position["players"][seat_name(3 - seat)]["zones"]["field"] = json!([]);
        position["players"][seat_name(seat)]["zones"]["field"] =
            json!([{"id":"f","card":"unit-follower"}]);
        position["players"][seat_name(seat)]["zones"]["evolve_deck"] =
            json!([{"id":"e","card":"evolved"}]);
        let abilities = json!([{"kind":"evolve","line":1,"costs":[{"op":"pp","amount":1}],"body":{"op":"evolve","subjects":"self"}}]);
        let loaded = loaded(json!({"op":"draw","side":"self","count":1}), abilities);
        // Only an untimed Add looks the same whether evolution keeps it as a layer or a difference.
        for (layer, accepted) in [
            (buff(1, 1, 2), true),
            (current(pair(1, 2)), false),
            (set::fixed(set::set_pair(4)), false),
        ] {
            let mut board = Game::new(
                Arc::clone(&loaded),
                &position,
                &Value::Null,
                &Value::Null,
                "evolution",
            )
            .unwrap();
            run(&mut board, vec![layer, hit(2, 1)], seat);
            let evolve = board
                .legal()
                .unwrap()
                .into_iter()
                .find(|choice| choice["do"] == "evolve")
                .unwrap();
            let before = exact_authority(&board);
            if accepted {
                board.decide(&evolve, "add-evolution").unwrap();
                stats(&board, 5, 6, 7);
                end(&mut board);
                stats(&board, 5, 6, 7);
            } else {
                board.decide(&evolve, "undecided-evolution").unwrap_err();
                assert_eq!(
                    exact_authority(&board),
                    before,
                    "the legal evolution rejects with payment and movement restored"
                );
            }
        }
    }
}

#[test]
fn prototype_grants_cannot_add_unadmitted_triggers_beside_numeric_layers() {
    let followers = json!({"side":"both","zone":"field","type":"follower"});
    let grants = [
        json!({"op":"modify","subjects":followers,"abilities":[
            {"kind":"trigger","line":9,"event":"leave","subject":"self","body":{"op":"draw","side":"self","count":1}}
        ]}),
        json!({"op":"modify","subjects":followers,"keywords":["guard"]}),
    ];
    for seat in [1, 2] {
        for grant in &grants {
            for layered in [true, false] {
                let mut board = Game::new(
                    loaded(grant.clone(), json!([])),
                    &position(seat),
                    &Value::Null,
                    &Value::Null,
                    "grant",
                )
                .unwrap();
                if layered {
                    run(&mut board, vec![current(pair(1, 2))], seat);
                }
                let play = board
                    .legal()
                    .unwrap()
                    .into_iter()
                    .find(|choice| choice["do"] == "play")
                    .unwrap();
                let before = exact_authority(&board);
                if layered {
                    board.decide(&play, "grant").unwrap_err();
                    assert_eq!(
                        exact_authority(&board),
                        before,
                        "a legal grant beside typed layers restores the whole play"
                    );
                    stats(&board, 3, 5, 5);
                } else {
                    board.decide(&play, "grant").unwrap();
                }
            }
        }
    }
}

#[test]
fn prototype_add_damage_and_unsupported_base_writes_share_the_numeric_boundary() {
    for seat in [1, 2] {
        for (body, allowed, expected) in [
            (
                json!({"op":"modify","subjects":{"side":"both","zone":"field","type":"follower"},"power":1,"hp":1}),
                true,
                (4, 6, 6),
            ),
            (
                json!({"op":"damage","subjects":{"side":"both","zone":"field","type":"follower"},"amount":1}),
                true,
                (3, 4, 5),
            ),
            (
                json!({"op":"modify","subjects":{"side":"both","zone":"field","type":"follower"},"set_hp":4}),
                false,
                (3, 5, 5),
            ),
            (
                json!({"op":"pilot","subjects":{"side":"both","zone":"field","type":"follower"},"by":"self"}),
                false,
                (3, 5, 5),
            ),
            (
                json!({"op":"become_type","subjects":{"side":"both","zone":"field","type":"follower"},"type":"amulet"}),
                false,
                (3, 5, 5),
            ),
        ] {
            let mut board = Game::new(
                loaded(body, json!([])),
                &position(seat),
                &Value::Null,
                &Value::Null,
                "writes",
            )
            .unwrap();
            run(&mut board, vec![current(pair(1, 2))], seat);
            let play = board
                .legal()
                .unwrap()
                .into_iter()
                .find(|choice| choice["do"] == "play")
                .unwrap();
            let before = exact_authority(&board);
            if allowed {
                board.decide(&play, "write").unwrap();
                stats(&board, expected.0, expected.1, expected.2);
                end(&mut board);
                stats(&board, expected.0 - 1, expected.1 - 2, expected.2 - 2);
            } else {
                board.decide(&play, "reject-write").unwrap_err();
                assert_eq!(
                    exact_authority(&board),
                    before,
                    "unsupported base writes restore the whole play"
                );
                stats(&board, expected.0, expected.1, expected.2);
            }
        }
    }
}

#[test]
fn prototype_pilot_blocks_typed_installation_on_its_target() {
    for seat in [1, 2] {
        for pilot in [false, true] {
            let body = if pilot {
                json!({"op":"pilot","subjects":{"side":"both","zone":"field","type":"follower"},"by":"self"})
            } else {
                json!({"op":"draw","side":"self","count":1})
            };
            let mut position = position(seat);
            position["players"][seat_name(seat)]["zones"]["hand"] =
                json!([{"id":"s","card":"unit-spell"},{"id":"p","card":"unit-spell"}]);
            let mut board = Game::new(
                loaded(body, json!([])),
                &position,
                &Value::Null,
                &Value::Null,
                "pilot",
            )
            .unwrap();
            board
                .decide(&json!({"do":"play","card":"p"}), "prototype")
                .unwrap();
            if pilot {
                // Pilot restoration writes prior stats back, so typed layers cannot coexist with it.
                unchanged(&mut board, &checked(vec![current(pair(1, 2))]), seat);
                stats(&board, 2, 3, 3);
            } else {
                run(&mut board, vec![current(pair(1, 2))], seat);
                stats(&board, 3, 5, 5);
            }
        }
    }
}

#[test]
fn saturating_prototype_arithmetic_cannot_reach_numeric_layers() {
    let followers = json!({"side":"both","zone":"field","type":"follower"});
    let overflow = json!({"fn":"add","args":[i64::MAX, 1]});
    // Each case: prototype body, typed layer present, accepted Attack (None rejects).
    let cases = [
        (
            json!({"op":"modify","subjects":followers,"power":overflow}),
            true,
            None,
        ),
        (
            json!({"op":"damage","subjects":followers,"amount":overflow}),
            true,
            None,
        ),
        (
            json!({"op":"seq","steps":[
                {"op":"replace_damage","subjects":followers,"kind":"any","amount":1},
                {"op":"damage","subjects":followers,"amount":i64::MAX}
            ]}),
            true,
            None,
        ),
        (
            json!({"op":"modify","subjects":followers,"power":i64::MAX}),
            true,
            Some(i64::MAX - 1),
        ),
        (
            json!({"op":"modify","subjects":followers,"power":overflow}),
            false,
            Some(i64::MAX),
        ),
    ];
    for seat in [1, 2] {
        for (body, layered, accepted) in &cases {
            let mut position = position(seat);
            position["players"][seat_name(3 - seat)]["zones"]["field"][0]["state"] =
                json!({"power":0});
            let mut board = Game::new(
                loaded(body.clone(), json!([])),
                &position,
                &Value::Null,
                &Value::Null,
                "saturation",
            )
            .unwrap();
            if *layered {
                run(
                    &mut board,
                    vec![current(patch(Stat::Attack, Expr::Literal(Int(-1))))],
                    seat,
                );
            }
            let play = board
                .legal()
                .unwrap()
                .into_iter()
                .find(|choice| choice["do"] == "play")
                .unwrap();
            let before = exact_authority(&board);
            if let Some(attack) = accepted {
                board.decide(&play, "checked").unwrap();
                assert_eq!(
                    board.projection(View::Referee).unwrap()["objects"]["f"]["power"],
                    *attack
                );
            } else {
                board.decide(&play, "saturating").unwrap_err();
                assert_eq!(
                    exact_authority(&board),
                    before,
                    "a saturated prototype operand restores the whole play"
                );
            }
        }
    }
}

#[test]
fn an_input_that_removes_the_last_layer_still_rejects_new_waits() {
    let opponents = json!({"side":"opponent","zone":"field","type":"follower"});
    let destroy = json!({"op":"destroy","subjects":opponents});
    let leave = json!({"kind":"trigger","line":9,"event":"leave","subject":"self","body":{"op":"draw","side":"self","count":1}});
    let grant = |subjects: &Value| json!({"op":"modify","subjects":subjects,"abilities":[leave]});
    let mine = json!({"side":"self","zone":"field","type":"follower"});
    for seat in [1, 2] {
        for (body, accepted) in [
            // A live grant on a follower that keeps no layer.
            (json!({"op":"seq","steps":[destroy, grant(&mine)]}), false),
            // The grant leaves with f's generation but its leave trigger is already waiting.
            (
                json!({"op":"seq","steps":[grant(&opponents), destroy]}),
                false,
            ),
            // A delayed leave watch would wait after f is destroyed by rules.
            (
                json!({"op":"seq","steps":[{"op":"delay","event":"leave","subjects":opponents,"body":{"op":"draw","side":"self","count":1}}, destroy]}),
                false,
            ),
            (json!({"op":"seq","steps":[destroy]}), true),
        ] {
            let mut position = position(seat);
            position["players"][seat_name(seat)]["zones"]["hand"] =
                json!([{"id":"s","card":"unit-spell"},{"id":"g","card":"unit-follower"}]);
            let mut board = Game::new(
                loaded(body, json!([])),
                &position,
                &Value::Null,
                &Value::Null,
                "last-layer",
            )
            .unwrap();
            run(&mut board, vec![current(pair(1, 2))], seat);
            board
                .decide(&json!({"do":"play","card":"g"}), "unlayered-follower")
                .unwrap();
            let before = exact_authority(&board);
            let outcome = board.decide(&json!({"do":"play","card":"s"}), "last-layer");
            if accepted {
                outcome.unwrap();
                assert!(
                    board.projection(View::Referee).unwrap()["semantic_state"]
                        .get("typed_numeric_layers")
                        .is_none(),
                    "destroying the only layered follower removes the last layer"
                );
            } else {
                outcome.unwrap_err();
                assert_eq!(
                    exact_authority(&board),
                    before,
                    "a new wait in the input that ends the last layer is still rejected"
                );
            }
        }
    }
}

#[test]
fn grants_return_once_expiry_clears_the_last_layer() {
    let followers = json!({"side":"both","zone":"field","type":"follower"});
    let body = json!({"op":"modify","subjects":followers,"abilities":[
        {"kind":"trigger","line":9,"event":"leave","subject":"self","body":{"op":"draw","side":"self","count":1}}
    ]});
    for seat in [1, 2] {
        let mut position = position(seat);
        position["players"][seat_name(3 - seat)]["zones"]["hand"] =
            json!([{"id":"p","card":"unit-spell"}]);
        let mut board = Game::new(
            loaded(body.clone(), json!([])),
            &position,
            &Value::Null,
            &Value::Null,
            "after-expiry",
        )
        .unwrap();
        run(&mut board, vec![current(pair(1, 2))], seat);
        end(&mut board);
        stats(&board, 2, 3, 3);
        board
            .decide(&json!({"do":"play","card":"p"}), "after-expiry")
            .unwrap();
    }
}

#[test]
fn closed_turn_and_unadmitted_intervals_are_rejected_without_consuming_authority() {
    for seat in [1, 2] {
        let mut position = position(seat);
        position["semantic_state"]["turn_schedule"] = json!({"closed":true});
        let mut board =
            Game::new(catalog(), &position, &Value::Null, &Value::Null, "closed").unwrap();
        unchanged(
            &mut board,
            &checked(vec![draw(2, 1), current(pair(1, 2))]),
            seat,
        );
        for end in [
            DurationEnd::CurrentTurnEnd {},
            DurationEnd::EndResolution {},
        ] {
            let mut step = modify(1, zone(ZoneKind::Field, 540), pair(1, 2), end.clone());
            if let Node::Instruction { instruction } = &mut step.node
                && let Action::Modify { expires, .. } = &mut instruction.action
            {
                expires
                    .intervals
                    .push(if end == (DurationEnd::CurrentTurnEnd {}) {
                        Interval::DuringNextPhase {
                            player: PlayerRef::Controller {},
                            phase: Phase::Main,
                        }
                    } else {
                        Interval::DuringNextTurn {
                            player: PlayerRef::Controller {},
                        }
                    });
            }
            unchanged(&mut turn_game(seat), &checked(vec![draw(2, 1), step]), seat);
        }
    }
}

#[test]
fn pending_resolution_cannot_advance_the_turn_and_late_end_installation_still_expires() {
    for seat in [1, 2] {
        let mut board = turn_game(seat);
        let progress = run(
            &mut board,
            vec![current(pair(1, 2)), integer_choice(2, 1)],
            seat,
        );
        assert!(
            !progress.complete,
            "the numeric layer is installed before the question"
        );
        let before = exact_authority(&board);
        board
            .decide(&json!({"do":"end-phase"}), "busy")
            .unwrap_err();
        assert_eq!(
            exact_authority(&board),
            before,
            "a pending answer owns the turn input"
        );
        stats(&board, 3, 5, 5);
        let question = board.internal_decision(View::Referee).unwrap();
        board
            .answer_internal(PlayerId { seat }, question.token, Answer::Integer(Nat(0)))
            .unwrap();
        end(&mut board);
        stats(&board, 2, 3, 3);
        board
            .decide(&json!({"do":"end-phase"}), "late-end")
            .unwrap();
        // The source remains controlled by the original actor during the opponent's Quick window.
        run(&mut board, vec![current(pair(2, 2))], seat);
        stats(&board, 4, 5, 5);
        board.decide(&json!({"do":"pass"}), "late-pass").unwrap();
        stats(&board, 2, 3, 3);
    }
}

#[test]
fn simultaneous_expiry_has_no_intermediate_lethal_check_or_stat_trigger() {
    for seat in [1, 2] {
        let mut board = turn_game(seat);
        let progress = run(
            &mut board,
            vec![
                current(pair(-4, -4)),
                modify(
                    2,
                    zone(ZoneKind::Field, 550),
                    pair(4, 4),
                    DurationEnd::CurrentTurnEnd {},
                ),
            ],
            seat,
        );
        assert_eq!(
            changes(&progress).len(),
            2,
            "each explicit instruction has its own fact"
        );
        let events = end(&mut board);
        assert!(
            !events
                .iter()
                .any(|event| event["object"] == "f" && event["kind"] == "移動"),
            "batch expiry cannot expose the intermediate negative life"
        );
        stats(&board, 2, 3, 3);
        assert!(
            board.projection(View::Referee).unwrap()["semantic_state"]
                .get("typed_numeric_layers")
                .is_none(),
            "the last layer releases the stored base"
        );
    }
}
