use super::super::*;
use super::joint::submit_exact;
use super::select::{components, domain, program, project_question, question, seats};
use crate::battle::question::common::seat;
use crate::battle::tests::{board_fixture, board_fixtures};
use crate::game::player::Viewer;
use crate::game::{Game, View};
use crate::ir::{CardProgram, WellFormedCardProgram};
use alloc::sync::Arc;
use serde_json::{Value, json};
use sve_battle_contract::{QuestionId, Zone};

fn cards(numbers: &[&str], prefix: &str) -> Value {
    json!(
        numbers
            .iter()
            .enumerate()
            .map(|(index, card)| json!({"id":format!("{prefix}-{index}"),"card":card}))
            .collect::<Vec<_>>()
    )
}

fn make(actor: Viewer, edge: &str, deck: &[&str], other_secret: &str, count: u32) -> Game {
    let selected = json!({"in":"seen"});
    let program = program(json!({"cost":{"pp":1_u32},"do":[
        {"look":{"top":{"who":"self","n":count},"bind":"seen"}},
        {"move":{"ref":selected,"to":"deck","position":edge,"order":"chooser"}},
        {"life":{"who":"self","delta":1_u32}}
    ]}));
    let (own, other) = seats(actor);
    let mut initial = board_fixtures::setup(actor.player().seat, json!([]));
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["zones"] =
        json!({"field":[{"id":"source","card":"unit-follower"}],"deck":cards(deck,"deck")});
    initial["players"][other]["zones"] = json!({"hand":[{"id":"hidden","card":other_secret}]});
    let mut game = Game::new(
        Arc::clone(&board_fixture::fixture().unwrap().catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "decision-order",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &empty).unwrap();
    }
    game.load_internal_card("unit-follower", &program).unwrap();
    game.activate_internal_card(
        "source",
        program.card().abilities.first().unwrap().node.id,
        actor.player(),
    )
    .unwrap();
    game.advance_player_input();
    game
}

fn deck() -> [&'static str; 5] {
    [
        "unit-follower",
        "unit-spell",
        "unit-amulet",
        "unit-spell",
        "unit-follower",
    ]
}

#[test]
fn looked_cards_keep_native_order_direction_destination_and_full_permutation_both_seats() {
    for actor in [Viewer::P1, Viewer::P2] {
        for (edge, expected, position) in [
            (
                "top",
                json!(["deck-2", "deck-1", "deck-0", "deck-3", "deck-4"]),
                wire::Edge::Top,
            ),
            (
                "bottom",
                json!(["deck-3", "deck-4", "deck-2", "deck-1", "deck-0"]),
                wire::Edge::Bottom,
            ),
        ] {
            let mut game = make(actor, edge, &deck(), "unit-spell", 3);
            let native = question(&game, actor).unwrap();
            assert!(
                matches!(domain(&native), Some(DecisionShape::Order(_))),
                "Look creates the real native Order stop"
            );
            let before = board_fixtures::exact_authority(&game);
            let (usage, shape, binding) = project_question(
                &native,
                &components(&game, actor),
                &QuestionId::new("order".into()),
            )
            .unwrap();
            assert!(
                board_fixtures::exact_authority(&game) == before,
                "projection preserves authority"
            );
            assert_eq!(usage, wire::QuestionUse::OrderCards);
            let wire::QuestionShape::Order {
                items,
                direction,
                destination,
            } = shape
            else {
                panic!("order keeps its permutation answer");
            };
            assert_eq!(direction, wire::OrderDirection::TopDown);
            assert_eq!(
                destination,
                Some(wire::Destination {
                    player: seat(actor),
                    zone: Zone::Deck,
                    position: Some(position)
                })
            );
            assert_eq!(items.len(), 3);
            let answer = Answer::Order {
                order: items.iter().rev().map(|item| item.id.clone()).collect(),
            };
            let command = binding.answer(native.token, &answer).unwrap();
            assert!(matches!(
                &command,
                player::Command::Answer {
                    answer: player::Answer::Order(_),
                    ..
                }
            ));
            submit_exact(&mut game, &native, &binding, &answer);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state[seats(actor).0]["deck"], expected);
            assert_eq!(state[seats(actor).0]["leader"]["life"], 21_u32);
        }
    }
}

fn projected(game: &Game, actor: Viewer) -> (player::Question, wire::QuestionShape, Binding) {
    let native = question(game, actor).unwrap();
    let (usage, shape, binding) = project_question(
        &native,
        &components(game, actor),
        &QuestionId::new("order".into()),
    )
    .unwrap();
    assert_eq!(usage, wire::QuestionUse::OrderCards);
    (native, shape, binding)
}

mod privacy;
mod validation;
