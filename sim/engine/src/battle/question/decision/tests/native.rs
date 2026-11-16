use super::super::*;
use crate::battle::tests::{board_fixture, board_fixtures, payment_fixtures};
use crate::game::player::{Command, Input, Question, QuestionShape, SubmissionKind, Viewer};
use crate::game::{Game, View};
use crate::ir::lowering::ExpandedLowerer;
use crate::ir::{CardProgram, PlayerId, WellFormedCardProgram, payment_expanded_tree};
use alloc::sync::Arc;
use serde_json::{Value, json};
use sve_battle_contract::QuestionId;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scenario {
    OptionalCost,
    ResolutionPayment,
    May,
}

impl Scenario {
    const fn shape(self) -> DecisionShape {
        match self {
            Self::OptionalCost => DecisionShape::OptionalCost,
            Self::ResolutionPayment => DecisionShape::ResolutionPayment,
            Self::May => DecisionShape::May,
        }
    }
}

fn question(game: &Game, actor: Viewer) -> Option<Question> {
    if let Input::Question(question) = game.input(actor) {
        Some(*question)
    } else {
        None
    }
}

fn program(scenario: Scenario) -> WellFormedCardProgram {
    let body = json!([{"life":{"who":"self","delta":1_i32}}]);
    let (head, body) = match scenario {
        Scenario::OptionalCost => (
            "fanfare",
            json!({"optional_cost":{"pp":1_i32,"result":"paid"},"do":body}),
        ),
        Scenario::ResolutionPayment => (
            "auto",
            json!({"on":{"phase":{"whose":"self","which":"end"}},"do":[{"pay":{"cost":{"pp":1_i32},"do":body}}]}),
        ),
        Scenario::May => (
            "activate",
            json!({"cost":{"pp":0_i32},"do":[{"optional":{"do":body}}]}),
        ),
    };
    let document = payment_fixtures::x_document(payment_expanded_tree(), head, body).unwrap();
    ExpandedLowerer::new()
        .unwrap()
        .lower(&document)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn make(scenario: Scenario, actor: Viewer, program: &WellFormedCardProgram) -> Game {
    let (seat, own, other) = match actor {
        Viewer::P1 => (1, "P1", "P2"),
        Viewer::P2 => (2, "P2", "P1"),
    };
    let mut initial = board_fixtures::setup(seat, json!([]));
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["pp"] = json!({"current":3_i32,"max":3_i32});
    initial["players"][own]["zones"] = if scenario == Scenario::OptionalCost {
        json!({"hand":[{"id":"source","card":"unit-follower"}]})
    } else {
        json!({"field":[{"id":"source","card":"unit-follower"}]})
    };
    initial["players"][other]["zones"] = json!({});
    let fixture = board_fixture::fixture().unwrap();
    let mut game = Game::new(
        Arc::clone(&fixture.catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "decision-confirm",
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
    game.load_internal_card("unit-follower", program).unwrap();
    match scenario {
        Scenario::OptionalCost => {
            game.play_internal_card("source", PlayerId { seat })
                .unwrap();
        }
        Scenario::ResolutionPayment => {
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
        }
        Scenario::May => {
            let id = program.card().abilities.first().unwrap().node.id;
            game.activate_internal_card("source", id, PlayerId { seat })
                .unwrap();
        }
    }
    game.advance_player_input();
    let pending = question(&game, actor).unwrap();
    if let QuestionShape::Decision(DecisionShape::Pending(choices)) = pending.shape {
        assert_eq!(choices.len(), 1);
        assert_eq!(
            game.submit_player(
                actor,
                Command::Answer {
                    token: pending.token,
                    answer: player::Answer::Pending(choices.first().unwrap().handle),
                },
            )
            .kind,
            SubmissionKind::Applied
        );
    }
    game
}

fn assert_outcome(game: &Game, scenario: Scenario, actor: Viewer, accept: bool) {
    let state = game.projection(View::Referee).unwrap();
    let own = if actor == Viewer::P1 { "P1" } else { "P2" };
    let pp = match (scenario, accept) {
        (Scenario::OptionalCost, true) => 1_i32,
        (Scenario::OptionalCost, false) | (Scenario::ResolutionPayment, true) => 2_i32,
        (Scenario::ResolutionPayment, false) | (Scenario::May, _) => 3_i32,
    };
    assert_eq!(state[own]["pp"]["current"], pp);
    let life = if scenario == Scenario::OptionalCost || accept {
        21_i32
    } else {
        20_i32
    };
    assert_eq!(state[own]["leader"]["life"], life);
}

#[test]
fn confirm_commands_keep_native_tokens_and_submit_to_the_producing_game() {
    let references = &board_fixture::fixture().unwrap().references;
    for scenario in [
        Scenario::OptionalCost,
        Scenario::ResolutionPayment,
        Scenario::May,
    ] {
        let domain = scenario.shape();
        let program = program(scenario);
        for actor in [Viewer::P1, Viewer::P2] {
            let other = match actor {
                Viewer::P1 => Viewer::P2,
                Viewer::P2 => Viewer::P1,
            };
            for accept in [false, true] {
                let mut game = make(scenario, actor, &program);
                let native = question(&game, actor).unwrap();
                assert_eq!(native.shape, QuestionShape::Decision(domain.clone()));
                assert_eq!(
                    native.purpose,
                    if domain == DecisionShape::OptionalCost {
                        player::QuestionPurpose::PlayParameter
                    } else {
                        player::QuestionPurpose::ResolutionChoice
                    }
                );
                let before = board_fixtures::exact_authority(&game);
                let (_, _, binding) = project(&domain).unwrap();
                game.observe(actor).unwrap();
                game.observe(other).unwrap();
                assert!(
                    matches!(game.input(other), Input::Waiting(waiting) if waiting.actor == actor && waiting.dice.is_none())
                );
                assert!(matches!(
                    game.battle_question(actor, &QuestionId::new("confirm".into()), references),
                    Err(ProjectionError::Unavailable)
                ));
                assert!(
                    game.battle_question(other, &QuestionId::new("confirm".into()), references)
                        .unwrap()
                        .is_none()
                );
                assert!(
                    board_fixtures::exact_authority(&game) == before,
                    "reading a confirmation preserves authority state"
                );
                let wire_answer = Answer::Confirm { accept };
                let command = binding.answer(native.token, &wire_answer).unwrap();
                assert!(
                    matches!(&command, Command::Answer { token, .. } if *token == native.token)
                );
                assert_eq!(
                    game.submit_player(other, command.clone()).kind,
                    SubmissionKind::Rejected
                );
                assert!(
                    board_fixtures::exact_authority(&game) == before,
                    "a wrong-seat submission preserves authority state"
                );
                assert_eq!(
                    game.submit_player(actor, command.clone()).kind,
                    SubmissionKind::Applied
                );
                assert_outcome(&game, scenario, actor, accept);
                let after = board_fixtures::exact_authority(&game);
                assert_eq!(
                    game.submit_player(actor, command).kind,
                    SubmissionKind::Rejected
                );
                assert!(
                    board_fixtures::exact_authority(&game) == after,
                    "a stale submission preserves authority state"
                );
            }
        }
    }
}
