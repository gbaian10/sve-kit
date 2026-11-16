use super::*;
use crate::battle::tests::board_fixtures;
use crate::game::player::observation::{ObservedZone, QuoteBasis};
use crate::ir::Nat;
use sve_battle_contract::OptionId;

fn board(actor: Viewer, exchange: bool) -> Game {
    if exchange {
        exchange::board(actor, false, "unit-spell")
    } else {
        searched(
            actor,
            &["unit-follower", "unit-spell"],
            "unit-amulet",
            vec![0],
            2,
        )
        .0
    }
}

fn reject_invalid(binding: &Binding, valid: &OptionId) {
    for answer in [
        Answer::Confirm { accept: true },
        Answer::Choose { chosen: vec![] },
        Answer::Choose {
            chosen: vec![valid.clone(); 2],
        },
        Answer::Choose {
            chosen: vec![OptionId::new("foreign-route".into())],
        },
    ] {
        assert_eq!(binding.decode(&answer), Err(AnswerError));
    }
}

#[test]
fn routes_reject_invalid_answers_wrong_seats_and_stale_tokens_without_mutation() {
    for actor in [Viewer::P1, Viewer::P2] {
        let other = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        for exchange in [false, true] {
            let mut game = board(actor, exchange);
            let native = question(&game, actor).unwrap();
            let id = QuestionId::new("invalid-route".into());
            let (_, shape, binding) =
                project_question(&native, &components(&game, actor), &id).unwrap();
            let wire::QuestionShape::Choose {
                options,
                bounds,
                count,
                ..
            } = shape
            else {
                panic!("single route")
            };
            assert_eq!((bounds.min.get(), bounds.max.unwrap().get()), (1, 1));
            assert!(count.is_none());
            reject_invalid(&binding, &options[0].id);
            let answer = Answer::Choose {
                chosen: vec![options[0].id.clone()],
            };
            let command = binding.answer(native.token, &answer).unwrap();
            let before = board_fixtures::exact_authority(&game);
            assert!(project_question(&native, &components(&game, other), &id).is_err());
            assert_eq!(
                game.submit_player(other, command.clone()).kind,
                SubmissionKind::Rejected
            );
            assert_eq!(
                game.submit_player(
                    actor,
                    player::Command::Answer {
                        token: native.token,
                        answer: player::Answer::Integer(Nat(0))
                    }
                )
                .kind,
                SubmissionKind::Rejected
            );
            assert!(
                board_fixtures::exact_authority(&game) == before,
                "invalid route answers preserve authority"
            );
            assert_eq!(
                game.submit_player(actor, command.clone()).kind,
                SubmissionKind::Applied
            );
            let after = board_fixtures::exact_authority(&game);
            assert_eq!(
                game.submit_player(actor, command).kind,
                SubmissionKind::Rejected
            );
            assert!(
                board_fixtures::exact_authority(&game) == after,
                "stale route answers preserve authority"
            );
        }
    }
}

#[test]
fn incomplete_or_inconsistent_frozen_route_facts_close_the_whole_projection() {
    let id = QuestionId::new("missing-route-facts".into());
    for actor in [Viewer::P1, Viewer::P2] {
        for exchange in [false, true] {
            let game = board(actor, exchange);
            let native = question(&game, actor).unwrap();
            let observation = components(&game, actor);
            let before = board_fixtures::exact_authority(&game);
            for case in 0_u8..3 {
                let mut invalid = native.clone();
                let player::QuestionShape::Decision(shape) = &mut invalid.shape else {
                    panic!("route domain")
                };
                corrupt(shape, case);
                assert!(project_question(&invalid, &observation, &id).is_err());
            }
            assert!(
                board_fixtures::exact_authority(&game) == before,
                "missing route facts do not modify authority"
            );
        }
    }
}

fn corrupt(shape: &mut DecisionShape, case: u8) {
    if let DecisionShape::ExchangeRoutes(choices) = shape {
        match case {
            0 => choices[0].cost.basis = QuoteBasis::CurrentContext,
            1 => choices.push(choices[0].clone()),
            _ => choices[0].index = Nat(u64::MAX),
        }
    } else {
        let DecisionShape::SearchReplacement(destinations) = shape else {
            panic!("route domain")
        };
        match case {
            0 => destinations.original = None,
            1 => destinations.replacement = None,
            _ => destinations.original.as_mut().unwrap().zone = ObservedZone::Deck,
        }
    }
}
