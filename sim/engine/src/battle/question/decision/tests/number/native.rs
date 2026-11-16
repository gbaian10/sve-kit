use super::super::super::*;
use crate::battle::tests::{board_fixture, board_fixtures, history_fixture, payment_fixtures};
use crate::game::player::observation::DiceHistory;
use crate::game::player::{Input, Question, QuestionShape, SubmissionKind, Viewer};
use crate::game::{Game, View};
use crate::ir::lowering::ExpandedLowerer;
use crate::ir::{
    CardProgram, Cardinality, Nat, PlayerId, WellFormedCardProgram, payment_expanded_tree,
};
use alloc::sync::Arc;
use serde_json::{Value, json};
use sve_battle_contract::{Nat as WireNat, QuestionId};

mod declaration;

fn question(game: &Game, actor: Viewer) -> Option<Question> {
    if let Input::Question(question) = game.input(actor) {
        Some(*question)
    } else {
        None
    }
}

fn domain(question: &Question) -> Option<&DecisionShape> {
    if let QuestionShape::Decision(domain) = &question.shape {
        Some(domain)
    } else {
        None
    }
}

fn dice(question: &Question) -> Option<(Nat, Nat)> {
    if let DecisionShape::DiceReroll { value, remaining } = domain(question)? {
        Some((*value, *remaining))
    } else {
        None
    }
}

fn board(actor: Viewer, program: &WellFormedCardProgram) -> Game {
    let (seat, own, other) = match actor {
        Viewer::P1 => (1, "P1", "P2"),
        Viewer::P2 => (2, "P2", "P1"),
    };
    let mut initial = board_fixtures::setup(seat, json!([]));
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["pp"] = json!({"current":3_i32,"max":3_i32});
    initial["players"][own]["zones"] = json!({"field":[{"id":"source","card":"unit-follower"}]});
    initial["players"][other]["zones"] = json!({});
    let mut game = Game::new(
        Arc::clone(&board_fixture::fixture().unwrap().catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "decision-number",
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
    game.activate_internal_card(
        "source",
        program.card().abilities.first().unwrap().node.id,
        PlayerId { seat },
    )
    .unwrap();
    game.advance_player_input();
    game
}

fn assert_unchanged(game: &Game, before: &str) {
    assert!(
        board_fixtures::exact_authority(game) == before,
        "reading or rejecting a numeric answer preserves authority state"
    );
}

#[test]
fn real_x_bounds_pay_the_declared_value_and_keep_rejections_read_only() {
    let document = payment_fixtures::x_document(
        payment_expanded_tree(),
        "activate",
        json!({"x":{},"cost":{"pp":"x"},"do":[{"life":{"who":"self","delta":"x"}}]}),
    )
    .unwrap();
    let program = ExpandedLowerer::new()
        .unwrap()
        .lower(&document)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for actor in [Viewer::P1, Viewer::P2] {
        let other = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        for (value, pp, life) in [(0_u32, 3_i32, 20_i32), (3_u32, 0_i32, 23_i32)] {
            let mut game = board(actor, &program);
            let native = question(&game, actor).unwrap();
            assert_eq!(
                domain(&native),
                Some(&DecisionShape::Integer {
                    bounds: Cardinality {
                        minimum: Nat(0),
                        maximum: Some(Nat(3))
                    },
                    source: player::IntegerSource::XCost,
                })
            );
            assert_eq!(native.purpose, player::QuestionPurpose::PlayParameter);
            let before = board_fixtures::exact_authority(&game);
            let (usage, _, binding) = project(domain(&native).unwrap(), None).unwrap();
            assert_eq!(usage, wire::QuestionUse::DeclareX);
            game.observe(actor).unwrap();
            game.observe(other).unwrap();
            assert!(
                matches!(game.input(other), Input::Waiting(waiting) if waiting.actor == actor && waiting.dice.is_none())
            );
            assert!(matches!(
                game.battle_question(
                    actor,
                    &QuestionId::new("x".into()),
                    &board_fixture::fixture().unwrap().references
                ),
                Err(ProjectionError::Unavailable)
            ));
            assert_eq!(
                binding.answer(
                    native.token,
                    &Answer::Integer {
                        value: WireNat::from(4_u32)
                    }
                ),
                Err(AnswerError)
            );
            let command = binding
                .answer(
                    native.token,
                    &Answer::Integer {
                        value: WireNat::from(value),
                    },
                )
                .unwrap();
            assert_eq!(
                game.submit_player(other, command.clone()).kind,
                SubmissionKind::Rejected
            );
            assert_unchanged(&game, &before);
            assert_eq!(
                game.submit_player(actor, command.clone()).kind,
                SubmissionKind::Applied
            );
            let state = game.projection(View::Referee).unwrap();
            let own = if actor == Viewer::P1 { "P1" } else { "P2" };
            assert_eq!(state[own]["pp"]["current"], pp);
            assert_eq!(state[own]["leader"]["life"], life);
            let after = board_fixtures::exact_authority(&game);
            assert_eq!(
                game.submit_player(actor, command).kind,
                SubmissionKind::Rejected
            );
            assert_unchanged(&game, &after);
        }
    }
}

#[test]
fn real_dice_commands_keep_public_waiting_values_and_native_reroll_semantics() {
    let program = history_fixture::card(vec![
        history_fixture::ability(1, vec![history_fixture::roll(1)]).unwrap(),
        history_fixture::reroll().unwrap(),
    ])
    .unwrap();
    let references = &board_fixture::fixture().unwrap().references;
    for actor in [Viewer::P1, Viewer::P2] {
        let other = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        for choice in [wire::DiceChoice::Keep, wire::DiceChoice::Reroll] {
            let mut game = board(actor, &program);
            let native = question(&game, actor).unwrap();
            let (value, remaining) = dice(&native).unwrap();
            assert_eq!(remaining, Nat(1));
            assert_eq!(native.purpose, player::QuestionPurpose::ResolutionChoice);
            let before = board_fixtures::exact_authority(&game);
            let (usage, shape, binding) = project(domain(&native).unwrap(), None).unwrap();
            assert_eq!(usage, wire::QuestionUse::DiceReroll);
            assert_eq!(
                shape,
                wire::QuestionShape::DiceReroll {
                    value: WireNat::try_from(value.0).unwrap(),
                    remaining: WireNat::from(1_u32),
                    allowed: vec![wire::DiceChoice::Keep, wire::DiceChoice::Reroll],
                }
            );
            assert_eq!(
                game.observe(actor).unwrap().history.dice,
                Vec::<DiceHistory>::new()
            );
            assert_eq!(
                game.observe(other).unwrap().history.dice,
                Vec::<DiceHistory>::new()
            );
            assert!(matches!(game.input(other), Input::Waiting(waiting)
                if waiting.actor == actor && waiting.dice == Some((value, remaining))));
            assert!(
                game.battle_question(other, &QuestionId::new("dice".into()), references)
                    .unwrap()
                    .is_none()
            );
            assert!(matches!(
                game.battle_question(actor, &QuestionId::new("dice".into()), references),
                Err(ProjectionError::Unavailable)
            ));
            assert_eq!(
                binding.answer(
                    native.token,
                    &Answer::Integer {
                        value: WireNat::ZERO
                    }
                ),
                Err(AnswerError)
            );
            let command = binding
                .answer(native.token, &Answer::DiceReroll { choice })
                .unwrap();
            assert_eq!(
                game.submit_player(other, command.clone()).kind,
                SubmissionKind::Rejected
            );
            assert_unchanged(&game, &before);
            assert_eq!(
                game.submit_player(actor, command.clone()).kind,
                SubmissionKind::Applied
            );
            assert_final_die(&game, actor, choice, value);
            let after = board_fixtures::exact_authority(&game);
            assert_eq!(
                game.submit_player(actor, command).kind,
                SubmissionKind::Rejected
            );
            assert_unchanged(&game, &after);
        }
    }
}

fn assert_final_die(game: &Game, actor: Viewer, choice: wire::DiceChoice, value: Nat) {
    for viewer in [Viewer::P1, Viewer::P2] {
        let history = game.observe(viewer).unwrap().history;
        assert_eq!(history.dice.len(), 1);
        let fact = history.dice.first().unwrap();
        assert_eq!(fact.player, actor);
        assert!((1_u64..=6).contains(&fact.value));
        if choice == wire::DiceChoice::Keep {
            assert_eq!(fact.value, value.0);
        }
    }
}
