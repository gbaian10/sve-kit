use super::*;
use crate::battle::tests::{board_fixture, board_fixtures, payment_fixtures};
use crate::game::Game;
use crate::ir::lowering::ExpandedLowerer;
use alloc::sync::Arc;
use serde_json::Value;

fn hand_program(number: &str, count: i32) -> WellFormedCardProgram {
    let options = (1_i32..=count)
        .map(|delta| json!([{"life":{"who":"self","delta":delta}}]))
        .collect::<Vec<_>>();
    let mut document = payment_fixtures::x_document(
        ir::payment_expanded_tree(),
        "activate",
        json!({"cost":{"steps":[{"pp":1_i32},{"discard":{"sel":{"who":"self","zone":"hand"},"n":1_i32}}]},"do":[{"choice":{"n":1_i32,"options":options}}]}),
    )
    .unwrap();
    let entry = document["cards"]["unit-follower"].take();
    document["cards"] = json!({number:entry});
    let mut card = ExpandedLowerer::new()
        .unwrap()
        .lower(&document)
        .unwrap()
        .remove(number)
        .unwrap()
        .card()
        .clone();
    card.abilities.first_mut().unwrap().node.active_zones = vec![ir::ZoneKind::Hand];
    WellFormedCardProgram::check(card).unwrap()
}

fn private_board(actor: Viewer, source: &str, secret: &str) -> Game {
    let (own, other) = seats(actor);
    let mut initial = board_fixtures::setup(actor.player().seat, json!([]));
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["pp"] = json!({"current":3_i32,"max":3_i32});
    initial["players"][own]["zones"] = json!({"hand":[{"id":"follower","card":"unit-follower"},{"id":"amulet","card":"unit-amulet"}]});
    initial["players"][other]["zones"] = json!({"hand":[{"id":"secret","card":secret}]});
    let mut game = Game::new(
        Arc::clone(&board_fixture::fixture().unwrap().catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "private-modes",
    )
    .unwrap();
    let mut definition = None;
    for (number, count) in [("unit-follower", 2_i32), ("unit-amulet", 3_i32)] {
        let program = hand_program(number, count);
        if number.strip_prefix("unit-") == Some(source) {
            definition = Some(program.card().abilities.first().unwrap().node.id);
        }
        game.load_internal_card(number, &program).unwrap();
    }
    game.load_internal_card(
        "unit-spell",
        &WellFormedCardProgram::check(ir::CardProgram {
            abilities: vec![],
            card_play_costs: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    game.activate_internal_card(source, definition.unwrap(), actor.player())
        .unwrap();
    game.advance_player_input();
    game
}

#[test]
fn private_mode_sources_change_real_candidates_without_changing_the_waiting_view() {
    let id = QuestionId::new("private-modes".into());
    for actor in [Viewer::P1, Viewer::P2] {
        let other = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        let mut waiting = None;
        let mut unpaid_view = None;
        let mut domains = vec![];
        for source in ["follower", "amulet"] {
            let mut game = private_board(actor, source, "unit-spell");
            let before = board_fixtures::exact_authority(&game);
            let native = question(&game, actor).unwrap();
            domains.push(native.shape.clone());
            let (_, shape, binding) = project_game(&game, actor, &id).unwrap();
            let view = (
                game.input(other),
                game.observe(other).unwrap(),
                components(&game, other),
            );
            assert!(matches!(view.0, Input::Waiting(_)));
            if let Some(previous) = &waiting {
                assert!(
                    previous == &view,
                    "private sources preserve complete waiting views"
                );
            }
            waiting = Some(view);
            let references = &board_fixture::fixture().unwrap().references;
            assert!(
                game.battle_question(other, &id, references)
                    .unwrap()
                    .is_none()
            );
            assert!(matches!(
                game.battle_question(actor, &id, references),
                Ok(Some(_))
            ));
            assert!(
                board_fixtures::exact_authority(&game) == before,
                "mode projection preserves authority"
            );
            let option = if let wire::QuestionShape::Choose { options, .. } = &shape {
                options.first()
            } else {
                None
            }
            .unwrap();
            let command = binding
                .answer(
                    native.actor,
                    &binding.question().id,
                    &Answer::Choose {
                        chosen: vec![option.id.clone()],
                    },
                )
                .unwrap();
            assert_eq!(
                game.submit_player(actor, command).kind,
                SubmissionKind::Applied
            );
            assert!(matches!(
                domain(&question(&game, actor).unwrap()),
                Some(DecisionShape::Discard {
                    kind: player::DiscardKind::Cost,
                    ..
                })
            ));
            assert_eq!(
                game.projection(View::Referee).unwrap()[seats(actor).0]["pp"]["current"],
                3_i32
            );
            let after_choice = (
                game.input(other),
                game.observe(other).unwrap(),
                components(&game, other),
            );
            if let Some(previous) = &unpaid_view {
                assert!(
                    previous == &after_choice,
                    "chosen modes remain private before payment"
                );
            }
            unpaid_view = Some(after_choice);
        }
        assert!(
            domains.first() != domains.last(),
            "native mode candidates must actually differ"
        );
        opponent_secrets_preserve_the_actor_view(actor, &id);
    }
}

fn opponent_secrets_preserve_the_actor_view(actor: Viewer, id: &QuestionId) {
    let mut actor_view = None;
    for secret in ["unit-spell", "unit-amulet"] {
        let game = private_board(actor, "follower", secret);
        let observation = components(&game, actor);
        let (usage, shape, _) = project_game(&game, actor, id).unwrap();
        let view = (game.observe(actor).unwrap(), observation, usage, shape);
        if let Some(previous) = &actor_view {
            assert!(
                previous == &view,
                "opponent secrets preserve the full actor view"
            );
        }
        actor_view = Some(view);
    }
}
