use super::hand_exchange_declarations::{apply, seat, spell_game, spell_program};
use super::*;
use sve_engine::game::internal::{CostPaymentPurpose, ExchangePayment, PlayDeclaration, PpPayment};

fn records(game: &Game) -> Vec<ExchangePayment> {
    let before = exact_authority(game);
    let saved = game.internal_exchange_payments().unwrap().to_vec();
    for viewer in [Viewer::P1, Viewer::P2] {
        game.observe(viewer).unwrap();
    }
    assert_eq!(
        game.internal_exchange_payments().unwrap(),
        saved,
        "observations preserve completed exchange payments"
    );
    assert!(
        exact_authority(game) == before,
        "reading completed exchange payments is pure"
    );
    saved
}

fn act_program(delta: i64) -> WellFormedCardProgram {
    spell_program(
        json!({"optional_cost":{"act":{"sel":{"who":"self","zone":"field"},"n":1},"result":"exchange"},"optional_cost_delta":delta}),
    )
}

fn definition(program: &WellFormedCardProgram) -> &PlayCostExchange {
    &program.card().card_play_costs[0]
        .node
        .exchange
        .as_ref()
        .unwrap()
        .node
}

fn material_instruction(program: &WellFormedCardProgram) -> &Instruction {
    let steps = if let CostSpec::Ordered { steps } = &definition(program).cost {
        Some(steps)
    } else {
        None
    };
    assert!(
        steps.is_some(),
        "the fixture has one ordered material route"
    );
    &steps.unwrap()[0].instruction
}

fn assert_pp(game: &Game, paid: &ExchangePayment, amount: i64, actor: Viewer) {
    assert_eq!(
        paid.player,
        actor.player(),
        "the exchange freezes its actual payer"
    );
    assert_eq!(
        paid.pp,
        Quantity {
            amount: Int(amount),
            unit: QuantityUnit::Pp
        },
        "the payment uses the writer's adjusted debit"
    );
    assert_eq!(
        game.internal_pp_payments().unwrap(),
        &[PpPayment {
            player: actor.player(),
            amount: Int(amount)
        }],
        "the exchange record describes the same debit, not an additional PP payment"
    );
}

fn assert_material(game: &Game, program: &WellFormedCardProgram, paid: &ExchangePayment) {
    if let Some(material) = &paid.material {
        assert!(
            paid.accepted,
            "only accepted material exchanges have a material payment"
        );
        assert_eq!(
            *material.instruction,
            *material_instruction(program),
            "the original admitted material instruction is frozen"
        );
        let declared = game
            .internal_play_declarations()
            .unwrap()
            .iter()
            .find_map(|item| {
                if let PlayDeclaration::ExchangeMaterials { objects, .. } = item {
                    Some(objects)
                } else {
                    None
                }
            })
            .unwrap();
        assert!(
            matches!(&material.outcome,InstructionOutcome::Payment {subjects,quantity,..}
            if subjects==declared && *quantity==Quantity {amount:Int(1),unit:QuantityUnit::Cards}),
            "the writer paid the declared physical generation with a one-card quantity"
        );
    } else {
        assert!(
            !paid.accepted,
            "accepted posture exchanges must retain their material writer"
        );
    }
}

#[test]
fn a_paid_exchange_has_an_actual_payment_record_separate_from_its_declaration() {
    let program = act_program(-2);
    for actor in [Viewer::P1, Viewer::P2] {
        for accepted in [false, true] {
            let mut game = spell_game(&program, actor);
            assert!(
                records(&game).is_empty(),
                "only a completed exchange creates an exchange payment"
            );
            let q = question(&game, actor);
            let before = exact_authority(&game);
            let other = if actor == Viewer::P1 {
                Viewer::P2
            } else {
                Viewer::P1
            };
            assert_eq!(
                submit(&mut game, other, &q, Answer::OptionalCost(accepted)),
                SubmissionKind::Rejected
            );
            assert!(
                exact_authority(&game) == before,
                "a rejected answer preserves every authority field"
            );
            apply(&mut game, actor, Answer::OptionalCost(accepted));
            if accepted {
                assert!(
                    records(&game).is_empty(),
                    "acceptance has not paid the exchange"
                );
                assert_eq!(
                    game.projection(View::Referee).unwrap()[seat(actor)]["pp"]["current"],
                    4
                );
                let q = question(&game, actor);
                let before = exact_authority(&game);
                assert_eq!(
                    submit(&mut game, actor, &q, Answer::CostMaterials(vec![99])),
                    SubmissionKind::Rejected
                );
                assert!(
                    exact_authority(&game) == before,
                    "a rejected answer preserves every authority field"
                );
                apply(&mut game, actor, Answer::CostMaterials(vec![0]));
            }
            let saved = records(&game);
            assert_eq!(
                saved.len(),
                1,
                "the successful exchange path survives independently of CostSpec receipts"
            );
            let paid = &saved[0];
            assert_eq!(paid.receipt, definition(&program).receipt);
            assert_eq!(paid.accepted, accepted);
            assert_pp(&game, paid, if accepted { 2 } else { 4 }, actor);
            assert!(!game.internal_cost_spec_payments().unwrap().iter().any(|cost|
                matches!(cost.purpose,CostPaymentPurpose::Optional {receipt,..} if receipt==paid.receipt)),
                "an exchange does not masquerade as an optional CostSpec payment");
            assert_material(&game, &program, paid);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["m0"]["acted"], accepted);
            let progress = finish_may(&mut game);
            assert!(
                matches!(progress.receipts.get(&paid.receipt),Some(InstructionOutcome::ExchangePayment(result)) if result.complete==Bool(accepted))
            );
            if let Some(material) = &paid.material {
                assert_eq!(
                    progress.receipts.get(&material.instruction.receipt),
                    Some(&material.outcome)
                );
            }
            assert_eq!(game.internal_exchange_payments(), None);
        }
    }
}

#[test]
fn a_zero_pp_exchange_is_a_completed_payment_and_still_pays_its_material() {
    let program = act_program(-4);
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = spell_game(&program, actor);
        apply(&mut game, actor, Answer::OptionalCost(true));
        apply(&mut game, actor, Answer::CostMaterials(vec![0]));
        let saved = records(&game);
        assert_eq!(saved.len(), 1);
        assert_pp(&game, &saved[0], 0, actor);
        assert!(saved[0].accepted);
        assert!(saved[0].material.is_some());
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["m0"]["acted"],
            true
        );
        finish_may(&mut game);
    }
}

#[test]
fn pp_increases_record_one_adjusted_debit_including_zero_without_a_material_step() {
    for (discount, accepted, amount) in [(-6_i64, true, 2_i64), (-6, false, 0), (-9, true, 0)] {
        let program = spell_program(
            json!({"optional_cost":{"pp":4,"result":"exchange"},"cost_delta":discount}),
        );
        for actor in [Viewer::P1, Viewer::P2] {
            let mut game = spell_game(&program, actor);
            assert!(
                records(&game).is_empty(),
                "only a completed exchange creates an exchange payment"
            );
            apply(&mut game, actor, Answer::OptionalCost(accepted));
            let saved = records(&game);
            assert_eq!(saved.len(), 1);
            let paid = &saved[0];
            assert_pp(&game, paid, amount, actor);
            assert_eq!(paid.accepted, accepted);
            assert!(
                paid.material.is_none(),
                "PP increase joins the intrinsic debit rather than inventing a material writer"
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()[seat(actor)]["pp"]["current"],
                4 - amount
            );
            let progress = finish_may(&mut game);
            assert!(
                matches!(progress.receipts.get(&paid.receipt),Some(InstructionOutcome::ExchangePayment(result)) if result.steps.len()==usize::from(accepted))
            );
        }
    }
}

#[test]
fn discarded_exchange_materials_keep_the_writer_outcome_and_remain_owned_by_a_clone() {
    let program = spell_program(
        json!({"optional_cost":{"discard":{"sel":{"who":"self","zone":"hand"},"n":1},"result":"exchange"},"optional_cost_delta":-2}),
    );
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = spell_game(&program, actor);
        apply(&mut game, actor, Answer::OptionalCost(true));
        apply(&mut game, actor, Answer::CostMaterials(vec![1]));
        let saved = records(&game);
        let material = saved[0].material.as_ref().unwrap();
        assert_eq!(*material.instruction, *material_instruction(&program));
        let declared = game
            .internal_play_declarations()
            .unwrap()
            .iter()
            .find_map(|item| {
                if let PlayDeclaration::ExchangeMaterials { objects, .. } = item {
                    Some(objects)
                } else {
                    None
                }
            })
            .unwrap();
        let outcome = if let InstructionOutcome::Discard(outcome) = &material.outcome {
            Some(outcome)
        } else {
            None
        };
        assert!(
            outcome.is_some(),
            "the exchange retained its actual discard writer result"
        );
        let outcome = outcome.unwrap();
        assert_eq!(
            outcome
                .successful
                .iter()
                .map(|member| member.before.unwrap().object)
                .collect::<Vec<_>>(),
            *declared
        );
        assert_eq!(outcome.moves.len(), 1);
        assert_eq!(outcome.moves[0].before, declared[0]);
        assert!(outcome.moves[0].after.is_some());
        assert_eq!(
            game.projection(View::Referee).unwrap()[seat(actor)]["cemetery"],
            json!(["h1"])
        );
        let mut cloned = game.clone();
        let progress = finish_may(&mut game);
        assert_eq!(
            progress.receipts.get(&material.instruction.receipt),
            Some(&material.outcome)
        );
        assert_eq!(game.internal_exchange_payments(), None);
        assert_eq!(
            records(&cloned),
            saved,
            "finishing one execution does not retire a cloned authority's payment"
        );
        finish_may(&mut cloned);
        assert_eq!(cloned.internal_exchange_payments(), None);
    }
}

#[test]
fn a_later_body_fault_rolls_back_exchange_payment_and_its_material() {
    let program = lifecycle::with_overflow(&act_program(-2));
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = spell_game(&program, actor);
        apply(&mut game, actor, Answer::OptionalCost(true));
        apply(&mut game, actor, Answer::CostMaterials(vec![0]));
        assert_eq!(records(&game).len(), 1);
        let q = question(&game, actor);
        assert_eq!(
            submit(&mut game, actor, &q, Answer::Integer(Nat(1))),
            SubmissionKind::EngineFault
        );
        assert_eq!(game.internal_exchange_payments(), None);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[seat(actor)]["pp"]["current"], 4);
        assert_eq!(state[seat(actor)]["hand"].as_array().unwrap().len(), 3);
        assert_eq!(state["objects"]["m0"]["acted"], false);
    }
}

#[test]
fn an_ordinary_paid_activation_does_not_become_an_exchange_payment() {
    let program = program(
        "activate",
        json!({"cost":{"pp":1},"do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]}),
    );
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = fixture(&program, actor);
        assert_eq!(game.internal_exchange_payments(), None);
        game.activate_internal_card(
            "source",
            program.card().abilities[0].node.id,
            actor.player(),
        )
        .unwrap();
        assert!(
            records(&game).is_empty(),
            "only a completed exchange creates an exchange payment"
        );
        assert!(
            !game.internal_cost_spec_payments().unwrap().is_empty(),
            "the ordinary activation actually completed a CostSpec payment"
        );
        finish_may(&mut game);
    }
}
