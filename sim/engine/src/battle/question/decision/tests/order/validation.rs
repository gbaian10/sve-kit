use super::*;
use crate::game::player::observation::ObservedZone;
use crate::ir::{Position, PositiveNat};
use alloc::collections::BTreeSet;
use core::num::NonZeroU64;
use sve_battle_contract::{OptionId, Subject};

#[test]
fn partial_duplicate_foreign_and_wrong_tag_answers_cannot_be_permutations() {
    let game = make(Viewer::P1, "bottom", &deck(), "unit-spell", 3);
    let (_, shape, binding) = projected(&game, Viewer::P1);
    let wire::QuestionShape::Order { items, .. } = shape else {
        panic!("order");
    };
    let ids = items.iter().map(|item| item.id.clone()).collect::<Vec<_>>();
    for answer in [
        Answer::Confirm { accept: true },
        Answer::Order { order: vec![] },
        Answer::Order {
            order: ids.iter().take(2).cloned().collect(),
        },
        Answer::Order {
            order: vec![ids.first().unwrap().clone(); 3],
        },
        Answer::Order {
            order: vec![OptionId::new("foreign".into()); 3],
        },
        Answer::Order {
            order: ids.iter().cloned().chain(ids.first().cloned()).collect(),
        },
    ] {
        assert_eq!(binding.decode(&answer), Err(AnswerError));
    }
    assert_eq!(
        binding.decode(&Answer::Order { order: ids }),
        Ok(player::Answer::Order(vec![0, 1, 2]))
    );
}

#[test]
fn native_placement_and_identification_must_be_complete_in_the_same_authorized_board() {
    let game = make(Viewer::P1, "top", &deck(), "unit-spell", 3);
    let board = components(&game, Viewer::P1);
    for mode in [0_u32, 1, 2, 3, 4] {
        let mut native = question(&game, Viewer::P1).unwrap();
        let player::QuestionShape::Decision(DecisionShape::Order(order)) = &mut native.shape else {
            panic!("order");
        };
        match mode {
            0 => order.destination.player = Viewer::P2,
            1 => order.destination.zone = ObservedZone::Hand,
            2 => {
                order.position = Position::Index {
                    index: PositiveNat(NonZeroU64::new(1).unwrap()),
                }
            }
            3 => order.cards.first_mut().unwrap().object = None,
            4 => order.cards.push(order.cards.first().unwrap().clone()),
            _ => panic!("five invalid native facts"),
        }
        assert!(matches!(
            project_question(&native, &board, &QuestionId::new("invalid".into())),
            Err(ProjectionError::Unavailable)
        ));
    }
    let native = question(&game, Viewer::P1).unwrap();
    assert!(matches!(
        project(domain(&native).unwrap(), None),
        Err(ProjectionError::Unavailable)
    ));
    let mut missing = board.clone();
    missing.objects.clear();
    assert!(matches!(
        project_question(&native, &missing, &QuestionId::new("missing".into())),
        Err(ProjectionError::Unavailable)
    ));
    let mut wrong = board.clone();
    wrong.viewer = sve_battle_contract::Seat::P2;
    assert!(matches!(
        project_question(&native, &wrong, &QuestionId::new("wrong".into())),
        Err(ProjectionError::Unavailable)
    ));
    let mut moved = board;
    let owner = moved
        .players
        .iter_mut()
        .find(|player| player.seat == sve_battle_contract::Seat::P1)
        .unwrap();
    let located = owner.zones.deck.cards.pop().unwrap();
    owner.zones.hand.cards.push(located);
    assert!(matches!(
        project_question(&native, &moved, &QuestionId::new("zone".into())),
        Err(ProjectionError::Unavailable)
    ));
}

#[test]
fn identical_printings_still_have_distinct_scoped_items_and_a_real_full_permutation() {
    let mut game = make(
        Viewer::P1,
        "top",
        &[
            "unit-follower",
            "unit-follower",
            "unit-follower",
            "unit-spell",
        ],
        "unit-spell",
        3,
    );
    let (native, shape, binding) = projected(&game, Viewer::P1);
    let wire::QuestionShape::Order { items, .. } = shape else {
        panic!("order");
    };
    let mut objects = BTreeSet::new();
    for item in &items {
        let Some(Subject::Card {
            card,
            object: Some(object),
        }) = &item.subject
        else {
            panic!("same-board card capability");
        };
        assert_eq!(card.face, sve_battle_contract::Nat::from(1_u32));
        assert!(
            objects.insert(object.clone()),
            "identical printings retain distinct current objects"
        );
        assert!(item.source.is_none());
    }
    assert_eq!(objects.len(), 3);
    submit_exact(
        &mut game,
        &native,
        &binding,
        &Answer::Order {
            order: items.iter().rev().map(|item| item.id.clone()).collect(),
        },
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["deck"],
        json!(["deck-2", "deck-1", "deck-0", "deck-3"])
    );
}
