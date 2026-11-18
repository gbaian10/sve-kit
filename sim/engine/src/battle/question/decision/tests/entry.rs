use super::super::*;
use super::select::{
    components, domain, project_game, project_question, question, seats, submit_projected,
};
use crate::battle::tests::{board_fixture, board_fixtures, history_fixture, payment_fixtures};
use crate::game::player::{Input, SubmissionKind};
use crate::game::{Game, View};
use crate::ir::lowering::ExpandedLowerer;
use crate::ir::{self, WellFormedCardProgram, payment_expanded_tree};
use alloc::sync::Arc;
use serde_json::{Value, json};
use sve_battle_contract::{QuestionId, Subject, Zone};

fn draw_program(count: i64) -> ir::WellFormedProgram {
    ir::WellFormedProgram::check(history_fixture::body(
        1,
        vec![history_fixture::instruction(
            700,
            ir::Action::Draw {
                player: ir::PlayerRef::Controller {},
                count: ir::Expr::Literal(ir::Int(count)),
                mode: ir::StepwiseMode::UpToStepwise {
                    decision: Box::new(ir::StepwiseDecision {
                        id: ir::DecisionId(700),
                        actor: ir::PlayerRef::Controller {},
                        visibility: ir::Visibility::Only {
                            players: ir::PlayerSet {
                                relation: ir::PlayerRelation::SelfPlayer,
                                relative_to: ir::PlayerRef::Controller {},
                                text_quantifier: ir::TextQuantifier::All,
                                cardinality: ir::Cardinality {
                                    minimum: ir::Nat(1),
                                    maximum: Some(ir::Nat(1)),
                                },
                            },
                        },
                        timing: ir::StepwiseTiming::BeforeDraw,
                    }),
                },
            },
        )],
    ))
    .unwrap()
}

fn ward_program() -> WellFormedCardProgram {
    let mut document = payment_fixtures::x_document(payment_expanded_tree(), "activate", json!({
        "cost":{"pp":0_i32},
        "do":[{"choose":{"bind":"picked","from":{"who":"self","zone":"hand","kind":"follower"},"n":1_i32}},
            {"move":{"ref":"picked","to":"field","result":"entered"}}]
    })).unwrap();
    let mut aura = document["cards"]["unit-follower"]["abilities"][0].clone();
    aura.as_object_mut().unwrap().remove("activate");
    aura["static"] = json!({"effects":[{"grant":{"on":{"who":"self","zone":"field","kind":"follower"},"keywords":["ward"]}}]});
    aura["line"] = json!(2_u32);
    document["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(aura);
    ExpandedLowerer::new()
        .unwrap()
        .lower(&document)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn board(actor: Viewer, ward: bool, first: bool, secret: &str) -> Game {
    let program = if ward {
        ward_program()
    } else {
        WellFormedCardProgram::check(ir::CardProgram {
            abilities: vec![],
            card_play_costs: vec![],
        })
        .unwrap()
    };
    let mut initial = board_fixtures::setup(actor.player().seat, json!([]));
    let (own, other) = seats(actor);
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["pp"] = json!({"current":3_i32,"max":3_i32});
    let hand = if first {
        json!([{"id":"a","card":"unit-follower"},{"id":"b","card":"unit-spell"}])
    } else {
        json!([{"id":"a","card":"unit-spell"},{"id":"b","card":"unit-follower"}])
    };
    initial["players"][own]["zones"] = json!({
        "field":[{"id":"source","card":"unit-follower"}],"hand":hand,
        "deck":[{"id":"d1","card":"unit-spell"},{"id":"d2","card":"unit-amulet"}]
    });
    initial["players"][other]["zones"] = json!({"hand":[{"id":"secret","card":secret}]});
    let mut game = Game::new(
        Arc::clone(&board_fixture::fixture().unwrap().catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "entry",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(
            number,
            &WellFormedCardProgram::check(ir::CardProgram {
                abilities: vec![],
                card_play_costs: vec![],
            })
            .unwrap(),
        )
        .unwrap();
    }
    if ward {
        game.activate_internal_card(
            "source",
            program.card().abilities.first().unwrap().node.id,
            actor.player(),
        )
        .unwrap();
    } else {
        let mut context = history_fixture::context(actor.player().seat);
        context.source = "source".into();
        game.execute_internal(
            &draw_program(2),
            &history_fixture::source("unit-follower"),
            context,
        )
        .unwrap();
    }
    game.advance_player_input();
    if ward {
        let native = question(&game, actor).unwrap();
        let selection = if let DecisionShape::Targets(selection) = domain(&native).unwrap() {
            Some(selection)
        } else {
            None
        }
        .unwrap();
        assert_eq!(selection.members.len(), 1);
        assert_eq!(
            game.submit_player(
                actor,
                player::Command::Answer {
                    token: native.token,
                    answer: player::Answer::Targets(vec![
                        selection.members.first().unwrap().handle
                    ])
                }
            )
            .kind,
            SubmissionKind::Applied
        );
        assert!(matches!(
            domain(&question(&game, actor).unwrap()),
            Some(DecisionShape::WardEntry { .. })
        ));
    }
    game
}

#[test]
fn before_draw_chooses_typed_steps_and_keeps_each_native_stop() {
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = board(actor, false, true, "unit-spell");
        let id = QuestionId::new("draw".into());
        for step in [wire::StepwiseChoice::Continue, wire::StepwiseChoice::Stop] {
            let native = question(&game, actor).unwrap();
            assert_eq!(domain(&native), Some(&DecisionShape::BeforeDraw));
            let before = board_fixtures::exact_authority(&game);
            let (usage, shape, binding) = project_game(&game, actor, &id).unwrap();
            assert_eq!(usage, wire::QuestionUse::DrawContinue);
            let (options, bounds, count, earlier, capacity) = if let wire::QuestionShape::Choose {
                options,
                bounds,
                count,
                earlier,
                capacity,
            } = shape
            {
                Some((options, bounds, count, earlier, capacity))
            } else {
                None
            }
            .unwrap();
            assert_eq!(bounds.min.get(), 1);
            assert_eq!(bounds.max.unwrap().get(), 1);
            assert!(count.is_none() && earlier.is_empty() && capacity.is_none());
            assert_eq!(options.len(), 2);
            let selected = options
                .iter()
                .find(|o| o.value == wire::ChoiceValue::Stepwise { step })
                .unwrap()
                .id
                .clone();
            assert_eq!(
                binding.answer(
                    native.actor,
                    &binding.question().id,
                    &Answer::Confirm { accept: true }
                ),
                Err(AnswerError)
            );
            for invalid in [
                vec![],
                vec![selected.clone(), selected.clone()],
                vec![sve_battle_contract::OptionId::new("foreign".into())],
            ] {
                assert_eq!(
                    binding.answer(
                        native.actor,
                        &binding.question().id,
                        &Answer::Choose { chosen: invalid }
                    ),
                    Err(AnswerError)
                );
            }
            assert!(
                board_fixtures::exact_authority(&game) == before,
                "reads preserve authority"
            );
            submit_projected(
                &mut game,
                &native,
                &binding,
                &Answer::Choose {
                    chosen: vec![selected],
                },
            );
        }
        assert!(
            !question(&game, actor).is_some_and(|q| domain(&q) == Some(&DecisionShape::BeforeDraw))
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()[seats(actor).0]["hand"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
    }
}

#[test]
fn ward_confirm_preserves_private_card_origin_and_posture_channel() {
    for actor in [Viewer::P1, Viewer::P2] {
        for accept in [false, true] {
            let mut game = board(actor, true, false, "unit-spell");
            let native = question(&game, actor).unwrap();
            let (usage, shape, binding) =
                project_game(&game, actor, &QuestionId::new("ward".into())).unwrap();
            assert_eq!(usage, wire::QuestionUse::WardEntry);
            let (allowed, subject, origin) = if let wire::QuestionShape::Confirm {
                allowed,
                subject,
                origin,
            } = shape
            {
                Some((allowed, subject, origin))
            } else {
                None
            }
            .unwrap();
            assert_eq!(allowed, vec![false, true]);
            assert!(
                matches!(subject, Some(Subject::Card { card, object: Some(_) }) if card.printing_id == "printing-follower" && card.face.get() == 1)
            );
            let origin = origin.unwrap();
            assert_eq!(origin.zone, Zone::Hand);
            assert_eq!(origin.position.unwrap().get(), 1);
            assert_eq!(
                binding.answer(
                    native.actor,
                    &binding.question().id,
                    &Answer::Integer {
                        value: sve_battle_contract::Nat::from(1_u32)
                    }
                ),
                Err(AnswerError)
            );
            submit_projected(&mut game, &native, &binding, &Answer::Confirm { accept });
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["b"]["acted"],
                accept
            );
        }
    }
}

mod privacy;

#[test]
fn a_new_ward_token_uses_its_frozen_face_without_inventing_a_board_object() {
    let actor = Viewer::P1;
    let game = board(actor, true, true, "unit-spell");
    let mut native = question(&game, actor).unwrap();
    if let player::QuestionShape::Decision(DecisionShape::WardEntry {
        zone, object, face, ..
    }) = &mut native.shape
    {
        *zone = "void".into();
        *object = None;
        face.face = 1;
    }
    let (_, shape, binding) = project_question(
        &native,
        &components(&game, actor),
        &QuestionId::new("new-token".into()),
    )
    .unwrap();
    assert!(
        matches!(shape, wire::QuestionShape::Confirm { subject: Some(Subject::Card { card, object: None }), origin: None, .. }
        if card.printing_id == "printing-follower" && card.face.get() == 0)
    );
    assert_eq!(
        binding.decode(&Answer::Confirm { accept: true }),
        Ok(player::Answer::WardEntry(true))
    );
}

#[test]
fn zero_stepwise_draws_skip_the_native_question() {
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = board_fixture::empty_board(actor, "unit-spell").unwrap();
        let result = game
            .execute_internal(
                &draw_program(0),
                &history_fixture::source("unit-follower"),
                history_fixture::context(actor.player().seat),
            )
            .unwrap();
        assert!(result.complete);
        assert!(
            !question(&game, actor).is_some_and(|q| domain(&q) == Some(&DecisionShape::BeforeDraw))
        );
    }
}

#[test]
fn ward_refuses_missing_mismatched_and_wrong_viewer_source_facts() {
    let actor = Viewer::P1;
    let game = board(actor, true, true, "unit-spell");
    let native = question(&game, actor).unwrap();
    let id = QuestionId::new("source-facts".into());
    let observation = components(&game, actor);
    for fault in 0_u8..5 {
        let mut broken = native.clone();
        if let player::QuestionShape::Decision(DecisionShape::WardEntry {
            object,
            position,
            zone,
            face,
            ..
        }) = &mut broken.shape
        {
            match fault {
                0 => *object = None,
                1 => *position = Some(0),
                2 => *zone = "unknown".into(),
                3 => face.face = 1,
                _ => face.card = "unit-spell".into(),
            }
        }
        assert!(matches!(
            project_question(&broken, &observation, &id),
            Err(ProjectionError::Unavailable)
        ));
    }
    assert!(matches!(
        project_question(&native, &components(&game, Viewer::P2), &id),
        Err(ProjectionError::Unavailable)
    ));
}
