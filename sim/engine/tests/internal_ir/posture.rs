use super::*;

fn subjects(scope: u64) -> Selector {
    Selector::Zone {
        players: players(PlayerRelation::Opponents),
        zone: ZoneKind::Field,
        view: SelectionView::Current {},
        binding: Binding::new(9, ScopeId(scope)),
        predicate: Box::new(Expr::Literal(Bool(true))),
    }
}
fn lock() -> Action {
    let affected = Binding::new(1, ScopeId(902));
    let targets = Selector::Bound {
        reference: affected,
    };
    let player = PlayerRef::RequireOne {
        players: Box::new(players(PlayerRelation::Opponents)),
    };
    let duration = Duration {
        end: DurationEnd::NextPhaseEdge {
            player: player.clone(),
            phase: Phase::Start,
            edge: Edge::End,
        },
        anchor: TimeAnchor::Installation {},
        usage: None,
        intervals: vec![],
    };
    Action::InstallEffect {
        subjects: InstallationSubjects::Objects {
            objects: subjects(901),
            affected,
        },
        capture: InstallationTime::InstructionStart,
        effect: Box::new(Effect::Permission {
            subject: PermissionSubject::Objects {
                objects: targets.clone(),
            },
            action: ActionPattern::Stand {
                objects: targets,
                scope: StandScope::RuleStart { player },
            },
            polarity: Polarity::Forbid,
            condition: Expr::Literal(Bool(true)),
            duration: duration.clone(),
            rule: RuleId(723),
        }),
        duration,
    }
}
fn run(
    game: &mut Game,
    actions: Vec<Action>,
    seat: u32,
) -> Result<Progress, sve_engine::EngineFailure> {
    let steps = actions
        .into_iter()
        .enumerate()
        .map(|(index, action)| {
            instruction(
                u64::try_from(index).unwrap().checked_add(1).unwrap(),
                action,
            )
        })
        .collect();
    game.execute_internal(&checked(steps), &source("synthetic"), context(seat))
}
fn end(game: &mut Game) {
    if game.internal_card_stage().is_some() {
        game.resume_internal_card().unwrap();
    }
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let question = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
        .unwrap();
}
fn board(seat: u32) -> Game {
    scheduled_board(seat, Value::Null)
}

fn scheduled_board(seat: u32, schedule: Value) -> Game {
    let mut initial = setup(seat, json!([]));
    initial["semantic_state"]["turn_schedule"] = schedule;
    initial["turn"]["active"] = json!(if seat == 1 { "P1" } else { "P2" });
    let actor = if seat == 1 { "P1" } else { "P2" };
    let opponent = if seat == 1 { "P2" } else { "P1" };
    initial["players"][actor]["zones"]["hand"] = json!([]);
    initial["players"][actor]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"][opponent]["zones"]["field"][0]["state"] = json!({"acted":true});
    for player in [actor, opponent] {
        initial["players"][player]["zones"]["deck"] = json!(
            (0..8)
                .map(|i| json!({"id":format!("{player}-d{i}"),"card":"unit-spell"}))
                .collect::<Vec<_>>()
        );
    }
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "stand").unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for name in ["unit-follower", "unit-spell"] {
        game.load_internal_card(name, &empty).unwrap();
    }
    game
}
#[test]
fn no_stand_only_blocks_one_rule_start_and_never_effect_stand() {
    for seat in [1, 2] {
        let mut game = board(seat);
        run(&mut game, vec![lock()], seat).unwrap();
        let mut effect = game.clone();
        run(
            &mut effect,
            vec![Action::Posture {
                objects: subjects(903),
                posture: Posture::Standing,
            }],
            seat,
        )
        .unwrap();
        assert_eq!(
            effect.projection(View::Referee).unwrap()["objects"]["f"]["acted"],
            false
        );
        end(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["f"]["acted"],
            true
        );
        end(&mut game);
        end(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["f"]["acted"],
            false
        );
    }
}
#[test]
fn no_stand_survives_source_departure_but_not_target_new_generation() {
    let mut game = board(1);
    run(
        &mut game,
        vec![
            lock(),
            Action::Move {
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
            },
        ],
        1,
    )
    .unwrap();
    end(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["f"]["acted"],
        true
    );

    let mut game = board(1);
    run(
        &mut game,
        vec![
            lock(),
            Action::Move {
                objects: subjects(904),
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
            },
        ],
        1,
    )
    .unwrap();
    let returned = checked(vec![instruction(
        1,
        Action::Move {
            objects: Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            movement: MoveSpec {
                to: MoveDestination::Shared {
                    zone: ZoneRef {
                        player: PlayerRef::Controller {},
                        kind: ZoneKind::Field,
                    },
                },
                initial_posture: None,
                entry_effects: vec![],
                position: None,
                order: None,
            },
            reason: MoveReason::Put,
        },
    )]);
    game.execute_internal(
        &returned,
        &source("synthetic"),
        Context {
            source: "f".into(),
            generation: Generation(1),
            controller: PlayerId { seat: 2 },
        },
    )
    .unwrap();
    run(
        &mut game,
        vec![Action::Posture {
            objects: subjects(905),
            posture: Posture::Acted,
        }],
        1,
    )
    .unwrap();
    end(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["f"]["acted"],
        false
    );
}
#[test]
fn no_stand_rejects_other_permission_lifetimes_without_mutation() {
    for wrong in [0, 1, 2, 3] {
        let mut action = lock();
        let Action::InstallEffect {
            duration, effect, ..
        } = &mut action
        else {
            panic!("synthetic permission shape");
        };
        let Effect::Permission {
            duration: inner,
            action: pattern,
            condition,
            ..
        } = effect.as_mut()
        else {
            panic!("synthetic permission shape");
        };
        match wrong {
            0 => {
                duration.end = DurationEnd::CurrentTurnEnd {};
                *inner = duration.clone();
            }
            1 => {
                let ActionPattern::Stand { scope, .. } = pattern else {
                    panic!("synthetic permission shape")
                };
                *scope = StandScope::Any {};
            }
            2 => *condition = Expr::Literal(Bool(false)),
            _ => {
                duration.end = DurationEnd::NextPhaseEdge {
                    player: PlayerRef::Controller {},
                    phase: Phase::Start,
                    edge: Edge::End,
                };
                *inner = duration.clone();
            }
        }
        let checked = WellFormedProgram::check(body(1, vec![instruction(1, action)]));
        if let Ok(checked) = checked {
            let mut game = board(1);
            let before = exact_authority(&game);
            game.execute_internal(&checked, &source("synthetic"), context(1))
                .unwrap_err();
            assert_eq!(exact_authority(&game), before);
        }
    }
}

#[test]
fn no_stand_waits_for_the_next_actual_start_despite_extra_and_skipped_turns() {
    let mut game = scheduled_board(
        1,
        json!({"extra":[{"player":"P1","count":1}],"skipped":[{"player":"P2","count":1}]}),
    );
    run(&mut game, vec![lock()], 1).unwrap();
    for _ in 0..2 {
        end(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["turn"]["active"], "P1");
        assert_eq!(view["objects"]["f"]["acted"], true);
    }
    end(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P2"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["f"]["acted"],
        true
    );
    end(&mut game);
    end(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["f"]["acted"],
        false
    );
}
