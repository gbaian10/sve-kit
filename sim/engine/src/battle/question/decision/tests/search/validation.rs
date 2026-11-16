use super::*;
use crate::game::player::observation::ObservedZone;
use crate::ir::Nat;
use sve_battle_contract::{OptionId, Seat};

#[test]
fn wrong_tag_duplicate_foreign_and_oversized_search_answers_are_rejected() {
    let game = make(Viewer::P1, &single(), &deck(), "unit-spell");
    let (_, selection, binding) = projected(&game, Viewer::P1);
    let id = selection.candidates.first().unwrap().id.clone();
    for answer in [
        Answer::Confirm { accept: true },
        Answer::Select {
            selected: vec![id.clone(), id.clone()],
            groups: None,
            roles: vec![],
        },
        Answer::Select {
            selected: vec![OptionId::new("foreign".into())],
            groups: None,
            roles: vec![],
        },
        Answer::Select {
            selected: vec![id.clone(); 3],
            groups: None,
            roles: vec![],
        },
        Answer::Select {
            selected: vec![id],
            groups: Some(vec![]),
            roles: vec![],
        },
    ] {
        assert_eq!(binding.decode(&answer), Err(AnswerError));
    }
    let count_only = json!({"sel":{"who":"self","zone":"deck"},"n":2_u32,"to":"hand"});
    let required_game = make(Viewer::P1, &count_only, &deck(), "unit-spell");
    let (_, _, required_binding) = projected(&required_game, Viewer::P1);
    assert_eq!(
        required_binding.decode(&Answer::Select {
            selected: vec![],
            groups: None,
            roles: vec![]
        }),
        Err(AnswerError)
    );
}

fn native_search(native: &mut player::Question) -> &mut player::SearchQuestion {
    let player::QuestionShape::Decision(DecisionShape::Search(search)) = &mut native.shape else {
        panic!("native search");
    };
    search
}

#[test]
fn missing_inconsistent_or_unbound_frozen_search_facts_stop_projection() {
    let game = make(Viewer::P1, &grouped(), &deck(), "unit-spell");
    let board = components(&game, Viewer::P1);
    let id = QuestionId::new("invalid".into());
    for mode in 0_u32..10 {
        let mut native = question(&game, Viewer::P1).unwrap();
        let search = native_search(&mut native);
        match mode {
            0 => search.facts = None,
            1 => search.facts.as_mut().unwrap().origin.player = Viewer::P2,
            2 => search.facts.as_mut().unwrap().origin.zone = ObservedZone::Hand,
            3 => search.facts.as_mut().unwrap().faces.clear(),
            4 => {
                let face = search
                    .facts
                    .as_ref()
                    .unwrap()
                    .faces
                    .first_key_value()
                    .unwrap()
                    .1
                    .clone();
                search.facts.as_mut().unwrap().faces.insert(99, face);
            }
            5 => {
                search
                    .facts
                    .as_mut()
                    .unwrap()
                    .faces
                    .first_entry()
                    .unwrap()
                    .get_mut()
                    .face = u64::MAX;
            }
            6 => {
                search
                    .facts
                    .as_mut()
                    .unwrap()
                    .faces
                    .first_entry()
                    .unwrap()
                    .get_mut()
                    .card = "unit-amulet".into();
            }
            7 => search.cards.first_mut().unwrap().handle = 99,
            8 => {
                let duplicate = search.cards.first().unwrap().handle;
                search.cards.last_mut().unwrap().handle = duplicate;
            }
            9 => search.cardinality.minimum = Nat(4),
            _ => panic!("ten incomplete facts"),
        }
        assert!(matches!(
            project_question(&native, &board, &id),
            Err(ProjectionError::Unavailable)
        ));
    }
    let native = question(&game, Viewer::P1).unwrap();
    assert!(matches!(
        project(domain(&native).unwrap(), None),
        Err(ProjectionError::Unavailable)
    ));
    let mut wrong = board;
    wrong.viewer = Seat::P2;
    assert!(matches!(
        project_question(&native, &wrong, &id),
        Err(ProjectionError::Unavailable)
    ));
}

#[test]
fn required_names_costs_and_numeric_ranges_cannot_be_dropped_from_search() {
    let constrained = json!({"sel":{"who":"self","zone":"deck"},"upto":3_u32,"distinct_by":"name","cost_sum_le":2_u32,"to":"hand"});
    let game = make(Viewer::P1, &constrained, &deck(), "unit-spell");
    for mode in 0_u32..4 {
        let mut native = question(&game, Viewer::P1).unwrap();
        let search = native_search(&mut native);
        let expected = match mode {
            0 => {
                search.cards.first_mut().unwrap().rules_names = None;
                ProjectionError::Unavailable
            }
            1 => {
                search.cards.first_mut().unwrap().original_cost = None;
                ProjectionError::Unavailable
            }
            2 => {
                search.cards.first_mut().unwrap().original_cost = Some(i64::MAX);
                ProjectionError::Range
            }
            3 => {
                search.cardinality.maximum = Some(Nat(u64::MAX));
                ProjectionError::Range
            }
            _ => panic!("four incomplete constraints"),
        };
        assert!(
            matches!(project_question(&native, &components(&game, Viewer::P1), &QuestionId::new("facts".into())), Err(error) if error == expected)
        );
    }
}

#[test]
fn a_prior_real_look_preserves_only_the_existing_same_board_link_without_its_slot() {
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = make_with_look(actor, &single(), &deck(), "unit-spell", true);
        let (native, selection, binding) = projected(&game, actor);
        let linked = selection
            .candidates
            .iter()
            .filter(|candidate| {
                matches!(
                    &candidate.subject,
                    Some(Subject::Card {
                        object: Some(_),
                        ..
                    })
                )
            })
            .count();
        assert_eq!(linked, 1);
        assert!(
            selection.candidates.iter().all(|candidate| candidate
                .origin
                .unwrap()
                .position
                .is_none())
        );
        let board = components(&game, actor);
        for mode in 0_u32..5 {
            let mut broken = board.clone();
            let owner = broken
                .players
                .iter_mut()
                .find(|player| player.seat == seat(actor))
                .unwrap();
            let located = owner.zones.deck.cards.first().unwrap().clone();
            match mode {
                0 => broken.objects.clear(),
                1 => owner.zones.deck.cards.push(located),
                2 => {
                    owner.zones.deck.cards.clear();
                    owner.zones.hand.cards.push(located);
                }
                3 => {
                    broken.objects.get_mut(&located.object).unwrap().controller =
                        seat(if actor == Viewer::P1 {
                            Viewer::P2
                        } else {
                            Viewer::P1
                        });
                }
                4 => {
                    broken
                        .objects
                        .get_mut(&located.object)
                        .unwrap()
                        .effective_face = board_fixture::fixture()
                        .unwrap()
                        .references
                        .resolve(
                            &board_fixture::fixture().unwrap().catalog,
                            &player::observation::CardFace {
                                card: "unit-spell".into(),
                                face: 0,
                            },
                        )
                        .unwrap();
                }
                _ => panic!("five broken links"),
            }
            assert!(matches!(
                project_question(&native, &broken, &QuestionId::new("link".into())),
                Err(ProjectionError::Unavailable)
            ));
        }
        submit_exact(
            &mut game,
            &native,
            &binding,
            &Answer::Select {
                selected: selection
                    .candidates
                    .iter()
                    .map(|candidate| candidate.id.clone())
                    .collect(),
                groups: None,
                roles: vec![],
            },
        );
    }
}
