use super::*;

fn choice(player: &str, binding: &str) -> Value {
    json!({"choose":{"bind":binding,"from":{"who":player,"zone":"field","kind":"follower"},"n":1,"by":player}})
}

fn procedure(player: &str, stages: &[Vec<Value>]) -> Value {
    json!({"each_player":{"as":player,"stages":stages}})
}

#[test]
fn stage_cursor_tracks_the_real_actor_and_ends_before_an_unrelated_question() {
    let steps = json!([
        procedure(
            "p",
            &[vec![choice("p", "first")], vec![choice("p", "second")]]
        ),
        json!({"optional":{"do":[{"life":{"who":"self","delta":1}}]}}),
    ]);
    for first in [1, 2] {
        let other = 3 - first;
        let (mut game, mut question) = play_as(first, &steps, (1, 1), (json!([]), json!([])));
        for seat in [first, other, first, other] {
            assert_eq!(game.internal_each_player_actor(), Some(PlayerId { seat }));
            let current = question.unwrap();
            assert_eq!(current.actor, PlayerId { seat });
            let before = exact_authority(&game);
            game.answer_internal_card(
                PlayerId { seat: 3 - seat },
                current.token,
                Answer::Targets(vec![handles(&current)[0]]),
            )
            .unwrap_err();
            assert_eq!(exact_authority(&game), before);
            assert_eq!(game.internal_each_player_actor(), Some(PlayerId { seat }));
            question = answer(&mut game, &current, vec![handles(&current)[0]]);
        }
        assert_eq!(game.internal_each_player_actor(), None);
        let tail = question.unwrap();
        assert_eq!(tail.actor, PlayerId { seat: first });
        resolved(
            game.answer_internal_card(tail.actor, tail.token, Answer::Integer(Nat(0)))
                .unwrap(),
        );
        assert_eq!(game.internal_each_player_actor(), None);
    }
}

#[test]
fn nested_procedure_end_restores_the_outer_actor() {
    let steps = json!([procedure(
        "outer",
        &[vec![
            choice("outer", "before"),
            procedure("inner", &[vec![choice("inner", "inside")]]),
            choice("outer", "after"),
        ]]
    )]);
    for first in [1, 2] {
        let other = 3 - first;
        let (mut game, mut question) = play_as(first, &steps, (1, 1), (json!([]), json!([])));
        for seat in [first, first, other, first, other, first, other, other] {
            assert_eq!(game.internal_each_player_actor(), Some(PlayerId { seat }));
            let current = question.unwrap();
            assert_eq!(current.actor, PlayerId { seat });
            question = answer(&mut game, &current, vec![handles(&current)[0]]);
        }
        assert!(question.is_none());
        assert_eq!(game.internal_each_player_actor(), None);
    }
}

#[test]
fn termination_discards_the_procedure_cursor_with_its_continuation() {
    let steps = json!([procedure(
        "p",
        &[vec![choice("p", "before"), choice("p", "after")]]
    )]);
    for first in [1, 2] {
        let (mut game, question) = play_as(first, &steps, (1, 1), (json!([]), json!([])));
        assert_eq!(
            game.internal_each_player_actor(),
            Some(PlayerId { seat: first })
        );
        let current = question.unwrap();
        let result = game.concede_internal(PlayerId { seat: first }).unwrap();
        assert!(!result.progress.unwrap().complete);
        assert_eq!(game.internal_each_player_actor(), None);
        assert!(game.internal_decision(View::Referee).is_none());
        let before = exact_authority(&game);
        game.answer_internal_card(
            current.actor,
            current.token,
            Answer::Targets(vec![handles(&current)[0]]),
        )
        .unwrap_err();
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn procedure_actor_is_independent_of_the_question_actor() {
    let mut pick = choice("p", "picked");
    pick["choose"]["by"] = json!("self");
    let steps = json!([procedure("p", &[vec![pick]])]);
    for controller in [1, 2] {
        let (mut game, mut question) = play_as(controller, &steps, (1, 1), (json!([]), json!([])));
        for seat in [controller, 3 - controller] {
            assert_eq!(game.internal_each_player_actor(), Some(PlayerId { seat }));
            let current = question.unwrap();
            assert_eq!(current.actor, PlayerId { seat: controller });
            question = answer(&mut game, &current, vec![handles(&current)[0]]);
        }
        assert!(question.is_none());
        assert_eq!(game.internal_each_player_actor(), None);
    }
}
