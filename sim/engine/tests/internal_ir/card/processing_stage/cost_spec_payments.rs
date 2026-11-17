use super::*;
use sve_engine::game::internal::{CostPaymentPurpose, CostSpecPayment, InstructionOutcome};

fn mixed_cost_program() -> WellFormedCardProgram {
    let program = program(
        "activate",
        json!({"cost":{"steps":[{"pp":1},{"act_self":true}]},
            "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
            "do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]
        }),
    );
    let mut raw = program.card().clone();
    let cost = &mut raw.abilities[0].node.play.as_mut().unwrap().costs;
    let steps = if let CostSpec::Ordered { steps } = cost {
        Some(steps)
    } else {
        None
    };
    assert!(steps.is_some(), "the fixture requires ordered costs");
    let steps = steps.unwrap();
    let mut life = steps[0].clone();
    life.instruction.id = InstructionId(1000);
    life.instruction.receipt = ReceiptId(1000);
    life.instruction.action = Action::Life {
        player: PlayerRef::Controller {},
        change: NumericChange::Add {
            amount: Expr::Literal(Int(-2)),
        },
    };
    steps.insert(1, life);
    WellFormedCardProgram::check(raw).unwrap()
}

fn snapshot(game: &Game) -> Vec<CostSpecPayment> {
    let before = exact_authority(game);
    let first = game.internal_cost_spec_payments().unwrap().to_vec();
    assert_eq!(
        game.internal_cost_spec_payments().unwrap(),
        first.as_slice(),
        "repeated reads preserve completed costs"
    );
    assert!(
        exact_authority(game) == before,
        "cost snapshots are read-only"
    );
    first
}

fn assert_mixed_costs(paid: &[CostSpecPayment], actor: Viewer) {
    assert_eq!(paid.len(), 2, "both original and mode costs finished");
    assert_eq!(
        paid[0].purpose,
        CostPaymentPurpose::Original,
        "the first invocation paid original costs"
    );
    assert_eq!(
        paid[1].purpose,
        CostPaymentPurpose::Modes,
        "mode costs have their own invocation"
    );
    assert!(
        paid[1].steps.is_empty(),
        "an actually completed empty CostSpec is known empty"
    );
    let steps = &paid[0].steps;
    assert_eq!(
        steps.len(),
        3,
        "the original cost has three ordered instructions"
    );
    assert!(
        steps.iter().all(|step| step.actor == actor.player()),
        "step actors are frozen in this execution"
    );
    assert_eq!(
        steps
            .iter()
            .map(|step| (step.quantity.unit, step.quantity.amount.0))
            .collect::<Vec<_>>(),
        vec![
            (QuantityUnit::Pp, 1),
            (QuantityUnit::Life, 2),
            (QuantityUnit::Cards, 1)
        ],
        "returned quantities retain the writers' units and amounts"
    );
    assert!(
        matches!(&steps[1].instruction.action, Action::Life { .. }),
        "the original life instruction is retained"
    );
    assert_eq!(
        steps[1].instruction.receipt,
        ReceiptId(1000),
        "the admitted receipt remains stable"
    );
    assert_eq!(
        steps[2].subjects.len(),
        1,
        "posture payment retains its material"
    );
}

#[test]
fn only_fully_paid_cost_specs_publish_their_ordered_steps() {
    let program = mixed_cost_program();
    for actor in [Viewer::P1, Viewer::P2] {
        let me = if actor == Viewer::P1 { "P1" } else { "P2" };
        let mut game = fixture(&program, actor);
        assert_eq!(game.internal_cost_spec_payments(), None);
        game.activate_internal_card(
            "source",
            program.card().abilities[0].node.id,
            actor.player(),
        )
        .unwrap();
        assert!(
            snapshot(&game).is_empty(),
            "before targets no cost is complete"
        );
        let before = exact_authority(&game);
        for viewer in [Viewer::P1, Viewer::P2] {
            game.observe(viewer).unwrap();
        }
        assert!(
            exact_authority(&game) == before,
            "quotes do not publish completed payment groups"
        );
        let targets = question(&game, actor);
        assert_eq!(
            submit(&mut game, actor, &targets, Answer::Targets(vec![0])),
            SubmissionKind::Applied
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[me]["pp"]["current"], 2);
        assert_eq!(state[me]["leader"]["life"], 18);
        assert_eq!(state["objects"]["source"]["acted"], true);
        let paid = snapshot(&game);
        assert_mixed_costs(&paid, actor);
        let steps = &paid[0].steps;
        closed_wire(&game, actor);
        let progress = finish_may(&mut game);
        assert!(
            matches!(progress.receipts.get(&steps[0].instruction.receipt), Some(InstructionOutcome::Payment { quantity, .. }) if quantity.amount == Int(1))
        );
        assert_eq!(game.internal_cost_spec_payments(), None);
    }
}

#[test]
fn successful_hand_costs_keep_actual_materials_and_the_original_discard_receipt() {
    let program = program(
        "activate",
        json!({"cost":{"steps":[
        {"pp":1},{"discard":{"sel":{"who":"self","zone":"hand"},"n":1}}
    ]},"do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]}),
    );
    for actor in [Viewer::P1, Viewer::P2] {
        let me = if actor == Viewer::P1 { "P1" } else { "P2" };
        let mut game = fixture_with(&program, actor, |initial| {
            initial["players"][me]["zones"]["hand"] = json!([{"id":"hidden","card":"unit-spell"}]);
        });
        game.activate_internal_card(
            "source",
            program.card().abilities[0].node.id,
            actor.player(),
        )
        .unwrap();
        assert!(
            snapshot(&game).is_empty(),
            "selecting a cost material is not payment"
        );
        let material = question(&game, actor);
        assert_eq!(
            submit(&mut game, actor, &material, Answer::CostMaterials(vec![0])),
            SubmissionKind::Applied
        );
        let paid = snapshot(&game);
        let discarded = &paid[0].steps[1];
        assert_eq!(paid[0].purpose, CostPaymentPurpose::Original);
        assert!(matches!(
            discarded.instruction.action,
            Action::Discard { .. }
        ));
        assert_eq!(
            discarded.quantity,
            Quantity {
                amount: Int(1),
                unit: QuantityUnit::Cards
            }
        );
        assert_eq!(discarded.subjects.len(), 1);
        assert!(
            discarded.characters.is_empty(),
            "the writer did not provide frozen characteristics for this discard"
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[me]["pp"]["current"], 2);
        assert_eq!(state[me]["cemetery"], json!(["hidden"]));
        closed_wire(&game, actor);
        let progress = finish_may(&mut game);
        assert!(
            matches!(progress.receipts.get(&discarded.instruction.receipt), Some(InstructionOutcome::Discard(outcome)) if outcome.moves.len() == 1)
        );
        assert_eq!(game.internal_cost_spec_payments(), None);
    }
}
