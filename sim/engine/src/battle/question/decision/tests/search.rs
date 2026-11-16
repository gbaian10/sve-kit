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
use sve_battle_contract::{QuestionId, Subject, Zone};

fn make(actor: Viewer, search: &Value, deck: &[&str], other_secret: &str) -> Game {
    make_with_look(actor, search, deck, other_secret, false)
}

fn make_with_look(
    actor: Viewer,
    search: &Value,
    deck: &[&str],
    other_secret: &str,
    look: bool,
) -> Game {
    let mut steps = vec![
        json!({"search":search}),
        json!({"life":{"who":"self","delta":1_u32}}),
    ];
    if look {
        steps.insert(
            0,
            json!({"look":{"top":{"who":"self","n":1_u32},"bind":"seen"}}),
        );
    }
    let program = program(json!({"cost":{"pp":1_u32},"do":steps}));
    let (own, other) = seats(actor);
    let mut initial = board_fixtures::setup(actor.player().seat, json!([]));
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["zones"] = json!({
        "field":[{"id":"source","card":"unit-follower"}],
        "deck":deck.iter().enumerate().map(|(index, card)|
            json!({"id":format!("deck-{index}"),"card":card})).collect::<Vec<_>>()
    });
    initial["players"][other]["zones"] = json!({"hand":[{"id":"hidden","card":other_secret}]});
    let mut game = Game::new(
        Arc::clone(&board_fixture::fixture().unwrap().catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "decision-search",
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

fn single() -> Value {
    json!({"sel":{"who":"self","zone":"deck","kind":"follower"},"upto":2_u32,"to":"hand"})
}

fn deck() -> [&'static str; 3] {
    ["unit-follower", "unit-spell", "unit-follower"]
}

fn grouped() -> Value {
    json!({"groups":[
        {"sel":{"who":"self","zone":"deck","kind":"follower"},"n":1_u32},
        {"sel":{"who":"self","zone":"deck","kind":"amulet"},"n":1_u32},
        {"sel":{"who":"self","zone":"deck","kind":"spell"},"upto":1_u32}
    ],"to":"hand"})
}

fn projected(game: &Game, actor: Viewer) -> (player::Question, wire::Selection, Binding) {
    let native = question(game, actor).unwrap();
    assert!(
        matches!(domain(&native), Some(DecisionShape::Search(_))),
        "the admitted program reaches the real native Search stop"
    );
    let before = board_fixtures::exact_authority(game);
    let (usage, shape, binding) = project_question(
        &native,
        &components(game, actor),
        &QuestionId::new("search".into()),
    )
    .unwrap();
    assert!(
        board_fixtures::exact_authority(game) == before,
        "projection preserves complete authority"
    );
    assert_eq!(usage, wire::QuestionUse::Search);
    let wire::QuestionShape::Select { selection } = shape else {
        panic!("search retains its complete selection");
    };
    (native, selection, binding)
}

mod groups;
mod privacy;
mod validation;

#[test]
fn hidden_search_printings_keep_exact_faces_without_inventing_board_links_both_seats() {
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = make(actor, &single(), &deck(), "unit-spell");
        let (native, selection, binding) = projected(&game, actor);
        assert_eq!(selection.candidates.len(), 2);
        assert_eq!(selection.group_assignment, wire::GroupAssignment::Existence);
        assert_eq!(selection.groups, []);
        let refs = &board_fixture::fixture().unwrap().references;
        let catalog = &board_fixture::fixture().unwrap().catalog;
        let expected = refs
            .resolve(
                catalog,
                &player::observation::CardFace {
                    card: "unit-follower".into(),
                    face: 0,
                },
            )
            .unwrap();
        assert_eq!(expected.face.get(), 1);
        for candidate in &selection.candidates {
            assert_eq!(
                candidate.subject,
                Some(Subject::Card {
                    card: expected.clone(),
                    object: None
                })
            );
            assert_eq!(candidate.controller, Some(seat(actor)));
            assert_eq!(
                candidate.origin,
                Some(wire::Origin {
                    player: seat(actor),
                    zone: Zone::Deck,
                    position: None
                })
            );
        }
        assert_ne!(
            selection.candidates.first().unwrap().id,
            selection.candidates.last().unwrap().id
        );
        let answer = Answer::Select {
            selected: selection
                .candidates
                .iter()
                .rev()
                .map(|candidate| candidate.id.clone())
                .collect(),
            groups: None,
            roles: vec![],
        };
        assert!(matches!(
            binding.answer(native.token, &answer).unwrap(),
            player::Command::Answer {
                answer: player::Answer::Search(player::SearchAnswer::Ungrouped(_)),
                ..
            }
        ));
        submit_exact(&mut game, &native, &binding, &answer);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[seats(actor).0]["hand"], json!(["deck-0", "deck-2"]));
        assert_eq!(state[seats(actor).0]["leader"]["life"], 21_u32);
    }
}
