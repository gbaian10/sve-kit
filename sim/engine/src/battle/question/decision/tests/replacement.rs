use super::super::*;
use super::select::{board, components, domain, program, project_question, question};
use crate::battle::tests::{board_fixture, board_fixtures};
use crate::game::player::{Input, SubmissionKind};
use crate::game::{Game, View};
use serde_json::json;
use sve_battle_contract::{OptionId, QuestionId};

#[test]
fn damage_application_order_uses_captured_sources_and_changes_real_arithmetic() {
    let program = program(json!({"cost":{"pp":0_i32},"do":[
        {"apply":{"on":"this","effect":{"damage_mod":{"delta":2_i32}},"until":"turn"}},
        {"apply":{"on":"this","effect":{"damage_cap":{"threshold":2_i32,"set":1_i32}},"until":"turn"}},
        {"damage":{"to":"this","n":3_i32}}
    ]}));
    for actor in [Viewer::P1, Viewer::P2] {
        for reverse in [false, true] {
            let mut game = board(actor, &program);
            let native = question(&game, actor).unwrap();
            assert!(
                matches!(domain(&native),Some(DecisionShape::DamageReplacements(choices)) if choices.len()==2 && choices.iter().all(|choice| choice.source.is_some()))
            );
            let before = board_fixtures::exact_authority(&game);
            let id = QuestionId::new("replacement".into());
            let (usage, shape, binding) =
                project_question(&native, &components(&game, actor), &id).unwrap();
            assert_eq!(usage, wire::QuestionUse::DamageReplacementOrder);
            let wire::QuestionShape::Order {
                items,
                direction,
                destination,
            } = shape
            else {
                panic!("application order")
            };
            assert_eq!(direction, wire::OrderDirection::Application);
            assert!(destination.is_none());
            assert!(
                items
                    .iter()
                    .all(|item| item.source.is_some() && item.subject.is_none())
            );
            let mut order = items.iter().map(|item| item.id.clone()).collect::<Vec<_>>();
            if reverse {
                order.reverse();
            }
            reject_invalid_orders(&binding, &order);
            let answer = Answer::Order { order };
            let expected = if reverse { vec![1, 0] } else { vec![0, 1] };
            assert_eq!(binding.decode(&answer), Ok(player::Answer::Order(expected)));
            let other = if actor == Viewer::P1 {
                Viewer::P2
            } else {
                Viewer::P1
            };
            assert!(matches!(game.input(other), Input::Waiting(_)));
            let references = &board_fixture::fixture().unwrap().references;
            assert!(
                game.battle_question(other, &id, references)
                    .unwrap()
                    .is_none()
            );
            assert!(
                board_fixtures::exact_authority(&game) == before,
                "projection and rejected answers preserve authority"
            );
            let command = binding.answer(native.token, &answer).unwrap();
            assert_eq!(
                game.submit_player(other, command.clone()).kind,
                SubmissionKind::Rejected
            );
            assert!(
                board_fixtures::exact_authority(&game) == before,
                "projection and rejected answers preserve authority"
            );
            assert_eq!(
                game.submit_player(actor, command.clone()).kind,
                SubmissionKind::Applied
            );
            assert_eq!(
                game.submit_player(actor, command).kind,
                SubmissionKind::Rejected
            );
            assert_actual_damage(&game, actor, reverse);
        }
    }
}

mod privacy;

#[test]
fn static_and_granted_replacements_capture_the_definition_instead_of_its_label() {
    use crate::battle::tests::payment_fixtures;
    use crate::game::player::observation::TextProvenance;
    use crate::ir::lowering::ExpandedLowerer;
    use crate::ir::payment_expanded_tree;
    for granted in [false, true] {
        let head = json!({"static":{"effects":[{"damage_mod":{"delta":2_i32}}]}});
        let mut body = vec![];
        if granted {
            body.push(json!({"grant":{"on":"this","abilities":[head.clone()]}}));
        }
        body.extend([
            json!({"apply":{"on":"this","effect":{"damage_cap":{"threshold":2_i32,"set":1_i32}},"until":"turn"}}),
            json!({"damage":{"to":"this","n":3_i32}}),
        ]);
        let mut tree = payment_fixtures::x_document(
            payment_expanded_tree(),
            "activate",
            json!({"cost":{"pp":0_i32},"do":body}),
        )
        .unwrap();
        if !granted {
            let mut entry = tree["cards"]["unit-follower"]["abilities"][0].clone();
            entry.as_object_mut().unwrap().remove("activate");
            entry["line"] = json!(5_i32);
            entry["static"] = head["static"].clone();
            tree["cards"]["unit-follower"]["abilities"]
                .as_array_mut()
                .unwrap()
                .push(entry);
        }
        let program = ExpandedLowerer::new()
            .unwrap()
            .lower(&tree)
            .unwrap()
            .remove("unit-follower")
            .unwrap();
        for actor in [Viewer::P1, Viewer::P2] {
            let game = board(actor, &program);
            let native = question(&game, actor).unwrap();
            let DecisionShape::DamageReplacements(choices) = domain(&native).unwrap() else {
                panic!("replacement order")
            };
            assert_eq!(choices.len(), 2);
            assert!(choices.iter().all(|choice| choice.source.is_some()));
            if granted {
                assert!(choices.iter().any(|choice| matches!(
                    choice.source.as_ref().unwrap().provenance,
                    TextProvenance::Granted { .. }
                )));
            } else {
                assert!(
                    choices
                        .iter()
                        .any(|choice| choice.source.as_ref().unwrap().line == Some(5))
                );
            }
            project_question(
                &native,
                &components(&game, actor),
                &QuestionId::new("static-origin".into()),
            )
            .unwrap();
        }
    }
}

#[test]
fn replacement_projection_refuses_missing_provenance_duplicate_handles_and_wrong_seats() {
    let program = program(json!({"cost":{"pp":0_i32},"do":[
        {"apply":{"on":"this","effect":{"damage_mod":{"delta":2_i32}},"until":"turn"}},
        {"apply":{"on":"this","effect":{"damage_cap":{"threshold":2_i32,"set":1_i32}},"until":"turn"}},
        {"damage":{"to":"this","n":3_i32}}
    ]}));
    for actor in [Viewer::P1, Viewer::P2] {
        let game = board(actor, &program);
        let before = board_fixtures::exact_authority(&game);
        let native = question(&game, actor).unwrap();
        let id = QuestionId::new("replacement-evidence".into());
        let other = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        assert!(project_question(&native, &components(&game, other), &id).is_err());
        for missing_source in [false, true] {
            let mut invalid = native.clone();
            let player::QuestionShape::Decision(DecisionShape::DamageReplacements(choices)) =
                &mut invalid.shape
            else {
                panic!("replacement order")
            };
            if missing_source {
                choices[0].source = None;
            } else {
                choices[1].handle = choices[0].handle;
            }
            assert!(project_question(&invalid, &components(&game, actor), &id).is_err());
        }
        assert!(
            board_fixtures::exact_authority(&game) == before,
            "projection and rejected answers preserve authority"
        );
    }
}

#[test]
fn native_suppresses_replacement_questions_when_the_order_cannot_matter() {
    for deltas in [vec![], vec![1_i32], vec![1_i32, 2_i32]] {
        let mut steps=deltas.into_iter().map(|delta|json!({"apply":{"on":"this","effect":{"damage_mod":{"delta":delta}},"until":"turn"}})).collect::<Vec<_>>();
        steps.push(json!({"damage":{"to":"this","n":1_i32}}));
        let program = program(json!({"cost":{"pp":0_i32},"do":steps}));
        for actor in [Viewer::P1, Viewer::P2] {
            let game = board(actor, &program);
            assert!(!matches!(
                question(&game, actor).as_ref().and_then(domain),
                Some(DecisionShape::DamageReplacements(_))
            ));
        }
    }
}

fn reject_invalid_orders(binding: &Binding, order: &[OptionId]) {
    for invalid in [
        Answer::Confirm { accept: true },
        Answer::Order { order: vec![] },
        Answer::Order {
            order: vec![order[0].clone(); 2],
        },
        Answer::Order {
            order: vec![order[0].clone(), OptionId::new("foreign".into())],
        },
    ] {
        assert_eq!(binding.decode(&invalid), Err(AnswerError));
    }
}

fn assert_actual_damage(game: &Game, actor: Viewer, reverse: bool) {
    let hp = game.projection(View::Referee).unwrap()["objects"]["source"]["hp"].clone();
    assert_eq!(hp, if reverse { json!(3_i32) } else { json!(2_i32) });
    assert!(
        game.projection(View::Referee).unwrap()[super::select::seats(actor).0]
            [if reverse { "cemetery" } else { "field" }]
        .as_array()
        .unwrap()
        .contains(&json!("source"))
    );
}
