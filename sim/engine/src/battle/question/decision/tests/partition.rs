use super::super::*;
use super::joint::submit_exact;
use super::search;
use super::select::{
    components, domain, project_game, project_question, question, seats, submit_projected,
};
use crate::battle::question::common::seat;
use crate::battle::tests::{board_fixture, board_fixtures};
use crate::game::player::Viewer;
use crate::game::{Game, View};
use serde_json::json;
use sve_battle_contract::{BucketChoice, Nat as WireNat, QuestionId, Zone};

fn make(actor: Viewer, deck: &[&str], other_secret: &str) -> Game {
    let procedure = json!({"sel":{"who":"self","zone":"deck"},"n":3_u32,
        "to":{"order":"printed","partition":[{"zone":"hand","n":1_u32},{"zone":"cemetery","n":2_u32}]}});
    begin(actor, &procedure, deck, other_secret)
}

fn begin(actor: Viewer, procedure: &serde_json::Value, deck: &[&str], other_secret: &str) -> Game {
    let mut game = search::make(actor, procedure, deck, other_secret);
    let (native, selection, binding) = search::projected(&game, actor);
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
    game
}

fn projected(
    game: &Game,
    actor: Viewer,
    id: &str,
) -> (
    player::Question,
    Vec<wire::Candidate>,
    Vec<wire::PartitionBucket>,
    Binding,
) {
    let native = question(game, actor).unwrap();
    assert!(
        matches!(domain(&native), Some(DecisionShape::SearchPartition(_))),
        "real native allocation stop"
    );
    let before = board_fixtures::exact_authority(game);
    let (usage, shape, binding) = project_question(
        &native,
        &components(game, actor),
        &QuestionId::new(id.into()),
    )
    .unwrap();
    assert_eq!(usage, wire::QuestionUse::SearchPartition);
    assert!(
        board_fixtures::exact_authority(game) == before,
        "read-only projection preserves complete authority"
    );
    let wire::QuestionShape::Partition { items, buckets } = shape else {
        panic!("complete partition");
    };
    (native, items, buckets, binding)
}

fn answer(items: &[wire::Candidate], selected: &[usize]) -> Answer {
    Answer::Partition {
        buckets: vec![
            BucketChoice {
                bucket: WireNat::from(1_u32),
                items: items
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| !selected.contains(index))
                    .map(|(_, item)| item.id.clone())
                    .collect(),
            },
            BucketChoice {
                bucket: WireNat::ZERO,
                items: items
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| selected.contains(index))
                    .map(|(_, item)| item.id.clone())
                    .collect(),
            },
        ],
    }
}

#[test]
fn every_native_partition_stop_includes_the_complete_remainder_and_actual_destination() {
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = make(
            actor,
            &["unit-follower", "unit-spell", "unit-amulet"],
            "unit-spell",
        );
        let first = question(&game, actor).unwrap();
        let (_, shape, binding) =
            project_game(&game, actor, &QuestionId::new("first".into())).unwrap();
        let wire::QuestionShape::Partition { items, buckets } = shape else {
            panic!("partition");
        };
        assert_eq!(items.len(), 3);
        assert_eq!(buckets.len(), 2);
        assert_eq!(
            buckets.first().unwrap().destination,
            Some(wire::Destination {
                player: seat(actor),
                zone: Zone::Hand,
                position: None
            })
        );
        assert_eq!(
            buckets.first().unwrap().bounds,
            wire::Bounds {
                min: WireNat::from(1_u32),
                max: Some(WireNat::from(1_u32))
            }
        );
        assert_eq!(buckets.last().unwrap().destination, None);
        assert_eq!(
            buckets.last().unwrap().bounds,
            wire::Bounds {
                min: WireNat::from(2_u32),
                max: Some(WireNat::from(2_u32))
            }
        );
        assert!(buckets.iter().all(|bucket| !bucket.ordered));
        submit_projected(&mut game, &first, &binding, &answer(&items, &[2]));
        assert_eq!(
            game.projection(View::Referee).unwrap()[seats(actor).0]["hand"],
            json!(["deck-2"])
        );
        let second = question(&game, actor).unwrap();
        let (_, second_shape, second_binding) =
            project_game(&game, actor, &QuestionId::new("second".into())).unwrap();
        let wire::QuestionShape::Partition {
            items: remaining,
            buckets: second_buckets,
        } = second_shape
        else {
            panic!("partition");
        };
        assert_eq!(remaining.len(), 2);
        assert_eq!(
            second_buckets.first().unwrap().destination.unwrap().zone,
            Zone::Cemetery
        );
        assert_eq!(second_buckets.last().unwrap().bounds.min, WireNat::ZERO);
        submit_projected(
            &mut game,
            &second,
            &second_binding,
            &answer(&remaining, &[0, 1]),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state[seats(actor).0]["cemetery"],
            json!(["deck-0", "deck-1"])
        );
        assert_eq!(state[seats(actor).0]["deck"], json!([]));
        assert_eq!(state[seats(actor).0]["leader"]["life"], 21_u32);
    }
}

mod edges;
mod privacy;
mod validation;
