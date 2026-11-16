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
use sve_battle_contract::{Nat as WireNat, QuestionId, Zone};

fn make(actor: Viewer, body: &Value, zones: Value, other_secret: &str) -> Game {
    make_with_other(actor, body, zones, other_secret, json!({}))
}

fn make_with_other(
    actor: Viewer,
    body: &Value,
    zones: Value,
    other_secret: &str,
    mut other_zones: Value,
) -> Game {
    let program = program(body.clone());
    let (own, other) = seats(actor);
    let mut initial = board_fixtures::setup(actor.player().seat, json!([]));
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["zones"] = zones;
    other_zones["hand"] = json!([{"id":"hidden","card":other_secret}]);
    initial["players"][other]["zones"] = other_zones;
    let mut game = Game::new(
        Arc::clone(&board_fixture::fixture().unwrap().catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "decision-capacity",
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

fn field(actor: Viewer) -> Game {
    let body = json!({"cost":{"pp":1_u32},"targets":[{"bind":"returned","from":{"who":"self","zone":"cemetery","kind":"follower"},"n":2_u32}],"do":[{"move":{"ref":"returned","to":"field","result":"entered"}},{"life":{"who":"self","delta":1_u32}}]});
    let zones = json!({"field":[
        {"id":"source","card":"unit-follower"},{"id":"old-a","card":"unit-follower"},{"id":"old-b","card":"unit-follower"},{"id":"old-c","card":"unit-follower"}
    ],"cemetery":[{"id":"grave-a","card":"unit-follower"},{"id":"grave-b","card":"unit-follower"}]});
    let mut game = make(actor, &body, zones, "unit-spell");
    choose_all(&mut game, actor, "targets");
    game
}

fn choose_all(game: &mut Game, actor: Viewer, id: &str) {
    let targets = question(game, actor).unwrap();
    let (_, shape, binding) = project_question(
        &targets,
        &components(game, actor),
        &QuestionId::new(id.into()),
    )
    .unwrap();
    let wire::QuestionShape::Select { selection } = shape else {
        panic!("native selection");
    };
    submit_exact(
        game,
        &targets,
        &binding,
        &Answer::Select {
            selected: selection
                .candidates
                .iter()
                .map(|candidate| candidate.id.clone())
                .collect(),
            groups: None,
            roles: vec![],
        },
    );
}

#[test]
fn actual_movement_capacity_selects_exactly_the_frozen_available_spaces_both_seats() {
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = field(actor);
        let native = question(&game, actor).unwrap();
        assert!(
            matches!(domain(&native), Some(DecisionShape::MovementCapacity(_))),
            "real native movement capacity stop"
        );
        let (usage, shape, binding) = project_question(
            &native,
            &components(&game, actor),
            &QuestionId::new("capacity".into()),
        )
        .unwrap();
        assert_eq!(usage, wire::QuestionUse::MovementCapacity);
        let wire::QuestionShape::Select { selection } = shape else {
            panic!("capacity selection");
        };
        assert_eq!(selection.candidates.len(), 2);
        assert_eq!(selection.bounds.min, WireNat::from(1_u32));
        assert_eq!(selection.bounds.max, Some(selection.bounds.min));
        assert_eq!(
            selection.capacity,
            Some(wire::Capacity {
                destination: wire::Destination {
                    player: seat(actor),
                    zone: Zone::Field,
                    position: None
                },
                available: selection.bounds.min
            })
        );
        let answer = Answer::Select {
            selected: vec![selection.candidates.last().unwrap().id.clone()],
            groups: None,
            roles: vec![],
        };
        assert!(matches!(
            binding.answer(native.token, &answer).unwrap(),
            player::Command::Answer {
                answer: player::Answer::SearchPartition(_),
                ..
            }
        ));
        submit_exact(&mut game, &native, &binding, &answer);
        let state = game.projection(View::Referee).unwrap();
        assert!(
            state[seats(actor).0]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("grave-b"))
        );
        assert_eq!(state[seats(actor).0]["cemetery"], json!(["grave-a"]));
        assert_eq!(state[seats(actor).0]["leader"]["life"], 21_u32);
    }
}

mod fixture;
mod token;

mod movement;

mod transform;

mod mixed;
mod validation;
