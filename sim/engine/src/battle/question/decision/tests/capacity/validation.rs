use super::*;
use crate::game::player::{CapacityCard, observation::ObservedZone};
use crate::ir::Nat;
use sve_battle_contract::{OptionId, RoleChoice};

#[test]
fn movement_answers_require_the_exact_shape_count_and_scoped_coordinates() {
    for actor in [Viewer::P1, Viewer::P2] {
        let game = field(actor);
        let native = question(&game, actor).unwrap();
        let (_, shape, binding) = project_question(
            &native,
            &components(&game, actor),
            &QuestionId::new("movement".into()),
        )
        .unwrap();
        let wire::QuestionShape::Select { selection } = shape else {
            panic!("select");
        };
        let id = selection.candidates[0].id.clone();
        let before = board_fixtures::exact_authority(&game);
        let (_, other_shape, _) = project_question(
            &native,
            &components(&game, actor),
            &QuestionId::new("other".into()),
        )
        .unwrap();
        let wire::QuestionShape::Select { selection: other } = other_shape else {
            panic!("select");
        };
        for invalid in [
            Answer::Confirm { accept: true },
            Answer::Choose {
                chosen: vec![id.clone()],
            },
            Answer::Select {
                selected: vec![],
                groups: None,
                roles: vec![],
            },
            Answer::Select {
                selected: vec![id.clone(); 2],
                groups: None,
                roles: vec![],
            },
            Answer::Select {
                selected: selection
                    .candidates
                    .iter()
                    .map(|item| item.id.clone())
                    .collect(),
                groups: None,
                roles: vec![],
            },
            Answer::Select {
                selected: vec![OptionId::new("foreign".into())],
                groups: None,
                roles: vec![],
            },
            Answer::Select {
                selected: vec![other.candidates[0].id.clone()],
                groups: None,
                roles: vec![],
            },
            Answer::Select {
                selected: vec![id.clone()],
                groups: Some(vec![]),
                roles: vec![],
            },
            Answer::Select {
                selected: vec![id.clone()],
                groups: None,
                roles: vec![RoleChoice {
                    role: WireNat::ZERO,
                    selected: vec![id],
                }],
            },
        ] {
            assert_eq!(binding.decode(&invalid), Err(AnswerError));
        }
        assert!(
            board_fixtures::exact_authority(&game) == before,
            "invalid selection answers preserve full authority"
        );
    }
}

#[test]
fn token_answers_require_the_exact_shape_count_and_scoped_coordinates() {
    for actor in [Viewer::P1, Viewer::P2] {
        let game = token::game(actor, "ex");
        let (native, shape, binding) = fixture::project_game(&game, actor, "tokens");
        let wire::QuestionShape::Choose { options, .. } = shape else {
            panic!("choose");
        };
        let (_, other, _) = fixture::project_game(&game, actor, "other");
        let wire::QuestionShape::Choose { options: other, .. } = other else {
            panic!("choose");
        };
        let id = options[0].id.clone();
        let before = board_fixtures::exact_authority(&game);
        for invalid in [
            Answer::Confirm { accept: true },
            Answer::Select {
                selected: vec![id.clone()],
                groups: None,
                roles: vec![],
            },
            Answer::Choose { chosen: vec![] },
            Answer::Choose {
                chosen: vec![id; 2],
            },
            Answer::Choose {
                chosen: vec![options[0].id.clone(), options[1].id.clone()],
            },
            Answer::Choose {
                chosen: vec![OptionId::new("foreign".into())],
            },
            Answer::Choose {
                chosen: vec![other[0].id.clone()],
            },
        ] {
            assert_eq!(binding.answer(native.token, &invalid), Err(AnswerError));
        }
        assert!(
            board_fixtures::exact_authority(&game) == before,
            "invalid token answers preserve full authority"
        );
    }
}

#[test]
fn incomplete_or_inconsistent_movement_facts_refuse_the_whole_question() {
    let game = field(Viewer::P1);
    let board = components(&game, Viewer::P1);
    for mode in 0_u32..13 {
        let mut native = question(&game, Viewer::P1).unwrap();
        let player::QuestionShape::Decision(DecisionShape::MovementCapacity(capacity)) =
            &mut native.shape
        else {
            panic!("capacity");
        };
        match mode {
            0 => capacity.facts = None,
            1 => capacity.facts.as_mut().unwrap().clear(),
            2 => capacity.zone = ObservedZone::Hand,
            3 => capacity.available = Nat(0),
            4 => capacity.available = Nat(2),
            5 => capacity.selection.cardinality.maximum = None,
            6 => capacity.selection.cards[1].handle = capacity.selection.cards[0].handle,
            7 => capacity.selection.cards[0].object = None,
            8 => capacity.selection.cards[0].card = "unit-spell".into(),
            _ => {
                let facts = capacity
                    .facts
                    .as_mut()
                    .unwrap()
                    .first_entry()
                    .unwrap()
                    .into_mut();
                let CapacityCard::Existing { face, controller } = facts else {
                    panic!("existing");
                };
                match mode {
                    9 => *controller = Some(Viewer::P2),
                    10 => face.face = u64::MAX,
                    11 => face.face = 1,
                    12 => *facts = CapacityCard::Token { face: face.clone() },
                    _ => panic!("thirteen invalid captured facts"),
                }
            }
        }
        assert!(
            project_question(&native, &board, &QuestionId::new("invalid".into())).is_err(),
            "case {mode}"
        );
    }
    let native = question(&game, Viewer::P1).unwrap();
    assert!(matches!(
        project(domain(&native).unwrap(), None),
        Err(ProjectionError::Unavailable)
    ));
    let mut wrong = board;
    wrong.viewer = seat(Viewer::P2);
    assert!(project_question(&native, &wrong, &QuestionId::new("wrong".into())).is_err());
}

#[test]
fn token_metadata_is_complete_and_comes_from_the_captured_printing() {
    let game = token::game(Viewer::P1, "field");
    let board = fixture::board(&game, Viewer::P1);
    let fixture = fixture::fixture();
    for mode in 0_u32..13 {
        let mut native = question(&game, Viewer::P1).unwrap();
        let player::QuestionShape::Decision(DecisionShape::TokenCapacity(capacity)) =
            &mut native.shape
        else {
            panic!("tokens");
        };
        match mode {
            0 => capacity.faces = None,
            1 => capacity.faces.as_mut().unwrap().clear(),
            2 => capacity.choices.clear(),
            3 => capacity.choices[0].clear(),
            4 => capacity.choices[0][0] = " ".into(),
            5 => capacity.choices[0][0] = "capacity-b".into(),
            6 => capacity.faces.as_mut().unwrap()[0].clear(),
            7 => capacity.faces.as_mut().unwrap()[0][0].face = u64::MAX,
            8 => capacity.faces.as_mut().unwrap()[0][0].card = "unit-follower".into(),
            9 => capacity.available = Nat(0),
            10 => capacity.available = Nat(1),
            11 => capacity.zone = ObservedZone::Hand,
            12 => capacity.available = Nat(u64::MAX),
            _ => panic!("thirteen incomplete token facts"),
        }
        assert!(
            project(
                domain(&native).unwrap(),
                Some(Context {
                    id: &QuestionId::new("invalid".into()),
                    actor: native.actor,
                    purpose: native.purpose,
                    observation: &board,
                    catalog: &fixture.catalog,
                    references: &fixture.references
                })
            )
            .is_err(),
            "case {mode}"
        );
    }
}
