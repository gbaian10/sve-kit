use super::*;
use crate::game::player::observation::ObservedZone;
use crate::ir::{Nat, Position};
use alloc::collections::BTreeSet;
use sve_battle_contract::{OptionId, Subject};

#[test]
fn every_bucket_and_item_is_required_once_and_invalid_answers_preserve_the_stop() {
    for actor in [Viewer::P1, Viewer::P2] {
        let game = make(
            actor,
            &["unit-follower", "unit-spell", "unit-amulet"],
            "unit-spell",
        );
        let (_, items, _, binding) = projected(&game, actor, "first");
        let before = board_fixtures::exact_authority(&game);
        assert_eq!(
            binding.decode(&Answer::Confirm { accept: true }),
            Err(AnswerError)
        );
        for mode in 0_u32..9 {
            let mut invalid = answer(&items, &[0]);
            let Answer::Partition { buckets } = &mut invalid else {
                panic!("partition");
            };
            match mode {
                0 => {
                    buckets.pop();
                }
                1 => buckets.push(buckets.first().unwrap().clone()),
                2 => buckets.last_mut().unwrap().bucket = WireNat::from(1_u32),
                3 => buckets.last_mut().unwrap().bucket = WireNat::from(99_u32),
                4 => {
                    buckets.first_mut().unwrap().items.pop();
                }
                5 => {
                    buckets.first_mut().unwrap().items = vec![
                        items.first().unwrap().id.clone(),
                        items.last().unwrap().id.clone(),
                    ];
                }
                6 => {
                    buckets.first_mut().unwrap().items = vec![
                        OptionId::new("foreign".into()),
                        items.last().unwrap().id.clone(),
                    ];
                }
                7 => buckets.first_mut().unwrap().items = vec![items.last().unwrap().id.clone(); 2],
                8 => {
                    buckets.last_mut().unwrap().items.clear();
                }
                _ => panic!("nine incomplete partitions"),
            }
            assert_eq!(binding.decode(&invalid), Err(AnswerError));
        }
        assert!(
            board_fixtures::exact_authority(&game) == before,
            "invalid wire answers leave complete authority unchanged"
        );
        let native = question(&game, actor).unwrap();
        let (_, shape, _) = project_question(
            &native,
            &components(&game, actor),
            &QuestionId::new("other-question".into()),
        )
        .unwrap();
        let wire::QuestionShape::Partition {
            items: other_items, ..
        } = shape
        else {
            panic!("partition");
        };
        assert_eq!(
            binding.decode(&answer(&other_items, &[0])),
            Err(AnswerError)
        );
    }
}

#[test]
fn incomplete_frozen_placement_or_candidates_refuse_the_whole_partition() {
    let game = make(
        Viewer::P1,
        &["unit-follower", "unit-spell", "unit-amulet"],
        "unit-spell",
    );
    let board = components(&game, Viewer::P1);
    for mode in 0_u32..13 {
        let mut native = question(&game, Viewer::P1).unwrap();
        let player::QuestionShape::Decision(DecisionShape::SearchPartition(partition)) =
            &mut native.shape
        else {
            panic!("partition");
        };
        if mode == 0 {
            partition.facts = None;
        } else {
            let facts = partition.facts.as_mut().unwrap();
            match mode {
                1 => facts.candidates.origin.player = Viewer::P2,
                2 => facts.candidates.origin.zone = ObservedZone::Hand,
                3 => facts.destination.player = Viewer::P2,
                4 => facts.destination.zone = ObservedZone::Deck,
                5 => facts.position = Some(Position::Top {}),
                6 => facts.ordered = true,
                7 => facts.remainder_destination = Some(facts.destination),
                8 => facts.candidates.faces.clear(),
                9 => {
                    facts.candidates.faces.first_entry().unwrap().get_mut().face = u64::MAX;
                }
                10 => {
                    let duplicate = partition.selection.cards.first().unwrap().handle;
                    partition.selection.cards.last_mut().unwrap().handle = duplicate;
                }
                11 => partition.selection.cardinality.minimum = Nat(4),
                12 => partition.selection.cardinality.maximum = Some(Nat(4)),
                _ => panic!("twelve incomplete facts"),
            }
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
    let mut wrong = board;
    wrong.viewer = seat(Viewer::P2);
    assert!(matches!(
        project_question(&native, &wrong, &QuestionId::new("wrong".into())),
        Err(ProjectionError::Unavailable)
    ));
}

#[test]
fn identical_printings_keep_distinct_scoped_items_and_exact_permanent_faces() {
    let mut game = make(
        Viewer::P1,
        &["unit-follower", "unit-follower", "unit-follower"],
        "unit-spell",
    );
    let (native, items, _, binding) = projected(&game, Viewer::P1, "copies");
    let ids = items
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(ids.len(), 3);
    assert!(items.iter().all(|item| matches!(&item.subject, Some(Subject::Card { card, object: None }) if card.face == WireNat::from(1_u32))));
    assert!(items.iter().all(
        |item| item.origin.is_some_and(|origin| origin.zone == Zone::Deck
            && origin.player == seat(Viewer::P1)
            && origin.position.is_none())
    ));
    submit_exact(&mut game, &native, &binding, &answer(&items, &[1]));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["deck-1"])
    );
}
