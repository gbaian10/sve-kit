use super::*;
use crate::game::player::Input;

#[test]
fn private_partition_remainders_preserve_waiting_complete_views_at_both_real_stops() {
    let refs = &board_fixture::fixture().unwrap().references;
    for actor in [Viewer::P1, Viewer::P2] {
        let other = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        let mut a = make(
            actor,
            &["unit-follower", "unit-follower", "unit-follower"],
            "unit-spell",
        );
        let mut b = make(
            actor,
            &["unit-spell", "unit-follower", "unit-follower"],
            "unit-spell",
        );
        for (stage, count) in [("first", 3_usize), ("second", 2)] {
            let captured_a = question(&a, actor).unwrap();
            let captured_b = question(&b, actor).unwrap();
            let (
                Some(DecisionShape::SearchPartition(domain_a)),
                Some(DecisionShape::SearchPartition(domain_b)),
            ) = (domain(&captured_a), domain(&captured_b))
            else {
                panic!("two real native partition stops");
            };
            assert_eq!(domain_a.selection.cards.len(), count);
            assert_eq!(domain_b.selection.cards.len(), count);
            assert_ne!(
                domain_a.selection.cards, domain_b.selection.cards,
                "actual private identities differ before projection"
            );
            let before_a = board_fixtures::exact_authority(&a);
            let before_b = board_fixtures::exact_authority(&b);
            assert!(
                a.observe(other).unwrap() == b.observe(other).unwrap(),
                "complete waiting boards hide private allocation members"
            );
            assert!(
                a.input(other) == b.input(other),
                "complete Waiting summaries hide the private remainder"
            );
            assert!(matches!(a.input(other), Input::Waiting(_)));
            assert_eq!(components(&a, other), components(&b, other));
            let id = QuestionId::new(stage.into());
            assert!(matches!(a.battle_question(other, &id, refs), Ok(None)));
            assert!(matches!(b.battle_question(other, &id, refs), Ok(None)));
            assert!(matches!(a.battle_question(actor, &id, refs), Ok(Some(_))));
            let (native_a, items_a, buckets_a, binding_a) = projected(&a, actor, stage);
            let (native_b, items_b, buckets_b, binding_b) = projected(&b, actor, stage);
            assert_ne!(items_a, items_b);
            assert_eq!(buckets_a, buckets_b);
            assert!(
                board_fixtures::exact_authority(&a) == before_a,
                "projection preserves authority"
            );
            assert!(
                board_fixtures::exact_authority(&b) == before_b,
                "projection preserves authority"
            );
            if stage == "first" {
                submit_exact(&mut a, &native_a, &binding_a, &answer(&items_a, &[2]));
                submit_exact(&mut b, &native_b, &binding_b, &answer(&items_b, &[2]));
            }
        }
    }
}

#[test]
fn opponent_secrets_do_not_change_actor_complete_partition_views_at_either_stop() {
    for actor in [Viewer::P1, Viewer::P2] {
        let cards = ["unit-follower", "unit-spell", "unit-amulet"];
        let mut a = make(actor, &cards, "unit-spell");
        let mut b = make(actor, &cards, "unit-amulet");
        for stage in ["first", "second"] {
            assert!(
                a.observe(actor).unwrap() == b.observe(actor).unwrap(),
                "complete actor boards hide opponent secrets"
            );
            assert!(
                a.input(actor) == b.input(actor),
                "complete native questions are unchanged"
            );
            assert_eq!(components(&a, actor), components(&b, actor));
            let (native_a, items_a, buckets_a, binding_a) = projected(&a, actor, stage);
            let (native_b, items_b, buckets_b, binding_b) = projected(&b, actor, stage);
            assert_eq!(items_a, items_b);
            assert_eq!(buckets_a, buckets_b);
            if stage == "first" {
                submit_exact(&mut a, &native_a, &binding_a, &answer(&items_a, &[2]));
                submit_exact(&mut b, &native_b, &binding_b, &answer(&items_b, &[2]));
            }
        }
    }
}
