use super::*;
use crate::battle::tests::{board_fixture, rule_fixtures};
use crate::catalog::{Catalog, EngineIdentityInput};
use crate::game::Game;
use crate::game::player::Input;
use crate::ir::{self, CardProgram};
use alloc::sync::Arc;
use serde_json::Value;

fn private_board(actor: Viewer, source: &str, secret: &str) -> Game {
    let fixture = board_fixture::fixture().unwrap();
    let (_, keywords, document) = board_fixtures::catalog_documents(&json!("1"));
    let rules = rule_fixtures::rules(
        &fixture.snapshot,
        json!([rule_fixtures::title(
            "synthetic",
            "unit-follower",
            json!([{"kind":"ub_enabled"}])
        )]),
        json!([]),
    );
    let catalog = Arc::new(
        Catalog::from_documents_with_rules(
            &fixture.snapshot,
            &keywords,
            &[("synthetic".into(), document)],
            &rules.to_string(),
            &EngineIdentityInput::LegacyJp,
        )
        .unwrap(),
    );
    let (own, other) = seats(actor);
    let mut initial = board_fixtures::setup(actor.player().seat, json!([]));
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["construction"] = json!("title");
    initial["players"][own]["title_code"] = json!("synthetic");
    initial["players"][own]["zones"] = json!({"field":[{"id":"receiver-first","card":"unit-follower"},{"id":"receiver-last","card":"unit-follower"}],"hand":[{"id":"follower","card":"unit-follower"},{"id":"amulet","card":"unit-amulet"}]});
    initial["players"][other]["zones"] = json!({"hand":[{"id":"secret","card":secret}]});
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "private-ability",
    )
    .unwrap();
    game.load_internal_card(
        "unit-spell",
        &WellFormedCardProgram::check(CardProgram {
            abilities: vec![],
            card_play_costs: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    let mut activated = None;
    for number in ["unit-follower", "unit-amulet"] {
        let mut tree=payment_fixtures::x_document(payment_expanded_tree(),"activate",json!({"cost":{"pp":0_i32},
            "targets":[{"bind":"friend","from":{"who":"self","zone":"field","kind":"follower"},"n":1_i32}],
            "do":[{"play_ability":{"of":"friend","ability":{"ub":true,"n":1_i32},"cost_policy":"waive_original"}}]

        })).unwrap();
        let mut child = tree["cards"]["unit-follower"]["abilities"][0].clone();
        child.as_object_mut().unwrap().remove("activate");
        child["line"] = json!(4_i32);
        child["activate"] =
            json!({"ub":true,"cost":{"pp":0_i32},"do":[{"life":{"who":"self","delta":2_i32}}]});
        tree["cards"]["unit-follower"]["abilities"]
            .as_array_mut()
            .unwrap()
            .push(child);
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
    let target = question(&game, actor).unwrap();
    let DecisionShape::Targets(selection) = domain(&target).unwrap() else {
        panic!("formal target")
    };
    assert_eq!(selection.members.len(), 2);
    let picked = usize::from(source == "amulet");
    assert_eq!(
        game.submit_player(
            actor,
            player::Command::Answer {
                token: target.token,
                answer: player::Answer::Targets(vec![selection.members[picked].handle])
            }
        )
        .kind,
        SubmissionKind::Applied
    );
    game
}

#[test]
fn private_parents_and_selected_holders_preserve_both_complete_authorized_views() {
    let id = QuestionId::new("private-ability".into());
    let references = &board_fixture::fixture().unwrap().references;
    for actor in [Viewer::P1, Viewer::P2] {
        let other = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        let mut previous = None;
        let mut domains = vec![];
        for source in ["follower", "amulet"] {
            let mut game = private_board(actor, source, "unit-spell");
            let before = board_fixtures::exact_authority(&game);
            let native = question(&game, actor).unwrap();
            domains.push(native.shape.clone());
            let (_, shape, binding) =
                project_question(&native, &components(&game, actor), &id).unwrap();
            let waiting = (
                game.input(other),
                game.observe(other).unwrap(),
                components(&game, other),
            );
            assert!(matches!(waiting.0, Input::Waiting(_)));
            if let Some(previous) = &previous {
                assert!(
                    previous == &waiting,
                    "private parents and selected holders preserve the full waiting view"
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
                Err(ProjectionError::Unavailable)
            ));
            assert!(
                board_fixtures::exact_authority(&game) == before,
                "projection and rejected answers preserve authority"
            );
            let wire::QuestionShape::Choose { options, .. } = shape else {
                panic!("ability choose")
            };
            assert_eq!(
                game.submit_player(
                    actor,
                    binding
                        .answer(
                            native.token,
                            &Answer::Choose {
                                chosen: vec![options[0].id.clone()]
                            }
                        )
                        .unwrap()
                )
                .kind,
                SubmissionKind::Applied
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()[seats(actor).0]["leader"]["life"],
                22_i32
            );
        }
        assert!(
            domains[0] != domains[1],
            "native provider identities must actually differ"
        );
        let mut actor_view = None;
        for secret in ["unit-spell", "unit-amulet"] {
            let game = private_board(actor, "follower", secret);
            let native = question(&game, actor).unwrap();
            let observation = components(&game, actor);
            let (usage, shape, _) = project_question(&native, &observation, &id).unwrap();
            let view = (game.observe(actor).unwrap(), observation, usage, shape);
            if let Some(actor_view) = &actor_view {
                assert!(
                    actor_view == &view,
                    "opponent secrets preserve the full actor view"
                );
            }
            actor_view = Some(view);
        }
    }
}
