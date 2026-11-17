use super::*;
use sve_engine::game::internal::{TargetDeclaration, TargetRoleAssignment};

fn target_program() -> WellFormedCardProgram {
    program(
        "activate",
        json!({"cost":{"pp":1},
            "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
            "do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]
        }),
    )
}

fn start(program: &WellFormedCardProgram, actor: Viewer) -> (Game, Question) {
    let mut game = fixture(program, actor);
    assert_eq!(
        game.internal_target_declarations(),
        None,
        "idle games have no execution"
    );
    game.activate_internal_card(
        "source",
        program.card().abilities[0].node.id,
        actor.player(),
    )
    .unwrap();
    let q = question(&game, actor);
    assert!(
        declarations(&game).is_empty(),
        "a question is not an accepted target declaration"
    );
    (game, q)
}

fn declarations(game: &Game) -> Vec<TargetDeclaration> {
    let before = exact_authority(game);
    let result = game.internal_target_declarations().unwrap().to_vec();
    for viewer in [Viewer::P1, Viewer::P2] {
        game.observe(viewer).unwrap();
    }
    assert_eq!(
        game.internal_target_declarations().unwrap(),
        result.as_slice(),
        "observations do not declare targets"
    );
    assert!(
        exact_authority(game) == before,
        "reading declarations and observing either seat is pure"
    );
    result
}

#[test]
fn accepted_targets_remain_after_the_parameter_slots_are_consumed() {
    let program = target_program();
    for actor in [Viewer::P1, Viewer::P2] {
        let (mut game, q) = start(&program, actor);
        let decision = game
            .internal_decision(View::Referee)
            .unwrap()
            .token
            .decision;
        let wrong = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        let before = exact_authority(&game);
        assert_eq!(
            submit(&mut game, wrong, &q, Answer::Targets(vec![0])),
            SubmissionKind::Rejected
        );
        for answer in [
            Answer::Targets(vec![99]),
            Answer::Targets(vec![0, 0]),
            Answer::Integer(Nat(0)),
        ] {
            assert_eq!(
                submit(&mut game, actor, &q, answer),
                SubmissionKind::Rejected
            );
        }
        assert!(
            exact_authority(&game) == before,
            "rejected answers cannot leave declarations"
        );
        assert_eq!(
            submit(&mut game, actor, &q, Answer::Targets(vec![0])),
            SubmissionKind::Applied
        );
        let selected = declarations(&game);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].player, actor.player());
        assert_eq!(selected[0].decision, decision);
        assert_eq!(selected[0].objects.len(), 1);
        assert_eq!(selected[0].roles, vec![]);
        assert_eq!(game.internal_play_phase(), Some(PlayPhase::Resolve));
        closed_wire(&game, actor);
        finish_may(&mut game);
        assert_eq!(game.internal_target_declarations(), None);
    }
}

#[test]
fn roles_keep_physical_members_separate_from_selection_order_including_empty_answers() {
    let program = program(
        "activate",
        json!({"cost":{"pp":1},
            "targets":[{"bind":"enemies","from":{"who":"opp","zone":"field","kind":"follower"},
                "upto":2,"roles":{"primary":{"if_nonempty":1},"rest":"remaining"}}],
            "do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]
        }),
    );
    for actor in [Viewer::P1, Viewer::P2] {
        for nonempty in [false, true] {
            let (mut game, q) = start(&program, actor);
            let targets = if nonempty { vec![1, 0] } else { vec![] };
            let roles = vec![
                TargetRoleAssignment {
                    role: Nat(0),
                    targets: if nonempty { vec![0] } else { vec![] },
                },
                TargetRoleAssignment {
                    role: Nat(1),
                    targets: if nonempty { vec![1] } else { vec![] },
                },
            ];
            let before = exact_authority(&game);
            assert_eq!(
                submit(&mut game, actor, &q, Answer::Targets(targets.clone())),
                SubmissionKind::Rejected
            );
            assert!(
                exact_authority(&game) == before,
                "omitting required roles is inert"
            );
            assert_eq!(
                submit(
                    &mut game,
                    actor,
                    &q,
                    Answer::TargetsWithRoles { targets, roles }
                ),
                SubmissionKind::Applied
            );
            let selected = declarations(&game);
            assert_eq!(selected.len(), 1);
            assert_eq!(selected[0].player, actor.player());
            assert_eq!(selected[0].roles.len(), 2);
            assert_eq!(selected[0].roles[0].role, Nat(0));
            assert_eq!(selected[0].roles[1].role, Nat(1));
            if nonempty {
                assert_eq!(selected[0].objects.len(), 2);
                assert_ne!(selected[0].objects[0], selected[0].objects[1]);
                assert_eq!(selected[0].roles[0].objects, vec![selected[0].objects[1]]);
                assert_eq!(selected[0].roles[1].objects, vec![selected[0].objects[0]]);
            } else {
                assert_eq!(selected[0].objects, vec![]);
                assert!(selected[0].roles.iter().all(|role| role.objects.is_empty()));
            }
            closed_wire(&game, actor);
            finish_may(&mut game);
            assert_eq!(game.internal_target_declarations(), None);
        }
    }
}

#[test]
fn a_later_resolution_selection_does_not_replace_the_formal_target_declaration() {
    let program = program(
        "activate",
        json!({"cost":{"pp":1},
            "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
            "do":[{"choose":{"bind":"later","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}},
                {"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]
        }),
    );
    for actor in [Viewer::P1, Viewer::P2] {
        let (mut game, q) = start(&program, actor);
        assert_eq!(
            submit(&mut game, actor, &q, Answer::Targets(vec![0])),
            SubmissionKind::Applied
        );
        let first = declarations(&game);
        assert_eq!(first.len(), 1);
        let later = question(&game, actor);
        assert_eq!(later.purpose, QuestionPurpose::ResolutionChoice);
        assert_eq!(
            submit(&mut game, actor, &later, Answer::Targets(vec![1])),
            SubmissionKind::Applied
        );
        assert_eq!(declarations(&game), first);
        finish_may(&mut game);
        assert_eq!(game.internal_target_declarations(), None);
    }
}

#[test]
fn a_body_fault_discards_the_accepted_targets_with_the_execution() {
    let program = lifecycle::with_overflow(&target_program());
    for actor in [Viewer::P1, Viewer::P2] {
        let (mut game, q) = start(&program, actor);
        assert_eq!(
            submit(&mut game, actor, &q, Answer::Targets(vec![0])),
            SubmissionKind::Applied
        );
        assert_eq!(declarations(&game).len(), 1);
        let may = question(&game, actor);
        assert_eq!(
            submit(&mut game, actor, &may, Answer::Integer(Nat(1))),
            SubmissionKind::EngineFault
        );
        assert_eq!(game.internal_target_declarations(), None);
        let me = if actor == Viewer::P1 { "P1" } else { "P2" };
        assert_eq!(
            game.projection(View::Referee).unwrap()[me]["pp"]["current"],
            3
        );
    }
}
