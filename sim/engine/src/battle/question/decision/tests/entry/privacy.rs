use super::*;

#[test]
fn entry_private_sources_preserve_complete_waiting_and_opponent_views() {
    let id = QuestionId::new("private-entry".into());
    let references = &board_fixture::fixture().unwrap().references;
    for actor in [Viewer::P1, Viewer::P2] {
        let other = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        for ward in [false, true] {
            let mut previous = None;
            let mut domains = vec![];
            for first in [false, true] {
                let game = board(actor, ward, first, "unit-spell");
                let before = board_fixtures::exact_authority(&game);
                let native = question(&game, actor).unwrap();
                domains.push(native.shape.clone());
                project_game(&game, actor, &id).unwrap();
                assert!(matches!(game.input(other), Input::Waiting(_)));
                assert!(
                    game.battle_question(other, &id, references)
                        .unwrap()
                        .is_none()
                );
                assert!(matches!(
                    game.battle_question(actor, &id, references),
                    Ok(Some(_))
                ));
                let view = (
                    game.input(other),
                    game.observe(other).unwrap(),
                    components(&game, other),
                );
                if let Some(previous) = &previous {
                    assert!(
                        previous == &view,
                        "private sources preserve the complete waiting view"
                    );
                }
                previous = Some(view);
                assert!(
                    board_fixtures::exact_authority(&game) == before,
                    "private projection preserves authority"
                );
            }
            if ward {
                assert!(
                    domains.first() != domains.last(),
                    "native Ward sources must actually differ"
                );
            }
            let mut actor_view = None;
            for secret in ["unit-spell", "unit-amulet"] {
                let game = board(actor, ward, true, secret);
                let observation = components(&game, actor);
                let (usage, shape, _) = project_game(&game, actor, &id).unwrap();
                let view = (game.observe(actor).unwrap(), observation, usage, shape);
                if let Some(actor_view) = &actor_view {
                    assert!(
                        actor_view == &view,
                        "opponent secrets preserve the complete actor view"
                    );
                }
                actor_view = Some(view);
            }
        }
    }
}
