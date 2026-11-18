use super::super::select::seats;
use super::*;
use crate::battle::tests::{board_fixture, payment_fixtures};
use crate::game::Game;
use crate::ir::lowering::ExpandedLowerer;
use crate::ir::{self, CardProgram, WellFormedCardProgram};
use alloc::sync::Arc;
use serde_json::Value;

fn private_board(actor: Viewer, source: &str, secret: &str) -> Game {
    let (own, other) = seats(actor);
    let mut initial = board_fixtures::setup(actor.player().seat, json!([]));
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["zones"] = json!({
        "hand":[{"id":"spell","card":"unit-spell"},{"id":"amulet","card":"unit-amulet"}],
        "field":[{"id":"recipient","card":"unit-follower"}]
    });
    initial["players"][other]["zones"] = json!({"hand":[{"id":"secret","card":secret}]});
    let mut game = Game::new(
        Arc::clone(&board_fixture::fixture().unwrap().catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "private-replacement",
    )
    .unwrap();
    game.load_internal_card(
        "unit-follower",
        &WellFormedCardProgram::check(CardProgram {
            abilities: vec![],
            card_play_costs: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    let mut activated = None;
    for number in ["unit-spell", "unit-amulet"] {
        let mut tree=payment_fixtures::x_document(ir::payment_expanded_tree(),"activate",json!({"cost":{"pp":0_i32},"do":[
            {"apply":{"on":{"who":"self","zone":"field","kind":"follower"},"effect":{"damage_mod":{"delta":2_i32}},"until":"turn"}},
            {"apply":{"on":{"who":"self","zone":"field","kind":"follower"},"effect":{"damage_cap":{"threshold":2_i32,"set":1_i32}},"until":"turn"}},
            {"damage":{"to":{"who":"self","zone":"field","kind":"follower"},"n":3_i32}}
        ]})).unwrap();
        let template = tree["cards"]["unit-follower"].take();
        tree["cards"] = json!({number:template});
        let mut card = ExpandedLowerer::new()
            .unwrap()
            .lower(&tree)
            .unwrap()
            .remove(number)
            .unwrap()
            .card()
            .clone();
        card.abilities[0].node.active_zones = vec![ir::ZoneKind::Hand];
        let program = WellFormedCardProgram::check(card).unwrap();
        if number.strip_prefix("unit-") == Some(source) {
            activated = Some(program.card().abilities[0].node.id);
        }
        game.load_internal_card(number, &program).unwrap();
    }
    game.activate_internal_card(source, activated.unwrap(), actor.player())
        .unwrap();
    game.advance_player_input();
    game
}

#[test]
fn actual_private_replacement_sources_preserve_both_complete_authorized_views() {
    let id = QuestionId::new("private-replacement".into());
    for actor in [Viewer::P1, Viewer::P2] {
        let other = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        let references = &board_fixture::fixture().unwrap().references;
        let mut previous = None;
        let mut domains = vec![];
        for source in ["spell", "amulet"] {
            let game = private_board(actor, source, "unit-spell");
            let before = board_fixtures::exact_authority(&game);
            let native = question(&game, actor).unwrap();
            domains.push(native.shape.clone());
            project_game(&game, actor, &id).unwrap();
            let waiting = (
                game.input(other),
                game.observe(other).unwrap(),
                components(&game, other),
            );
            assert!(matches!(waiting.0, Input::Waiting(_)));
            if let Some(previous) = &previous {
                assert!(
                    *previous == waiting,
                    "private providers preserve the complete waiting view"
                );
            }
            previous = Some(waiting);
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
                "projection and rejected answers preserve authority"
            );
        }
        assert!(
            domains[0] != domains[1],
            "real native replacement sources differ"
        );
        let mut actor_view = None;
        for secret in ["unit-spell", "unit-amulet"] {
            let game = private_board(actor, "spell", secret);
            let observation = components(&game, actor);
            let (usage, shape, _) = project_game(&game, actor, &id).unwrap();
            let view = (game.observe(actor).unwrap(), observation, usage, shape);
            if let Some(actor_view) = &actor_view {
                assert!(
                    *actor_view == view,
                    "opponent secrets preserve the complete actor view"
                );
            }
            actor_view = Some(view);
        }
    }
}
