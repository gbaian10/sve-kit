use super::*;
use crate::game::player::Input;

#[test]
fn different_real_search_domains_hide_candidates_and_group_sizes_from_the_waiting_seat() {
    let refs = &board_fixture::fixture().unwrap().references;
    let id = QuestionId::new("private-search".into());
    for actor in [Viewer::P1, Viewer::P2] {
        let other = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        for procedure in [single(), grouped()] {
            let a = make(actor, &procedure, &deck(), "unit-spell");
            let b = make(
                actor,
                &procedure,
                &["unit-amulet", "unit-spell", "unit-amulet"],
                "unit-spell",
            );
            let native_a = question(&a, actor).unwrap();
            let native_b = question(&b, actor).unwrap();
            let (Some(DecisionShape::Search(search_a)), Some(DecisionShape::Search(search_b))) =
                (domain(&native_a), domain(&native_b))
            else {
                panic!("real Search domains");
            };
            assert!(
                search_a != search_b,
                "native candidate facts differ before projection"
            );
            if search_a.groups.is_none() {
                assert_ne!(
                    search_a.cards.len(),
                    search_b.cards.len(),
                    "equal-sized decks have different candidate counts"
                );
            } else {
                assert_eq!(search_a.cards.len(), search_b.cards.len());
                assert_ne!(
                    search_a.groups, search_b.groups,
                    "printed group eligibility differs"
                );
            }
            let before_a = board_fixtures::exact_authority(&a);
            let before_b = board_fixtures::exact_authority(&b);
            assert!(
                a.observe(other).unwrap() == b.observe(other).unwrap(),
                "complete waiting native boards hide the private search"
            );
            assert!(
                a.input(other) == b.input(other),
                "complete Waiting summaries hide domain sizes and groups"
            );
            assert!(matches!(a.input(other), Input::Waiting(_)));
            assert_eq!(components(&a, other), components(&b, other));
            assert!(matches!(a.battle_question(other, &id, refs), Ok(None)));
            assert!(matches!(b.battle_question(other, &id, refs), Ok(None)));
            assert_ne!(projected(&a, actor).1, projected(&b, actor).1);
            assert!(matches!(a.battle_question(actor, &id, refs), Ok(Some(_))));
            assert!(
                board_fixtures::exact_authority(&a) == before_a,
                "read-only projection preserves authority"
            );
            assert!(
                board_fixtures::exact_authority(&b) == before_b,
                "read-only projection preserves authority"
            );
        }
    }
}

#[test]
fn opponent_secrets_do_not_change_the_searcher_complete_board_or_question() {
    for actor in [Viewer::P1, Viewer::P2] {
        for procedure in [single(), grouped()] {
            let a = make(actor, &procedure, &deck(), "unit-spell");
            let b = make(actor, &procedure, &deck(), "unit-amulet");
            assert!(
                a.observe(actor).unwrap() == b.observe(actor).unwrap(),
                "the complete native board hides opponent secrets"
            );
            assert!(
                a.input(actor) == b.input(actor),
                "the complete native question is independent of opponent secrets"
            );
            assert_eq!(components(&a, actor), components(&b, actor));
            assert_eq!(projected(&a, actor).1, projected(&b, actor).1);
        }
    }
}

#[test]
fn empty_and_single_candidate_searches_retain_the_real_native_answer_channel() {
    for actor in [Viewer::P1, Viewer::P2] {
        for cards in [&[][..], &["unit-follower"][..]] {
            let mut game = make(actor, &single(), cards, "unit-spell");
            let (native, selection, binding) = projected(&game, actor);
            assert_eq!(selection.candidates.len(), cards.len());
            let answer = Answer::Select {
                selected: selection
                    .candidates
                    .iter()
                    .map(|candidate| candidate.id.clone())
                    .collect(),
                groups: None,
                roles: vec![],
            };
            submit_exact(&mut game, &native, &binding, &answer);
            assert_eq!(
                game.projection(View::Referee).unwrap()[seats(actor).0]["leader"]["life"],
                21_u32
            );
        }
    }
}
