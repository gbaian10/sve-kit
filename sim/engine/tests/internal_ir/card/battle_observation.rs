use super::battle_pending::{board, board_with_program, fixture};
use super::*;
use sve_battle_contract::{MAX_SAFE_INTEGER, Seat, Subject, observation as wire};
use sve_engine::battle::ProjectionError;
use sve_engine::game::internal::InputDomain;
use sve_engine::game::player::events::{ActionDetails, ActionEvent, EventKind};
use sve_engine::game::player::{
    Command, DecisionShape, Input, OpeningAnswer, QuestionShape, SubmissionKind, Viewer,
};
use sve_engine::ir::DamageFlags;

fn read(game: &Game, viewer: Viewer) -> wire::BattleObservation {
    let before = format!("{game:?}");
    let result = game
        .battle_observation(viewer, &fixture().references)
        .unwrap();
    assert!(
        format!("{game:?}") == before,
        "a board read must not change authority"
    );
    result
}

fn make(initial: &Value) -> Game {
    make_with_program(initial, &card(vec![]))
}

fn make_with_program(initial: &Value, follower: &WellFormedCardProgram) -> Game {
    let mut game = Game::new(
        Arc::clone(&fixture().catalog),
        initial,
        &Value::Null,
        &Value::Null,
        "wire-board",
    )
    .unwrap();
    game.load_internal_card("unit-follower", follower).unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

fn attack_position(actor: Viewer, follower: &WellFormedCardProgram) -> Game {
    let (own, other) = if actor == Viewer::P1 {
        ("P1", "P2")
    } else {
        ("P2", "P1")
    };
    let mut initial = setup(actor.player().seat, json!([]));
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["zones"]["hand"] = json!([]);
    initial["players"][own]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower","state":{"entered_this_turn":false}}]);
    // A standing ordinary follower is not a legal attack target.
    initial["players"][other]["zones"]["field"][0]["state"] = json!({"acted":true});
    initial["players"][other]["zones"]["hand"] = json!([{"id":"secret","card":"unit-amulet"}]);
    make_with_program(&initial, follower)
}

fn answering(game: &Game) -> Option<Viewer> {
    [Viewer::P1, Viewer::P2]
        .into_iter()
        .find(|viewer| matches!(game.input(*viewer), Input::Question(_)))
}

fn assert_public_keys(value: &Value) {
    if let Some(object) = value.as_object() {
        for (key, value) in object {
            assert!(
                ![
                    "generation",
                    "serial",
                    "frame",
                    "continuation",
                    "capture",
                    "receipt",
                    "ir_path"
                ]
                .contains(&key.as_str()),
                "only contract fields belong in the board"
            );
            assert_public_keys(value);
        }
    }
    if let Some(array) = value.as_array() {
        for child in array {
            assert_public_keys(child);
        }
    }
}

#[test]
fn wire_board_combines_one_view_without_opponent_secrets_or_placeholder_handles() {
    for viewer in [Viewer::P1, Viewer::P2] {
        let first = board(viewer, "unit-spell");
        let second = board(viewer, "unit-amulet");
        let projected = read(&first, viewer);
        assert_eq!(
            projected,
            read(&second, viewer),
            "the opponent's secret cannot affect this board"
        );
        assert_eq!(
            projected.objects,
            first.battle_objects(viewer, &fixture().references).unwrap(),
            "the table shares the same authority facts"
        );
        assert_eq!(
            projected.turn,
            first.battle_turn(viewer).unwrap(),
            "the turn uses the same converter"
        );
        assert_eq!(
            projected.objects.len(),
            2,
            "only the two field cards are identifiable"
        );
        assert_eq!(
            projected.processing,
            [],
            "the main action window has no active resolution"
        );
        let opponent = usize::from(viewer == Viewer::P1);
        assert_eq!(
            projected.players[opponent].zones.hand.count.get(),
            1,
            "anonymous hand cards still count"
        );
        assert_eq!(
            projected.players[opponent].zones.hand.cards,
            [],
            "an unknown hand has no slots or handles"
        );
        for player in &projected.players {
            let zones = serde_json::to_value(&player.zones).unwrap();
            assert_eq!(
                zones.as_object().unwrap().len(),
                13,
                "all contract zones exist"
            );
            assert_eq!(
                player.zones.equipment.count.get(),
                0,
                "an absent area is empty"
            );
            assert_eq!(
                player.zones.equipment.cards,
                [],
                "an absent area has no fabricated members"
            );
            for zone in zones.as_object().unwrap().values() {
                for member in zone["cards"].as_array().unwrap() {
                    let handle = member["object"].as_str().unwrap();
                    assert!(
                        projected.objects.keys().any(|key| key.as_str() == handle),
                        "every zone member belongs to this object table"
                    );
                }
            }
        }
        assert_public_keys(&serde_json::to_value(projected).unwrap());
    }
}

fn assert_card_damage(view: &wire::BattleObservation, actor: Viewer, leader: bool) -> Option<()> {
    if leader {
        assert_eq!(
            view.history.damage.len(),
            0,
            "this ledger records damaged cards, not leaders"
        );
        return Some(());
    }
    assert_eq!(
        view.history.damage.len(),
        2,
        "both directions of combat keep their card damage"
    );
    let actor_index = usize::from(actor != Viewer::P1);
    let other_index = usize::from(actor == Viewer::P1);
    for (source, target, controller) in [
        (
            actor_index,
            other_index,
            if actor == Viewer::P1 {
                Seat::P1
            } else {
                Seat::P2
            },
        ),
        (
            other_index,
            actor_index,
            if actor == Viewer::P1 {
                Seat::P2
            } else {
                Seat::P1
            },
        ),
    ] {
        let source = &view.players[source].zones.field.cards[0].object;
        let target = &view.players[target].zones.field.cards[0].object;
        let fact = view
            .history
            .damage
            .iter()
            .find(|fact| fact.source.as_ref() == Some(source))?;
        assert_eq!(
            fact.recipient.as_ref(),
            Some(target),
            "the damaged card shares this scope"
        );
        assert_eq!(
            fact.controller, controller,
            "the controller is frozen at damage time"
        );
    }
    Some(())
}

fn damage_occurrences(game: &Game, viewer: Viewer) -> Vec<(DamageFlags, Option<Viewer>)> {
    game.events(viewer, None)
        .unwrap()
        .filter_map(|event| match &event.kind {
            EventKind::Action(ActionEvent {
                details: Some(ActionDetails::Damage(flags)),
                player,
                ..
            }) => Some((*flags, *player)),
            EventKind::Movement { .. }
            | EventKind::Action(_)
            | EventKind::Phase(_)
            | EventKind::Result { .. } => None,
        })
        .collect()
}

#[test]
fn wire_board_uses_real_quick_and_frozen_card_damage_after_the_arrow_is_gone() {
    for actor in [Viewer::P1, Viewer::P2] {
        for leader in [false, true] {
            let other = if actor == Viewer::P1 { "P2" } else { "P1" };
            let target = if leader {
                format!("{other}.leader")
            } else {
                "f".into()
            };
            let mut game = attack_position(actor, &card(vec![]));
            let index = usize::from(actor != Viewer::P1);
            let attacker = read(&game, actor).players[index].zones.field.cards[0]
                .object
                .clone();
            assert_eq!(
                game.internal_rule_action(&json!({"do":"attack","attacker":"s","target":target}))
                    .unwrap()
                    .outcome,
                "resolved",
                "the declaration must be a real legal attack"
            );
            game.advance_player_input();
            let during = read(&game, actor);
            assert_eq!(during.processing[0].stage, wire::ProcessingStage::Quick);
            let arrow = during.attack.unwrap();
            let Subject::Card {
                object: Some(current),
                card,
            } = arrow.attacker
            else {
                panic!("an attack comes from a follower")
            };
            assert_eq!(current, attacker);
            assert_eq!(card.printing_id, "printing-follower");
            assert_eq!(
                card.face.get(),
                1,
                "the subject uses the adopted effective face"
            );
            assert_eq!(
                matches!(arrow.target, Subject::Leader { .. }),
                leader,
                "the actual declared target is preserved"
            );
            let chooser = answering(&game).expect("the real rule stop has one answering seat");
            let question = player_pipeline::question(&game, chooser);
            assert_eq!(
                game.submit_player(
                    chooser,
                    Command::Answer {
                        token: question.token,
                        answer: Answer::Quick(None)
                    }
                )
                .kind,
                SubmissionKind::Applied
            );
            for viewer in [Viewer::P1, Viewer::P2] {
                let expected = if leader {
                    vec![(DamageFlags::Attack, Some(actor))]
                } else {
                    vec![
                        (DamageFlags::AttackCombat, Some(actor)),
                        (DamageFlags::Combat, Some(chooser)),
                    ]
                };
                assert_eq!(
                    damage_occurrences(&game, viewer),
                    expected,
                    "battle facts freeze the kind and controller even without DSL observers"
                );
                let after = read(&game, viewer);
                assert_eq!(after.attack, None, "a completed attack leaves no arrow");
                assert_eq!(
                    after.processing,
                    [],
                    "the game has returned to its main window"
                );
                assert_card_damage(&after, actor, leader)
                    .expect("both directions retain their frozen combat source");
                if viewer == actor {
                    assert_eq!(
                        after.players[index].zones.field.cards[0].object, attacker,
                        "uninterrupted identification survives combat"
                    );
                }
            }
        }
    }
}

#[test]
fn wire_board_pending_rules_keep_the_follower_target_arrow_and_public_controller() {
    let program = card(vec![timing::on_attack(ability(2, vec![]))]);
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = attack_position(actor, &program);
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"s","target":"f"}))
                .unwrap()
                .outcome,
            "resolved",
            "the trigger must come from a real legal attack"
        );
        game.internal_pending_choice().unwrap().unwrap();
        for viewer in [Viewer::P1, Viewer::P2] {
            let view = read(&game, viewer);
            assert_eq!(
                view.processing[0].stage,
                wire::ProcessingStage::PendingOrder
            );
            assert_eq!(
                view.processing[0].controller,
                Some(if actor == Viewer::P1 {
                    Seat::P1
                } else {
                    Seat::P2
                })
            );
            assert_eq!(
                view.pending.len(),
                1,
                "the actual public occurrence is waiting"
            );
            let arrow = view.attack.unwrap();
            let Subject::Card {
                object: Some(target),
                card,
            } = arrow.target
            else {
                panic!("the actual declared target is a follower")
            };
            let other = usize::from(actor == Viewer::P1);
            assert_eq!(target, view.players[other].zones.field.cards[0].object);
            assert_eq!(
                card, view.objects[&target].effective_face,
                "the target belongs to the same adopted object table"
            );
        }
    }
}

fn quota_position(actor: Viewer) -> Game {
    let programs = lowerer()
        .lower(&evolution_linked_movement::tree(&[(
            "auto",
            json!({
                "per_turn":1_u8,"on":{"left_field":{"who":"self","sel":{"kind":"follower"}}},
                "do":[{"draw":{"who":"self","n":1}}]
            }),
        )]))
        .unwrap();
    let mut raw = programs["unit-follower"].card().clone();
    raw.abilities.push(ability(
        20,
        vec![damage(
            20,
            RecipientSet::Objects {
                objects: zone(ZoneKind::Field, 50),
            },
            99,
        )],
    ));
    let own = if actor == Viewer::P1 { "P1" } else { "P2" };
    let other = if actor == Viewer::P1 { "P2" } else { "P1" };
    let mut initial = setup(actor.player().seat, json!([]));
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["zones"]["hand"] = json!([]);
    initial["players"][own]["zones"]["field"] = json!([
        {"id":"s","card":"unit-follower"},
        {"id":"a","card":"unit-follower"},
        {"id":"b","card":"unit-follower"}
    ]);
    initial["players"][other]["zones"]["field"] = json!([]);
    make_with_program(&initial, &WellFormedCardProgram::check(raw).unwrap())
}

#[test]
fn wire_board_admission_uses_the_current_public_group_and_keeps_budgets_controller_only() {
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = quota_position(actor);
        resolved(
            game.activate_internal_card("s", AbilityId(20), actor.player())
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let question = game.internal_pending_choice().unwrap().unwrap();
        assert!(
            matches!(question.domain, InputDomain::TriggerQuota(_)),
            "the real simultaneous deaths require a quota choice"
        );
        for viewer in [Viewer::P1, Viewer::P2] {
            let view = read(&game, viewer);
            assert_eq!(view.processing.len(), 1);
            assert_eq!(
                view.processing[0].stage,
                wire::ProcessingStage::TriggerAdmission
            );
            assert_eq!(
                view.processing[0].controller,
                Some(if actor == Viewer::P1 {
                    Seat::P1
                } else {
                    Seat::P2
                })
            );
            assert!(
                !view.pending.is_empty(),
                "the current public group is disclosed after its source departs"
            );
            assert!(
                view.pending
                    .iter()
                    .all(|pending| pending.admission_remaining.is_some() == (viewer == actor)),
                "the public occurrence never discloses another controller's remaining budget"
            );
        }
    }
}

#[test]
fn wire_board_has_a_closing_context_without_disclosing_the_other_hand() {
    for actor in [Viewer::P1, Viewer::P2] {
        let own = if actor == Viewer::P1 { "P1" } else { "P2" };
        let mut initial = setup(actor.player().seat, json!([]));
        initial["turn"]["active"] = json!(own);
        initial["players"][own]["zones"]["hand"] = json!(
            (0_usize..8)
                .map(|i| json!({"id":format!("hand-{i}"),"card":"unit-amulet"}))
                .collect::<Vec<_>>()
        );
        let mut game = make(&initial);
        game.advance_player_input();
        let question = player_pipeline::question(&game, actor);
        assert_eq!(
            game.submit_player(actor, player_pipeline::end_command(&question))
                .kind,
            SubmissionKind::Applied
        );
        let chooser = answering(&game).expect("the real rule stop has one answering seat");
        let next = player_pipeline::question(&game, chooser);
        if matches!(
            next.shape,
            QuestionShape::Decision(DecisionShape::Quick { .. })
        ) {
            assert_eq!(
                game.submit_player(
                    chooser,
                    Command::Answer {
                        token: next.token,
                        answer: Answer::Quick(None)
                    }
                )
                .kind,
                SubmissionKind::Applied
            );
        }
        let closing = player_pipeline::question(&game, actor);
        assert!(
            matches!(closing.shape, QuestionShape::Closing(_)),
            "the real hand-limit procedure is waiting"
        );
        for viewer in [Viewer::P1, Viewer::P2] {
            let view = read(&game, viewer);
            assert_eq!(view.processing.len(), 1);
            assert_eq!(view.processing[0].stage, wire::ProcessingStage::Closing);
            let index = usize::from(actor != Viewer::P1);
            assert_eq!(view.players[index].zones.hand.count.get(), 8);
            assert_eq!(
                view.players[index].zones.hand.cards.len(),
                if viewer == actor { 8 } else { 0 },
                "only the actor's board has those private members"
            );
        }
    }
}

fn assert_mulligan_actor(game: &Game, actor: Viewer) -> Option<()> {
    for viewer in [Viewer::P1, Viewer::P2] {
        let view = read(game, viewer);
        let procedure = view.processing[0].procedure?;
        assert_eq!(
            procedure.kind,
            wire::ProcedureKind::EachPlayer,
            "mulligans use the real per-seat rule procedure"
        );
        assert_eq!(
            procedure.actor,
            if actor == Viewer::P1 {
                Seat::P1
            } else {
                Seat::P2
            },
            "the procedure names the current chooser"
        );
        assert_eq!(
            procedure.round, None,
            "the rule procedure has no invented round counter"
        );
        assert_eq!(
            view.processing[0].controller, None,
            "the chooser is not a controller of this rules procedure"
        );
    }
    Some(())
}

#[test]
fn wire_board_opening_context_keeps_the_unestablished_turn_unknown() {
    let mut players = json!({});
    for seat in ["P1", "P2"] {
        players[seat] = json!({"construction":"class","leader":{"class":"ニュートラル"},
            "deck_list":(0_usize..8).map(|i| json!({"id":format!("{seat}-{i}"),"card":"unit-amulet"})).collect::<Vec<_>>(),"evolve_deck_list":[]});
    }
    for first in [Viewer::P1, Viewer::P2] {
        let mut game = make(&json!({"pregame":true,"players":players}));
        for viewer in [Viewer::P1, Viewer::P2] {
            let view = read(&game, viewer);
            assert_eq!(view.turn.active, None);
            assert_eq!(view.turn.first_player, None);
            assert_eq!(view.processing[0].stage, wire::ProcessingStage::Opening);
            assert_eq!(
                view.processing[0].controller, None,
                "the first-choice actor is not an established turn controller"
            );
            assert_eq!(
                view.processing[0].procedure, None,
                "choosing first player is not a fabricated per-seat loop"
            );
        }
        let chooser = answering(&game).expect("the real rule stop has one answering seat");
        let question = player_pipeline::question(&game, chooser);
        assert_eq!(
            game.submit_player(
                chooser,
                Command::Opening {
                    token: question.token,
                    answer: OpeningAnswer::FirstPlayer(first)
                }
            )
            .kind,
            SubmissionKind::Applied
        );
        for viewer in [Viewer::P1, Viewer::P2] {
            let view = read(&game, viewer);
            assert_eq!(view.processing[0].stage, wire::ProcessingStage::Opening);
            assert_eq!(
                view.turn.active,
                Some(if first == Viewer::P1 {
                    Seat::P1
                } else {
                    Seat::P2
                })
            );
            assert_eq!(view.turn.phase, None, "mulligans have not completed");
        }
        assert_mulligan_actor(&game, first)
            .expect("the first mulligan keeps its per-seat procedure");
        let question = player_pipeline::question(&game, first);
        assert_eq!(
            game.submit_player(
                first,
                Command::Opening {
                    token: question.token,
                    answer: OpeningAnswer::Keep
                }
            )
            .kind,
            SubmissionKind::Applied
        );
        let other = if first == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        assert_mulligan_actor(&game, other)
            .expect("the second mulligan keeps its per-seat procedure");
        let view = read(&game, other);
        assert_ne!(
            view.processing[0].procedure.unwrap().actor,
            view.turn.active.unwrap(),
            "the second mulligan actor differs from the established turn seat"
        );
    }
}

#[test]
fn wire_board_refuses_active_typed_processing_instead_of_inventing_an_empty_stack() {
    let mut activation = ability(1, vec![draw(1, 0)]);
    activation
        .node
        .play
        .as_mut()
        .unwrap()
        .parameters
        .push(play::target(10, 1, 1, 1));
    let program = card(vec![activation]);
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = board_with_program(actor, "unit-amulet", &program);
        let CardPlay::Awaiting(_) = game
            .activate_internal_card("s", AbilityId(1), actor.player())
            .unwrap()
        else {
            panic!("an actual declaration is waiting")
        };
        let before = format!("{game:?}");
        for viewer in [Viewer::P1, Viewer::P2] {
            assert_eq!(
                game.battle_observation(viewer, &fixture().references)
                    .unwrap_err(),
                ProjectionError::Unavailable,
                "unsupported required processing must stop the whole projection"
            );
        }
        assert!(
            format!("{game:?}") == before,
            "failed board reads must not mutate authority"
        );
    }
}

#[test]
fn wire_board_refuses_malformed_resources_and_exact_integer_overflow() {
    for path in [
        "/players/P1/leader/life",
        "/players/P1/pp/current",
        "/players/P2/pp/max",
    ] {
        for malformed in [Value::Null, json!(true), json!("2"), json!(1.5_f64)] {
            let mut initial = setup(1, json!([]));
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            *initial.pointer_mut(path).unwrap() = malformed;
            let game = make(&initial);
            for viewer in [Viewer::P1, Viewer::P2] {
                assert!(
                    game.observe(viewer).is_err(),
                    "a malformed public resource is not known zero"
                );
                assert_eq!(
                    game.battle_observation(viewer, &fixture().references)
                        .unwrap_err(),
                    ProjectionError::Unavailable
                );
            }
        }
    }
    for amount in [MAX_SAFE_INTEGER, MAX_SAFE_INTEGER + 1] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P2"]["leader"]["life"] = json!(amount);
        let game = make(&initial);
        for viewer in [Viewer::P1, Viewer::P2] {
            let result = game.battle_observation(viewer, &fixture().references);
            if amount == MAX_SAFE_INTEGER {
                assert_eq!(
                    result.unwrap().players[1].life.get(),
                    i64::try_from(amount).unwrap()
                );
            } else {
                assert_eq!(
                    result.unwrap_err(),
                    ProjectionError::Range,
                    "a known integer cannot be saturated"
                );
            }
        }
    }
}
