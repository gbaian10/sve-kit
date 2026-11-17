use super::*;
use sve_engine::game::internal::{CostPaymentPurpose, PpPayment};

#[test]
fn pp_debits_survive_recovery_without_turning_quotes_into_payments() {
    for cost in [1_i64, 3] {
        let program = program(
            "activate",
            json!({"cost":{"pp":cost},
                "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
                "do":[{"pp":{"who":"self","delta":cost}},
                    {"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]
            }),
        );
        for actor in [Viewer::P1, Viewer::P2] {
            let me = if actor == Viewer::P1 { "P1" } else { "P2" };
            let mut game = fixture(&program, actor);
            assert_eq!(game.internal_pp_payments(), None);
            game.activate_internal_card(
                "source",
                program.card().abilities[0].node.id,
                actor.player(),
            )
            .unwrap();
            assert!(
                game.internal_pp_payments().unwrap().is_empty(),
                "pricing has not debited PP"
            );
            let before = exact_authority(&game);
            for viewer in [Viewer::P1, Viewer::P2] {
                game.observe(viewer).unwrap();
            }
            assert!(
                exact_authority(&game) == before,
                "observation and affordability probes are pure"
            );
            let q = question(&game, actor);
            assert_eq!(
                submit(&mut game, actor, &q, Answer::Targets(vec![0])),
                SubmissionKind::Applied
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()[me]["pp"]["current"],
                3
            );
            payments(
                &game,
                &[PpPayment {
                    player: actor.player(),
                    amount: Int(cost),
                }],
            );
            let before = exact_authority(&game);
            for viewer in [Viewer::P1, Viewer::P2] {
                game.observe(viewer).unwrap();
            }
            assert!(
                exact_authority(&game) == before,
                "later quotes do not repeat an already paid cost"
            );
            closed_wire(&game, actor);
            let may = question(&game, actor);
            assert_eq!(
                submit(&mut game, actor, &may, Answer::Integer(Nat(0))),
                SubmissionKind::Applied
            );
            assert_eq!(game.internal_pp_payments(), None);
        }
    }
}

fn payments(game: &Game, expected: &[PpPayment]) {
    let before = exact_authority(game);
    assert_eq!(
        game.internal_pp_payments().unwrap(),
        expected,
        "only this execution's real PP debits are retained"
    );
    assert_eq!(
        game.internal_pp_payments().unwrap(),
        expected,
        "repeated payment reads return the same values"
    );
    assert!(
        exact_authority(game) == before,
        "payment diagnostics do not change authority"
    );
}

#[test]
fn resolution_payment_records_its_actual_payer_instead_of_the_controller() {
    for foreign in [false, true] {
        let program = program(
            "auto",
            json!({"on":{"phase":{"whose":"self","which":"end"}},"do":[
                {"pay":{"who":if foreign {"opp"} else {"self"},"cost":{"pp":1},
                    "do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]}}
            ]}),
        );
        for controller in [Viewer::P1, Viewer::P2] {
            let payer = if foreign {
                if controller == Viewer::P1 {
                    Viewer::P2
                } else {
                    Viewer::P1
                }
            } else {
                controller
            };
            let me = if payer == Viewer::P1 { "P1" } else { "P2" };
            let mut game = fixture(&program, controller);
            end_trigger(&mut game, controller);
            let q = question(&game, payer);
            payments(&game, &[]);
            let before = game.projection(View::Referee).unwrap()[me]["pp"]["current"]
                .as_i64()
                .unwrap();
            assert_eq!(
                submit(&mut game, payer, &q, Answer::OptionalCost(true)),
                SubmissionKind::Applied
            );
            payments(
                &game,
                &[PpPayment {
                    player: payer.player(),
                    amount: Int(1),
                }],
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()[me]["pp"]["current"],
                before - 1
            );
            assert!(
                game.internal_play_declarations().unwrap().is_empty(),
                "a real debit is not a play-time cost declaration"
            );
            closed_wire(&game, controller);
            let may = question(&game, controller);
            assert_eq!(
                submit(&mut game, controller, &may, Answer::Integer(Nat(0))),
                SubmissionKind::Applied
            );
            assert_eq!(game.internal_pp_payments(), None);
        }
    }
}

#[test]
fn declined_or_failed_resolution_payments_keep_only_prior_successful_debits() {
    let program = program(
        "auto",
        json!({"on":{"phase":{"whose":"self","which":"end"}},"do":[
            {"pay":{"cost":{"pp":1},"do":[{"life":{"who":"self","delta":1}}]}},
            {"pay":{"cost":{"pp":3},"do":[{"life":{"who":"self","delta":100}}]}},
            {"optional":{"do":[{"life":{"who":"self","delta":1}}]}}
        ]}),
    );
    for actor in [Viewer::P1, Viewer::P2] {
        for accept_second in [false, true] {
            let me = if actor == Viewer::P1 { "P1" } else { "P2" };
            let mut game = fixture(&program, actor);
            end_trigger(&mut game, actor);
            let first = question(&game, actor);
            assert_eq!(
                submit(&mut game, actor, &first, Answer::OptionalCost(true)),
                SubmissionKind::Applied
            );
            let expected = [PpPayment {
                player: actor.player(),
                amount: Int(1),
            }];
            payments(&game, &expected);
            let completed = game.internal_cost_spec_payments().unwrap().to_vec();
            let resolution = completed
                .iter()
                .filter(|cost| matches!(cost.purpose, CostPaymentPurpose::Resolution { .. }))
                .collect::<Vec<_>>();
            assert_eq!(resolution.len(), 1);
            assert_eq!(resolution[0].steps.len(), 1);
            assert_eq!(resolution[0].steps[0].actor, actor.player());
            assert_eq!(
                resolution[0].steps[0].quantity,
                Quantity {
                    amount: Int(1),
                    unit: QuantityUnit::Pp
                }
            );
            let second = question(&game, actor);
            assert_eq!(
                submit(
                    &mut game,
                    actor,
                    &second,
                    Answer::OptionalCost(accept_second)
                ),
                SubmissionKind::Applied
            );
            payments(&game, &expected);
            assert_eq!(
                game.internal_cost_spec_payments().unwrap(),
                completed.as_slice()
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state[me]["pp"]["current"], 2);
            assert_eq!(state[me]["leader"]["life"], 21);
            let may = question(&game, actor);
            assert_eq!(
                submit(&mut game, actor, &may, Answer::Integer(Nat(0))),
                SubmissionKind::Applied
            );
            assert_eq!(game.internal_pp_payments(), None);
        }
    }
}
