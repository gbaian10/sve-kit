use super::*;
use sve_engine::game::internal::{PlayPhase, QuestionStage};

#[test]
fn nested_completion_or_cancellation_restores_the_suspended_parents_phase() {
    for child_fails in [false, true] {
        let (mut game, definition) = build(
            "self",
            "unit-spell",
            true,
            Scenario::ProcessingPause { child_fails },
        );
        let outer = question(
            game.activate_internal_card("parent", definition, PlayerId { seat: 1 })
                .unwrap(),
        );
        assert_eq!(game.internal_play_phase(), Some(PlayPhase::Declare));
        let play = select(&mut game, &outer);
        let parent = if child_fails {
            play
        } else {
            let child = question(play);
            assert_ne!(child.token.execution, outer.token.execution);
            assert_eq!(game.internal_play_phase(), Some(PlayPhase::Declare));
            let before = exact_authority(&game);
            game.answer_internal_card(child.actor, child.token, Answer::Integer(Nat(0)))
                .unwrap_err();
            assert!(
                exact_authority(&game) == before,
                "rejection preserves the child and suspended parent"
            );
            select(&mut game, &child)
        };
        let CardPlay::Awaiting(parent) = parent else {
            panic!("the parent must reach its own optional body");
        };
        assert_eq!(parent.token.execution, outer.token.execution);
        assert_eq!(game.internal_play_phase(), Some(PlayPhase::Resolve));
        assert_eq!(game.internal_question_stage(), Some(QuestionStage::Resolve));
        let result = resolved(
            game.answer_internal_card(parent.actor, parent.token, Answer::Integer(Nat(0)))
                .unwrap(),
        );
        assert_eq!(game.internal_play_phase(), None);
        assert_eq!(result.execution, outer.token.execution);
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::CardPlayed { .. }))
                .count(),
            usize::from(!child_fails)
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 2);
        assert_eq!(
            state["P1"]["leader"]["life"],
            if child_fails { 31 } else { 33 }
        );
    }
}
