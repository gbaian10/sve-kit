#![expect(
    clippy::panic,
    reason = "Unexpected public decisions invalidate these scenarios."
)]
use super::*;

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "attack-restrictions",
    )
    .unwrap()
}

fn position(number: &str, foes: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
    initial["players"]["P2"]["zones"]["field"] = json!(
        (0..foes)
            .map(|i| json!({"id":format!("enemy-{i}"),"card":"BP01-037","state":{"acted":true}}))
            .collect::<Vec<_>>()
    );
    initial
}

fn attack(game: &mut Game, attacker: &str, target: &str) -> String {
    game.internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":target}))
        .unwrap()
        .outcome
}

#[test]
fn bp01_088_cannot_attack_either_target_and_loses_the_restriction_when_silenced() {
    for silenced in [false, true] {
        for target in ["P2.leader", "enemy-0"] {
            let mut initial = position("BP01-088", 1);
            initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"silenced":silenced});
            let mut game = game(&initial);
            assert_eq!(
                attack(&mut game, "source", target),
                if silenced {
                    "resolved"
                } else {
                    "cannot-attack"
                },
                "the printed attack prohibition ends when the ability is lost"
            );
        }
    }
}

#[test]
fn bp02_107_all_printings_count_opposing_followers_and_only_lock_leader_attacks() {
    for number in ["BP02-107", "BP02-SL20", "PR-049"] {
        for count in [1, 2] {
            for target in ["P2.leader", "enemy-0"] {
                let mut game = game(&position(number, count));
                assert_eq!(
                    attack(&mut game, "source", target),
                    if count == 2 && target == "P2.leader" {
                        "cannot-attack"
                    } else {
                        "resolved"
                    },
                    "two opposing followers forbid only the leader attack"
                );
            }
        }
    }
}

fn pass_quick(game: &mut Game) {
    for _ in 0_u8..4 {
        let Some(q) = game.internal_quick_choice().unwrap() else {
            return;
        };
        game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
    }
    panic!("Quick did not finish");
}

fn finish_turn(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the current main phase can end"
    );
    pass_quick(game);
}

fn enter_source(game: &mut Game) {
    let CardPlay::Entered(entry) = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("the complete real follower enters")
    };
    if matches!(entry.resume, ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn shiori(number: &str) -> Game {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"ally","card":"CP04-005","state":{"acted":true}}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"p1-next","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"p2-next","card":"BP01-037"}]);
    let mut game = game(&initial);
    enter_source(&mut game);
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("fanfare")
    };
    let CardPlay::Awaiting(q) = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap()
    else {
        panic!("target")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("target domain")
    };
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    game
}

#[test]
fn cp04_005_all_printings_stand_the_target_but_forbid_attacks_until_turn_end() {
    for number in ["CP04-005", "CP04-P01"] {
        let mut game = shiori(number);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["ally"]["acted"],
            false,
            "the selected follower stands but its attack prohibition lasts until turn end"
        );
        assert_eq!(
            attack(&mut game, "ally", "P2.leader"),
            "cannot-attack",
            "the selected follower stands but its attack prohibition lasts until turn end"
        );
        finish_turn(&mut game);
        finish_turn(&mut game);
        assert_eq!(
            attack(&mut game, "ally", "P2.leader"),
            "resolved",
            "the selected follower stands but its attack prohibition lasts until turn end"
        );
    }
}

#[test]
fn conditional_leader_restriction_recomputes_after_opposing_followers_leave() {
    let mut game = with_producer(
        &position("BP02-107", 2),
        Action::Move {
            objects: Selector::Zone {
                players: PlayerSet {
                    relation: PlayerRelation::Opponents,
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(0),
                        maximum: None,
                    },
                    text_quantifier: TextQuantifier::All,
                },
                zone: ZoneKind::Field,
                view: SelectionView::Current {},
                binding: Binding::new(80, ScopeId(1)),
                predicate: Box::new(Expr::Literal(Bool(true))),
            },
            movement: MoveSpec {
                to: MoveDestination::Owners {
                    zone: OwnerZone::Hand,
                },
                initial_posture: None,
                entry_effects: vec![],
                position: None,
                order: None,
            },
            reason: MoveReason::Put,
        },
    );
    assert_eq!(
        attack(&mut game, "source", "P2.leader"),
        "cannot-attack",
        "the live opposing follower count controls the leader prohibition"
    );
    resolved(
        game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    assert_eq!(
        attack(&mut game, "source", "P2.leader"),
        "resolved",
        "the live opposing follower count controls the leader prohibition"
    );
}

#[test]
fn installed_attack_permissions_reject_disconnected_turn_intervals() {
    let mut game = Game::from_selected_effects(
        producer_selection().clone(),
        &draw_position(),
        &Value::Null,
        &Value::Null,
        "attack-rejection",
    )
    .unwrap();
    let affected = Binding::new(1, ScopeId(2));
    let captured = Selector::Bound {
        reference: affected,
    };
    let duration = Duration {
        end: DurationEnd::CurrentTurnEnd {},
        anchor: TimeAnchor::Installation {},
        usage: None,
        intervals: vec![Interval::DuringNextTurn {
            player: PlayerRef::Controller {},
        }],
    };
    let action = Action::InstallEffect {
        subjects: InstallationSubjects::Objects {
            objects: Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            affected,
        },
        capture: InstallationTime::InstructionStart,
        effect: Box::new(Effect::Permission {
            subject: PermissionSubject::Objects {
                objects: captured.clone(),
            },
            action: ActionPattern::Attack {
                attacker: captured,
                target: None,
            },
            polarity: Polarity::Forbid,
            condition: Expr::Literal(Bool(true)),
            duration: duration.clone(),
            rule: RuleId(133),
        }),
        duration,
    };
    assert!(
        matches!(
            game.load_internal_card("producer", &producer(action)),
            Err(EngineFailure::Unsupported(_))
        ),
        "a disconnected interval cannot silently disable an accepted permission"
    );
}

#[test]
fn bahamut_fanfare_destroys_every_other_follower_but_keeps_amulets_and_itself() {
    for number in ["BP02-107", "BP02-SL20", "PR-049"] {
        let mut initial = position(number, 2);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"ally","card":"BP01-037"},{"id":"amulet","card":"BP13-087"}
        ]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
        let mut game = game(&initial);
        enter_source(&mut game);
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("printed fanfare")
        };
        assert_eq!(
            options[0].line, 3,
            "the third printed line destroys other followers"
        );
        resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["field"],
            json!(["amulet", "source"]),
            "the provider and amulet remain on field"
        );
        assert_eq!(
            view["P2"]["field"],
            json!([]),
            "all opposing followers leave together"
        );
        assert_eq!(
            view["P1"]["cemetery"],
            json!(["ally"]),
            "the allied follower is also destroyed"
        );
        assert_eq!(
            view["P2"]["cemetery"],
            json!(["enemy-0", "enemy-1"]),
            "both opposing followers reach their owner's cemetery"
        );
    }
}
