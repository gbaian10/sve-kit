use super::*;
use crate::battle::tests::board_fixtures;
use crate::game::player::{Command, Question};
use sve_battle_contract::{Nat as WireNat, OptionId};

fn mode_domain(native: &mut Question) -> Option<&mut DecisionShape> {
    if let player::QuestionShape::Decision(shape) = &mut native.shape {
        Some(shape)
    } else {
        None
    }
}

#[test]
fn modes_reject_bad_answers_wrong_seats_and_stale_tokens_without_mutation() {
    let program = modes_program();
    for actor in [Viewer::P1, Viewer::P2] {
        let other = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        let mut game = board(actor, &program);
        let native = question(&game, actor).unwrap();
        let before = board_fixtures::exact_authority(&game);
        let (_, shape, binding) = project_question(
            &native,
            &components(&game, actor),
            &QuestionId::new("reject-modes".into()),
        )
        .unwrap();
        let options = if let wire::QuestionShape::Choose { options, .. } = shape {
            Some(options)
        } else {
            None
        }
        .unwrap();
        let first = options.first().unwrap().id.clone();
        let last = options.last().unwrap().id.clone();
        for answer in [
            Answer::Confirm { accept: true },
            Answer::Choose { chosen: vec![] },
            Answer::Choose {
                chosen: vec![first.clone()],
            },
            Answer::Choose {
                chosen: options.iter().map(|option| option.id.clone()).collect(),
            },
            Answer::Choose {
                chosen: vec![first.clone(), first.clone()],
            },
            Answer::Choose {
                chosen: vec![first.clone(), OptionId::new("foreign".into())],
            },
        ] {
            assert_eq!(binding.decode(&answer), Err(AnswerError));
        }
        let foreign = Command::Answer {
            token: native.token,
            answer: player::Answer::Modes(vec![ModeId(9), ModeId(999)]),
        };
        assert_eq!(
            game.submit_player(actor, foreign).kind,
            SubmissionKind::Rejected
        );
        let command = binding
            .answer(
                native.token,
                &Answer::Choose {
                    chosen: vec![last, first],
                },
            )
            .unwrap();
        assert_eq!(
            game.submit_player(other, command.clone()).kind,
            SubmissionKind::Rejected
        );
        assert!(
            board_fixtures::exact_authority(&game) == before,
            "reads and rejected mode commands preserve authority"
        );
        assert_eq!(
            game.submit_player(actor, command.clone()).kind,
            SubmissionKind::Applied
        );
        let after = board_fixtures::exact_authority(&game);
        assert_eq!(
            game.submit_player(actor, command).kind,
            SubmissionKind::Rejected
        );
        assert!(
            board_fixtures::exact_authority(&game) == after,
            "stale mode commands preserve authority"
        );
    }
}

#[test]
fn mode_projection_refuses_missing_context_duplicates_inconsistent_bounds_and_wire_overflow() {
    let game = board(Viewer::P1, &modes_program());
    let native = question(&game, Viewer::P1).unwrap();
    let id = QuestionId::new("mode-facts".into());
    let observation = components(&game, Viewer::P1);
    assert!(matches!(
        project(domain(&native).unwrap(), None),
        Err(ProjectionError::Unavailable)
    ));
    assert!(matches!(
        project_question(&native, &components(&game, Viewer::P2), &id),
        Err(ProjectionError::Unavailable)
    ));
    let overflow = WireNat::MAX.get().checked_add(1).unwrap();
    for fault in 0_u8..6 {
        let mut invalid = native.clone();
        if let DecisionShape::Modes {
            modes,
            cardinality,
            count,
        } = mode_domain(&mut invalid).unwrap()
        {
            match fault {
                0 => modes.push(*modes.first().unwrap()),
                1 => *modes.first_mut().unwrap() = ModeId(overflow),
                2 => cardinality.minimum = ir::Nat(3),
                3 => cardinality.maximum = Some(ir::Nat(overflow)),
                4 => *count = ir::Nat(overflow),
                _ => *count = ir::Nat(0),
            }
        }
        let expected = if matches!(fault, 1 | 3 | 4) {
            ProjectionError::Range
        } else {
            ProjectionError::Unavailable
        };
        assert_eq!(
            project_question(&invalid, &observation, &id).err(),
            Some(expected)
        );
    }
}
