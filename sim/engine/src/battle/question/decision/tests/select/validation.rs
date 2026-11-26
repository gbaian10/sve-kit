use super::*;
use crate::ir::Nat;
use sve_battle_contract::{OptionId, RoleChoice, Seat};

fn assert_unchanged(game: &Game, before: &str) {
    assert!(
        board_fixtures::exact_authority(game) == before,
        "invalid selection answers cannot pay or mutate authority"
    );
}

#[test]
fn foreign_duplicate_and_wrong_group_answers_cannot_commit_a_partial_payment() {
    let (body, _, _) = privacy::cases().into_iter().nth(2).unwrap();
    let program = program(body);
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = board_with(
            actor,
            &program,
            &["unit-follower", "unit-follower", "unit-spell"],
            &["unit-spell"],
        );
        let native = question(&game, actor).unwrap();
        let (_, shape, binding) =
            project_game(&game, actor, &QuestionId::new("checked".into())).unwrap();
        let selection = selection(&shape).unwrap();
        let ids = selection
            .candidates
            .iter()
            .map(|candidate| candidate.id.clone())
            .collect::<Vec<_>>();
        let before = board_fixtures::exact_authority(&game);
        for selected in [
            vec![],
            vec![ids[0].clone(), ids[0].clone()],
            vec![ids[0].clone(), OptionId::new("foreign:c0".into())],
        ] {
            assert_eq!(
                binding.answer(
                    native.actor,
                    &binding.question().id,
                    &Answer::Select {
                        selected,
                        groups: None,
                        roles: vec![]
                    }
                ),
                Err(AnswerError)
            );
        }
        assert_eq!(
            binding.answer(
                native.actor,
                &binding.question().id,
                &Answer::Confirm { accept: true }
            ),
            Err(AnswerError)
        );
        assert_unchanged(&game, &before);
        let bad = Answer::Select {
            selected: ids.iter().take(2).cloned().collect(),
            groups: None,
            roles: vec![],
        };
        assert_eq!(
            game.submit_player(
                actor,
                binding
                    .answer(native.actor, &binding.question().id, &bad)
                    .unwrap()
            )
            .kind,
            SubmissionKind::Rejected
        );
        assert_unchanged(&game, &before);
        let good = Answer::Select {
            selected: vec![ids[0].clone(), ids[2].clone()],
            groups: None,
            roles: vec![],
        };
        assert_eq!(
            game.submit_player(
                actor,
                binding
                    .answer(native.actor, &binding.question().id, &good)
                    .unwrap()
            )
            .kind,
            SubmissionKind::Applied
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()[seats(actor).0]["pp"]["current"],
            2_i32
        );
    }
}

#[test]
fn missing_board_facts_context_and_unencodable_constraints_refuse_the_whole_question() {
    let program = program(json!({"cost":{"pp":0_i32},"targets":[
        {"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1_i32}
    ],"do":[{"damage":{"to":"enemy","n":1_i32}}]}));
    let game = board(Viewer::P1, &program);
    let native = question(&game, Viewer::P1).unwrap();
    let observation = components(&game, Viewer::P1);
    let id = QuestionId::new("missing".into());
    assert!(matches!(
        project(domain(&native).unwrap(), None),
        Err(ProjectionError::Unavailable)
    ));
    let mut board_missing_object = observation.clone();
    board_missing_object.objects.clear();
    assert!(matches!(
        project_question(&native, &board_missing_object, &id),
        Err(ProjectionError::Unavailable)
    ));
    let mut wrong_viewer = observation.clone();
    wrong_viewer.viewer = Seat::P2;
    assert!(matches!(
        project_question(&native, &wrong_viewer, &id),
        Err(ProjectionError::Unavailable)
    ));
    let mut unsupported_purpose = native.clone();
    // A rule choice among objects is the Stack recipient; a Quick window never selects targets.
    unsupported_purpose.purpose = player::QuestionPurpose::QuickWindow;
    assert!(matches!(
        project_question(&unsupported_purpose, &observation, &id),
        Err(ProjectionError::Unavailable)
    ));
    let mut too_large = native;
    if let QuestionShape::Decision(DecisionShape::Targets(domain)) = &mut too_large.shape {
        domain.cardinality.maximum = Some(Nat(u64::MAX));
    }
    assert!(matches!(
        project_question(&too_large, &observation, &id),
        Err(ProjectionError::Range)
    ));
}

#[test]
fn ordinary_target_roles_keep_empty_roles_and_native_role_size_validation() {
    let program = program(json!({"cost":{"pp":1_i32},"targets":[
        {"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"upto":2_i32,
            "roles":{"primary":{"if_nonempty":1_i32},"rest":"remaining"}}
    ],"do":[{"damage":{"to":{"role":{"of":"enemy","name":"primary"}},"n":1_i32}}]}));
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = board(actor, &program);
        let native = question(&game, actor).unwrap();
        let (_, shape, binding) = project_question(
            &native,
            &components(&game, actor),
            &QuestionId::new("roles".into()),
        )
        .unwrap();
        let selection = selection(&shape).unwrap();
        assert_eq!(selection.roles.len(), 2);
        assert!(
            matches!(selection.roles[0].size, wire::RoleSize::IfNonempty { count } if count.get() == 1)
        );
        assert_eq!(selection.roles[1].size, wire::RoleSize::Remainder);
        let selected = vec![selection.candidates[0].id.clone()];
        let good = Answer::Select {
            selected: selected.clone(),
            groups: None,
            roles: vec![
                RoleChoice {
                    role: selection.roles[0].role,
                    selected: selected.clone(),
                },
                RoleChoice {
                    role: selection.roles[1].role,
                    selected: vec![],
                },
            ],
        };
        let mut missing = good.clone();
        if let Answer::Select { roles, .. } = &mut missing {
            roles.pop();
        }
        assert_eq!(binding.answer(native.token, &missing), Err(AnswerError));
        let wrong_size = Answer::Select {
            selected: selected.clone(),
            groups: None,
            roles: vec![
                RoleChoice {
                    role: selection.roles[0].role,
                    selected: vec![],
                },
                RoleChoice {
                    role: selection.roles[1].role,
                    selected,
                },
            ],
        };
        let before = board_fixtures::exact_authority(&game);
        assert_eq!(
            game.submit_player(actor, binding.answer(native.token, &wrong_size).unwrap())
                .kind,
            SubmissionKind::Rejected
        );
        assert_unchanged(&game, &before);
        assert_eq!(
            game.submit_player(actor, binding.answer(native.token, &good).unwrap())
                .kind,
            SubmissionKind::Applied
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            1_i32
        );
    }
}
