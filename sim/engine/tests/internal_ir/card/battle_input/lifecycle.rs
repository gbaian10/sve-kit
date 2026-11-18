use super::*;
use sve_battle_contract::event::ResultReason;

#[test]
fn wire_input_waiting_retains_the_public_die_without_a_private_procedure() {
    let program = card(vec![
        history_fixture::ability(1, vec![history_fixture::roll(1)]).unwrap(),
        history_fixture::reroll().unwrap(),
    ]);
    for actor in [Viewer::P1, Viewer::P2] {
        let mut views = Vec::new();
        for secret in ["unit-spell", "unit-amulet"] {
            let mut initial = board_fixtures::setup(actor.player().seat, json!([]));
            let own = if actor == Viewer::P1 { "P1" } else { "P2" };
            initial["turn"]["active"] = json!(own);
            initial["players"][own]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
            initial["players"][own]["zones"]["hand"] = json!([{"id":"private","card":secret}]);
            let mut game = board(&initial, &program, &card(vec![]));
            game.activate_internal_card("s", AbilityId(1), actor.player())
                .unwrap();
            let input = project(&game, actor);
            let wire::QuestionShape::DiceReroll {
                value, remaining, ..
            } = &question(&input).shape
            else {
                panic!("die choice")
            };
            let waiting = project(&game, opposite(actor));
            let BattleInput::Waiting { context, .. } = waiting.input() else {
                panic!("waiting")
            };
            let die = context.dice.unwrap();
            assert_eq!((die.value, die.remaining), (*value, *remaining));
            assert!(
                context
                    .processing
                    .as_ref()
                    .is_none_or(|processing| processing.procedure.is_none())
            );
            submit(
                &mut game,
                actor,
                &input,
                WireAnswer::DiceReroll {
                    choice: wire::DiceChoice::Keep,
                },
            );
            assert_eq!(
                question(&project(&game, actor)).usage,
                wire::QuestionUse::MainAction
            );
            views.push(waiting);
        }
        assert_eq!(views[0].observation(), views[1].observation());
        assert_eq!(views[0].input(), views[1].input());
    }
}

#[test]
fn wire_input_concession_from_waiting_finishes_both_seats_and_disables_commands() {
    let mut game = board_fixture::empty_board(Viewer::P1, "unit-spell").unwrap();
    game.advance_player_input();
    let active = project(&game, Viewer::P1);
    let waiting = project(&game, Viewer::P2);
    let command = waiting.command(Viewer::P2, &WireCommand::Concede).unwrap();
    assert_eq!(
        game.submit_player(Viewer::P2, command).kind,
        SubmissionKind::Applied
    );
    for viewer in [Viewer::P1, Viewer::P2] {
        let finished = project(&game, viewer);
        assert_eq!(
            finished.input(),
            &BattleInput::Finished {
                winner: Some(Seat::P1),
                reason: ResultReason::Concession
            }
        );
        assert_eq!(
            finished.command(viewer, &WireCommand::Concede),
            Err(AnswerError)
        );
        assert_eq!(
            finished.answer(
                viewer,
                &question(&active).id,
                &WireAnswer::Confirm { accept: true }
            ),
            Err(AnswerError)
        );
    }
}

#[test]
fn wire_input_fault_has_no_winner_or_answer_and_only_advertises_native_concession() {
    let program = checked(vec![up_to(1, 1), draw(2, 10_001)]);
    for actor in [Viewer::P1, Viewer::P2] {
        let mut views = Vec::new();
        for secret in ["unit-spell", "unit-amulet"] {
            let mut game = board_fixture::empty_board(actor, secret).unwrap();
            game.execute_internal(&program, &source("synthetic"), context(actor.player().seat))
                .unwrap();
            let input = project(&game, actor);
            let wire::QuestionShape::Choose { options, .. } = &question(&input).shape else {
                panic!("stepwise draw")
            };
            let stop = options
                .iter()
                .find(|option| {
                    option.value
                        == wire::ChoiceValue::Stepwise {
                            step: wire::StepwiseChoice::Stop,
                        }
                })
                .unwrap();
            let command = input
                .answer(
                    actor,
                    &question(&input).id,
                    &WireAnswer::Choose {
                        chosen: vec![stop.id.clone()],
                    },
                )
                .unwrap();
            assert_eq!(
                game.submit_player(actor, command).kind,
                SubmissionKind::EngineFault
            );
            let fault = project(&game, actor);
            assert_eq!(
                fault.input(),
                &BattleInput::EngineFault { can_concede: true }
            );
            assert_eq!(
                fault.answer(
                    actor,
                    &question(&input).id,
                    &WireAnswer::Confirm { accept: true }
                ),
                Err(AnswerError)
            );
            let command = fault.command(actor, &WireCommand::Concede).unwrap();
            assert_eq!(
                game.submit_player(actor, command).kind,
                SubmissionKind::Applied
            );
            assert_eq!(
                project(&game, actor).input(),
                &BattleInput::Finished {
                    winner: Some(seat(opposite(actor))),
                    reason: ResultReason::Concession
                }
            );
            views.push(fault);
        }
        assert_eq!(views[0].observation(), views[1].observation());
        assert_eq!(views[0].input(), views[1].input());
    }
}
