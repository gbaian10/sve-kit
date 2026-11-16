use super::*;
use crate::battle::tests::{board_fixture, board_fixtures};
use crate::game::player::Input;

#[test]
fn unchosen_search_secrets_preserve_the_complete_waiting_replacement_view() {
    let id = QuestionId::new("private-route".into());
    let references = &board_fixture::fixture().unwrap().references;
    for actor in [Viewer::P1, Viewer::P2] {
        let other = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        let mut previous = None;
        let mut counts = vec![];
        for deck in [
            ["unit-follower", "unit-follower"],
            ["unit-follower", "unit-amulet"],
        ] {
            let (game, preceding) = searched(actor, &deck, "unit-spell", vec![0], 2);
            let DecisionShape::Search(facts) = domain(&preceding).unwrap() else {
                panic!("search candidates")
            };
            counts.push(facts.cards.len());
            let before = board_fixtures::exact_authority(&game);
            let native = question(&game, actor).unwrap();
            assert!(matches!(
                domain(&native),
                Some(DecisionShape::SearchReplacement(_))
            ));
            project_question(&native, &components(&game, actor), &id).unwrap();
            let view = (
                game.input(other),
                game.observe(other).unwrap(),
                components(&game, other),
            );
            assert!(matches!(view.0, Input::Waiting(_)));
            if let Some(previous) = &previous {
                assert!(
                    previous == &view,
                    "unchosen search cards preserve the full waiting view"
                );
            }
            previous = Some(view);
            assert!(
                game.battle_question(other, &id, references)
                    .unwrap()
                    .is_none()
            );
            assert!(matches!(
                game.battle_question(actor, &id, references),
                Err(ProjectionError::Unavailable)
            ));
            assert!(
                board_fixtures::exact_authority(&game) == before,
                "route projection preserves authority"
            );
        }
        assert_ne!(
            counts[0], counts[1],
            "the real preceding private search domains differ"
        );
    }
}

#[test]
fn opponent_secrets_preserve_both_complete_actor_route_views() {
    let id = QuestionId::new("private-opponent-route".into());
    for actor in [Viewer::P1, Viewer::P2] {
        for exchange in [false, true] {
            let mut previous = None;
            for secret in ["unit-spell", "unit-amulet"] {
                let game = if exchange {
                    exchange::board(actor, false, secret)
                } else {
                    searched(actor, &["unit-follower", "unit-spell"], secret, vec![0], 2).0
                };
                let before = board_fixtures::exact_authority(&game);
                let native = question(&game, actor).unwrap();
                let observation = components(&game, actor);
                let (usage, shape, _) = project_question(&native, &observation, &id).unwrap();
                let view = (game.observe(actor).unwrap(), observation, usage, shape);
                if let Some(previous) = &previous {
                    assert!(
                        previous == &view,
                        "opponent secrets preserve the full actor board and route choices"
                    );
                }
                previous = Some(view);
                assert!(
                    board_fixtures::exact_authority(&game) == before,
                    "route projection preserves authority"
                );
            }
        }
    }
}
