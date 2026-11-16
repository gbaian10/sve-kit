use super::*;
use crate::game::player::Input;

fn native_cards(game: &Game, actor: Viewer) -> Vec<player::CardOption> {
    let native = question(game, actor).unwrap();
    let Some(DecisionShape::Order(order)) = domain(&native) else {
        panic!("native Order");
    };
    order.cards.clone()
}

#[test]
fn different_real_private_order_domains_preserve_the_waiting_seat_complete_views_both_seats() {
    let refs = &board_fixture::fixture().unwrap().references;
    let id = QuestionId::new("private-order".into());
    for actor in [Viewer::P1, Viewer::P2] {
        let other = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        for edge in ["top", "bottom"] {
            let a = make(actor, edge, &deck(), "unit-spell", 3);
            let b = make(
                actor,
                edge,
                &[
                    "unit-follower",
                    "unit-follower",
                    "unit-amulet",
                    "unit-spell",
                    "unit-follower",
                ],
                "unit-spell",
                3,
            );
            let candidates_a = native_cards(&a, actor);
            let candidates_b = native_cards(&b, actor);
            assert_eq!(candidates_a.len(), 3);
            assert_eq!(candidates_b.len(), 3);
            assert!(
                candidates_a != candidates_b,
                "equal-sized decks produce different real card identities before projection"
            );
            let before_a = board_fixtures::exact_authority(&a);
            let before_b = board_fixtures::exact_authority(&b);
            assert!(
                a.observe(other).unwrap() == b.observe(other).unwrap(),
                "complete waiting native boards hide private permutations"
            );
            assert!(
                a.input(other) == b.input(other),
                "complete Waiting summaries hide domain sizes and identities"
            );
            assert!(matches!(a.input(other), Input::Waiting(_)));
            assert_eq!(components(&a, other), components(&b, other));
            assert!(matches!(a.battle_question(other, &id, refs), Ok(None)));
            assert!(matches!(b.battle_question(other, &id, refs), Ok(None)));
            assert_ne!(
                projected(&a, actor).1,
                projected(&b, actor).1,
                "only the chooser receives its frozen domain"
            );
            assert!(matches!(
                a.battle_question(actor, &id, refs),
                Err(ProjectionError::Unavailable)
            ));
            assert!(
                board_fixtures::exact_authority(&a) == before_a,
                "projection preserves complete authority"
            );
            assert!(
                board_fixtures::exact_authority(&b) == before_b,
                "projection preserves complete authority"
            );
        }
    }
}

#[test]
fn opponent_secrets_do_not_change_the_actor_complete_board_or_order_question() {
    for actor in [Viewer::P1, Viewer::P2] {
        for edge in ["top", "bottom"] {
            let a = make(actor, edge, &deck(), "unit-spell", 3);
            let b = make(actor, edge, &deck(), "unit-amulet", 3);
            assert!(
                a.observe(actor).unwrap() == b.observe(actor).unwrap(),
                "complete actor boards hide opponent secrets"
            );
            assert!(
                a.input(actor) == b.input(actor),
                "the complete native order question is unchanged"
            );
            assert_eq!(components(&a, actor), components(&b, actor));
            assert_eq!(projected(&a, actor).1, projected(&b, actor).1);
        }
    }
}

#[test]
fn native_empty_orders_skip_but_single_card_orders_still_require_the_complete_answer() {
    for actor in [Viewer::P1, Viewer::P2] {
        let empty = make(actor, "bottom", &[], "unit-spell", 1);
        let empty_input = question(&empty, actor).unwrap();
        assert!(
            !matches!(domain(&empty_input), Some(DecisionShape::Order(_))),
            "native empty order continues without a fabricated question"
        );
        let state = empty.projection(View::Referee).unwrap();
        assert_eq!(state[seats(actor).0]["leader"]["life"], 21_u32);
        assert_eq!(state[seats(actor).0]["pp"]["current"], 1_u32);
        let mut game = make(actor, "bottom", &["unit-follower"], "unit-spell", 1);
        let (native, shape, binding) = projected(&game, actor);
        let wire::QuestionShape::Order { items, .. } = shape else {
            panic!("one card still produces native Order");
        };
        assert_eq!(items.len(), 1);
        assert_eq!(
            binding.decode(&Answer::Order { order: vec![] }),
            Err(AnswerError)
        );
        submit_exact(
            &mut game,
            &native,
            &binding,
            &Answer::Order {
                order: vec![items.first().unwrap().id.clone()],
            },
        );
    }
}
