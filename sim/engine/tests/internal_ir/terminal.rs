use super::*;
use sve_battle_contract::{BattleInput, QuestionId, Seat, event::ResultReason};
use sve_engine::game::player::Viewer;

#[test]
fn effect_victory_preserves_the_prefix_without_retaining_a_resumable_execution() {
    for seat in [1, 2] {
        let deck = (0..4)
            .map(|index| json!({"id":format!("d{index}"),"card":"unit-follower"}))
            .collect::<Vec<_>>();
        let mut game = Game::new(
            base_catalog().unwrap(),
            &setup(seat, json!(deck)),
            &Value::Null,
            &Value::Null,
            "terminal",
        )
        .unwrap();
        let win = instruction(
            2,
            Action::GameResult {
                players: PlayerSet {
                    relation: PlayerRelation::SelfPlayer,
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(1),
                        maximum: Some(Nat(1)),
                    },
                    text_quantifier: TextQuantifier::Implicit,
                },
                result: Outcome::Win,
                source: AbilitySource {
                    holder: ObjectOperand::SourceHolder {},
                    origin: AbilityOrigin::Printed {
                        face: FaceId("0".into()),
                        line: Nat(1),
                    },
                    controller: PlayerRef::Controller {},
                },
            },
        );
        let program = checked(vec![draw(1, 1), win, draw(3, 1)]);
        let later = game
            .prepare_internal_play(
                &checked(vec![up_to(4, 1)]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
        let prepared = game
            .prepare_internal_play(&program, &source("synthetic"), context(seat))
            .unwrap();
        let progress = game.execute_internal_play(&prepared).unwrap();
        assert!(!progress.complete);
        assert_eq!(
            progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::Draw(_)))
                .count(),
            1
        );
        assert!(game.internal_decision(View::Referee).is_none());
        assert_eq!(
            game.projection(View::Referee).unwrap()["game"]["winner"],
            if seat == 1 { "P1" } else { "P2" }
        );
        let input = game
            .battle_input(
                Viewer::P1,
                &QuestionId::new("effect-end".into()),
                &base_references().unwrap(),
            )
            .unwrap();
        assert_eq!(
            input.input(),
            &BattleInput::Finished {
                winner: Some(if seat == 1 { Seat::P1 } else { Seat::P2 }),
                reason: ResultReason::Effect,
            }
        );
        let before = exact_authority(&game);
        game.execute_internal_play(&later).unwrap_err();
        game.execute_internal_play(&prepared).unwrap_err();
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn surrender_during_prepared_body_keeps_draw_prefix_and_invalidates_every_handle() {
    for seat in [1, 2] {
        let mut game = game(seat, 4);
        let program = checked(vec![
            draw(1, 1),
            up_to(2, 2),
            damage(
                3,
                RecipientSet::Leaders {
                    players: players(PlayerRelation::Opponents),
                },
                9,
            ),
        ]);
        let prepared = game
            .prepare_internal_play(&program, &source("synthetic"), context(seat))
            .unwrap();
        let first = game.execute_internal_play(&prepared).unwrap();
        assert!(!first.complete);
        let question = game.internal_decision(View::Referee).unwrap();
        game.answer_internal(
            question.actor,
            question.token,
            Answer::Stepwise(StepwiseAnswer::Continue),
        )
        .unwrap();
        let current = game.internal_decision(View::Referee).unwrap();
        let before = exact_authority(&game);
        game.concede_internal(PlayerId { seat: 0 }).unwrap_err();
        assert_eq!(before, exact_authority(&game));
        let result = game.concede_internal(PlayerId { seat: 3 - seat }).unwrap();
        assert_eq!(result.execution, Some(current.token.execution));
        assert_eq!(result.step.outcome, "game-end");
        let progress = result.progress.unwrap();
        assert!(!progress.complete);
        assert_eq!(
            progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::Draw(_)))
                .count(),
            2
        );
        assert!(!progress.facts.iter().any(|fact| matches!(
            fact,
            Fact::Damage(_) | Fact::AbilityResolved { .. } | Fact::CardResolved { .. }
        )));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view[if seat == 1 { "P1" } else { "P2" }]["hand"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert_eq!(view["P1"]["leader"]["life"], 20);
        assert_eq!(view["P2"]["leader"]["life"], 20);
        assert!(game.internal_decision(View::Referee).is_none());
        let ended = exact_authority(&game);
        game.answer_internal(
            current.actor,
            current.token,
            Answer::Stepwise(StepwiseAnswer::Continue),
        )
        .unwrap_err();
        game.execute_internal_play(&prepared).unwrap_err();
        game.concede_internal(PlayerId { seat }).unwrap_err();
        assert_eq!(ended, exact_authority(&game));
    }
}

#[test]
fn simultaneous_defeat_finishes_as_a_draw_and_invalidates_prepared_handles() {
    let mut game = Game::new(
        base_catalog().unwrap(),
        &setup(1, json!([{"id":"d0","card":"unit-follower"}])),
        &Value::Null,
        &Value::Null,
        "terminal",
    )
    .unwrap();
    let later = checked(vec![up_to(5, 1)]);
    let handle = game
        .prepare_internal_play(&later, &source("synthetic"), context(1))
        .unwrap();
    let fatal = checked(vec![damage(
        6,
        RecipientSet::Leaders {
            players: players(PlayerRelation::All),
        },
        20,
    )]);
    let fatal = game
        .prepare_internal_play(&fatal, &source("synthetic"), context(1))
        .unwrap();
    let complete = game.execute_internal_play(&fatal).unwrap();
    assert!(complete.complete);
    // Both leaders reach zero together, so the checks path ends the game as a draw, not a concession.
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["game"]["ended"], true);
    assert_eq!(view["game"]["by"], "1.2.2");
    assert!(view["game"]["winner"].is_null());
    let input = game
        .battle_input(
            Viewer::P1,
            &QuestionId::new("draw-end".into()),
            &base_references().unwrap(),
        )
        .unwrap();
    assert_eq!(
        input.input(),
        &BattleInput::Finished {
            winner: None,
            reason: ResultReason::SimultaneousDefeat,
        }
    );
    let ended = exact_authority(&game);
    game.execute_internal_play(&handle).unwrap_err();
    assert_eq!(ended, exact_authority(&game));
}
