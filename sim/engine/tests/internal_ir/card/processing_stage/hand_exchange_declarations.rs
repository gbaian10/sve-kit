use super::*;
use sve_engine::game::internal::{CostPaymentPurpose, InputDomain, PlayDeclaration};

fn hand_program(two_fees: bool) -> WellFormedCardProgram {
    if !two_fees {
        return program(
            "activate",
            json!({"cost":{"steps":[{"pp":1},{"act_self":true},
            {"discard":{"sel":{"who":"self","zone":"hand","kind":"spell"},"n":1}}]},
            "do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]}),
        );
    }
    let mut raw = program("auto", json!({
        "on":{"phase":{"whose":"self","which":"end"}},
        "optional_cost":{"discard":{"sel":{"who":"self","zone":"hand","kind":"spell"},"n":1},"result":"fees"},
        "do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]
    })).card().clone();
    let steps = if let Node::Control {
        control:
            Control::Pay {
                costs: CostSpec::Ordered { steps },
                ..
            },
    } = &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
    {
        Some(steps)
    } else {
        None
    };
    assert!(
        steps.is_some(),
        "the fixture has an optional trigger head payment"
    );
    let steps = steps.unwrap();
    let mut second = steps[0].clone();
    second.instruction.id = InstructionId(1000);
    second.instruction.receipt = ReceiptId(1000);
    let constraint = if let Action::Discard {
        mode: DiscardMode::Chosen { constraint },
        ..
    } = &mut second.instruction.action
    {
        Some(constraint)
    } else {
        None
    };
    assert!(constraint.is_some(), "the copied cost is a chosen discard");
    let selector = &mut constraint.unwrap().groups[0].selector;
    let updated = if let Selector::Zone {
        binding, predicate, ..
    } = selector
    {
        *binding = Binding::new(1000, ScopeId(1000));
        **predicate = Expr::Literal(Bool(true));
        true
    } else {
        false
    };
    assert!(
        updated,
        "the second unfiltered fee owns a distinct predicate scope"
    );
    steps.push(second);
    // Optional trigger heads already admit joint hand costs; activation heads do not.
    WellFormedCardProgram::check(raw).unwrap()
}

fn seat(actor: Viewer) -> &'static str {
    if actor == Viewer::P1 { "P1" } else { "P2" }
}

fn start(program: &WellFormedCardProgram, actor: Viewer, eligible: usize) -> Game {
    let mut game = fixture_with(program, actor, |initial| {
        initial["players"][seat(actor)]["zones"]["hand"] = json!((0..3).map(|i| json!({
            "id":format!("h{i}"),"card":if i < eligible {"unit-spell"} else {"unit-follower"}
        })).collect::<Vec<_>>());
    });
    if program.card().abilities[0].node.trigger.is_some() {
        end_trigger(&mut game, actor);
        apply(&mut game, actor, Answer::OptionalCost(true));
    } else {
        game.activate_internal_card(
            "source",
            program.card().abilities[0].node.id,
            actor.player(),
        )
        .unwrap();
    }
    game
}

fn apply(game: &mut Game, actor: Viewer, answer: Answer) {
    let q = question(game, actor);
    assert_eq!(
        submit(game, actor, &q, answer),
        SubmissionKind::Applied,
        "the fixture accepts its current declaration"
    );
}

fn declarations(game: &Game) -> Vec<PlayDeclaration> {
    let before = exact_authority(game);
    let saved = game.internal_play_declarations().unwrap().to_vec();
    for viewer in [Viewer::P1, Viewer::P2] {
        game.observe(viewer).unwrap();
    }
    assert_eq!(
        game.internal_play_declarations().unwrap(),
        saved,
        "observations preserve accepted declarations"
    );
    assert!(
        exact_authority(game) == before,
        "diagnostic reads must not change authority"
    );
    saved
}

fn hand_choices(game: &Game) -> Vec<(ReceiptId, Vec<ObjectRef>)> {
    declarations(game)
        .into_iter()
        .filter_map(|item| {
            if let PlayDeclaration::HandCostMaterials {
                receipt, objects, ..
            } = item
            {
                Some((receipt, objects))
            } else {
                None
            }
        })
        .collect()
}

fn unpaid(game: &Game, actor: Viewer) {
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state[seat(actor)]["pp"]["current"],
        3,
        "the first choice does not debit PP"
    );
    assert_eq!(
        state["objects"]["source"]["acted"], false,
        "the first choice does not act the source"
    );
    assert_eq!(
        state[seat(actor)]["hand"].as_array().unwrap().len(),
        3,
        "the first choice leaves all hand cards in place"
    );
    assert!(
        game.internal_cost_spec_payments().unwrap().is_empty(),
        "accepting a hand choice does not finish a payment"
    );
}

fn candidates(game: &Game) -> usize {
    let decision = game.internal_decision(View::Referee).unwrap();
    let domain = if let InputDomain::CostMaterials(domain) = decision.domain {
        Some(domain)
    } else {
        None
    };
    assert!(
        domain.is_some(),
        "the fixture waits for hand cost materials"
    );
    domain.unwrap().members.len()
}

fn assert_paid_hand_choices(game: &Game, actor: Viewer) {
    for (receipt, objects) in hand_choices(game) {
        let costs = game.internal_cost_spec_payments().unwrap();
        let paid = costs
            .iter()
            .flat_map(|cost| &cost.steps)
            .find(|step| step.instruction.receipt == receipt)
            .unwrap();
        assert_eq!(
            paid.actor,
            actor.player(),
            "the writer retains the actual payer"
        );
        assert_eq!(
            paid.subjects, objects,
            "the writer paid the declared physical generations"
        );
        assert_eq!(
            paid.quantity,
            Quantity {
                amount: Int(1),
                unit: QuantityUnit::Cards
            },
            "the hand fee paid one card"
        );
    }
}

#[test]
fn an_accepted_hand_cost_keeps_the_actual_cost_receipt_and_selected_generations() {
    let program = hand_program(false);
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = start(&program, actor, 2);
        assert!(
            declarations(&game).is_empty(),
            "affordability probes do not declare"
        );
        unpaid(&game, actor);
        let q = question(&game, actor);
        let before = exact_authority(&game);
        let other = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        assert_eq!(
            submit(&mut game, other, &q, Answer::CostMaterials(vec![0])),
            SubmissionKind::Rejected
        );
        for answer in [
            Answer::CostMaterials(vec![99]),
            Answer::CostMaterials(vec![0, 0]),
            Answer::Targets(vec![0]),
        ] {
            assert_eq!(
                submit(&mut game, actor, &q, answer),
                SubmissionKind::Rejected
            );
        }
        assert!(
            exact_authority(&game) == before,
            "invalid answers do not leave accepted choices"
        );
        apply(&mut game, actor, Answer::CostMaterials(vec![0]));
        assert!(
            matches!(declarations(&game).as_slice(), [PlayDeclaration::HandCostMaterials {player,objects,..}] if *player==actor.player() && objects.len()==1),
            "accepted cost choices remain after payment consumes cost_materials"
        );
        assert_paid_hand_choices(&game, actor);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[seat(actor)]["cemetery"], json!(["h0"]));
        assert_eq!(state[seat(actor)]["pp"]["current"], 2);
        closed_wire(&game, actor);
        finish_may(&mut game);
        assert_eq!(game.internal_play_declarations(), None);
    }
}

#[test]
fn successive_hand_fees_retain_each_choice_before_payment() {
    let program = hand_program(true);
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = start(&program, actor, 3);
        apply(&mut game, actor, Answer::CostMaterials(vec![0]));
        assert_eq!(hand_choices(&game).len(), 1);
        unpaid(&game, actor);
        assert_eq!(
            candidates(&game),
            2,
            "the first reservation is excluded from the second fee"
        );
        apply(&mut game, actor, Answer::CostMaterials(vec![0]));
        let choices = hand_choices(&game);
        assert_eq!(choices.len(), 2);
        assert_ne!(
            choices[0].0, choices[1].0,
            "separate cost instructions have separate receipts"
        );
        assert_ne!(
            choices[0].1, choices[1].1,
            "a physical hand card cannot pay both costs"
        );
        assert_paid_hand_choices(&game, actor);
        assert_eq!(
            game.projection(View::Referee).unwrap()[seat(actor)]["cemetery"],
            json!(["h0", "h1"])
        );
        finish_may(&mut game);
    }
}

#[test]
fn hidden_hand_cost_candidate_counts_do_not_change_the_waiting_seats_native_board() {
    let program = hand_program(true);
    for actor in [Viewer::P1, Viewer::P2] {
        let waiting = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        let two = start(&program, actor, 2);
        let three = start(&program, actor, 3);
        assert_eq!(candidates(&two), 2);
        assert_eq!(candidates(&three), 3);
        assert_eq!(
            two.observe(waiting).unwrap(),
            three.observe(waiting).unwrap()
        );
        assert_eq!(two.input(waiting), three.input(waiting));
        closed_wire(&two, actor);
        closed_wire(&three, actor);
    }
}

#[test]
fn different_accepted_hand_choices_before_payment_preserve_the_waiting_native_board() {
    let program = hand_program(true);
    for actor in [Viewer::P1, Viewer::P2] {
        let waiting = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        let mut first = start(&program, actor, 3);
        let mut second = first.clone();
        apply(&mut first, actor, Answer::CostMaterials(vec![0]));
        apply(&mut second, actor, Answer::CostMaterials(vec![1]));
        assert_ne!(hand_choices(&first), hand_choices(&second));
        unpaid(&first, actor);
        unpaid(&second, actor);
        assert_eq!(
            first.observe(waiting).unwrap(),
            second.observe(waiting).unwrap()
        );
        assert_eq!(first.input(waiting), second.input(waiting));
        closed_wire(&first, actor);
        closed_wire(&second, actor);
    }
}

#[test]
fn a_later_body_fault_discards_hand_declarations_and_rolls_back_payment() {
    let program = lifecycle::with_overflow(&hand_program(false));
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = start(&program, actor, 2);
        apply(&mut game, actor, Answer::CostMaterials(vec![0]));
        assert_eq!(hand_choices(&game).len(), 1);
        let q = question(&game, actor);
        assert_eq!(
            submit(&mut game, actor, &q, Answer::Integer(Nat(1))),
            SubmissionKind::EngineFault
        );
        assert_eq!(game.internal_play_declarations(), None);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[seat(actor)]["pp"]["current"], 3);
        assert_eq!(state[seat(actor)]["hand"].as_array().unwrap().len(), 3);
        assert_eq!(state["objects"]["source"]["acted"], false);
    }
}

fn spell_game(program: &WellFormedCardProgram, actor: Viewer) -> Game {
    let mut initial = exchange::initial("unit-spell", "field", 2, 4);
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"h0","card":"unit-spell"}),
            json!({"id":"h1","card":"unit-spell"}),
        ]);
    if actor == Viewer::P2 {
        let p1 = initial["players"]["P1"].take();
        let p2 = initial["players"]["P2"].take();
        initial["players"]["P1"] = p2;
        initial["players"]["P2"] = p1;
        initial["turn"]["active"] = json!("P2");
    }
    let mut game = exchange::position(program, &initial, 4, "unit-spell");
    game.play_internal_card("s", actor.player()).unwrap();
    game
}

fn spell_program(header: Value) -> WellFormedCardProgram {
    let mut tree = exchange::candidate("unit-spell", header, true);
    tree["cards"]["unit-spell"]["abilities"][1]["spell"]["do"] = json!([
        {"optional":{"do":[{"life":{"who":"self","delta":1}}]}}
    ]);
    exchange::lower(&tree, 4)
}

fn finish_exchange(game: &mut Game, receipt: ReceiptId) {
    let selected = declarations(game).into_iter().find_map(|declaration| {
        if let PlayDeclaration::ExchangeMaterials { objects, .. } = declaration {
            Some(objects)
        } else {
            None
        }
    });
    let progress = finish_may(game);
    if let Some(selected) = selected {
        assert!(
            matches!(progress.receipts.get(&receipt), Some(InstructionOutcome::Payment { subjects,.. }) if *subjects==selected),
            "the exchange payment writer used exactly the declared physical generations"
        );
    }
    assert_eq!(
        game.internal_play_declarations(),
        None,
        "finishing the body retires exchange declarations"
    );
}

#[test]
fn exchange_acceptance_and_materials_retain_their_own_identities_after_payment() {
    let program = spell_program(json!({
        "optional_cost":{"act":{"sel":{"who":"self","zone":"field"},"n":1},"result":"exchange"},
        "optional_cost_delta":-2
    }));
    let receipt = program.card().card_play_costs[0]
        .node
        .exchange
        .as_ref()
        .unwrap()
        .node
        .receipt;
    let cost = &program.card().card_play_costs[0]
        .node
        .exchange
        .as_ref()
        .unwrap()
        .node
        .cost;
    let steps = if let CostSpec::Ordered { steps } = cost {
        Some(steps)
    } else {
        None
    };
    assert!(
        steps.is_some(),
        "the fixture has one ordered exchange route"
    );
    let material_receipt = steps.unwrap()[0].instruction.receipt;
    for actor in [Viewer::P1, Viewer::P2] {
        for accepted in [false, true] {
            let mut game = spell_game(&program, actor);
            apply(&mut game, actor, Answer::OptionalCost(accepted));
            let optional = PlayDeclaration::OptionalCost {
                player: actor.player(),
                receipt,
                required: false,
                accepted,
            };
            assert_eq!(declarations(&game), vec![optional.clone()]);
            if accepted {
                let decision = game
                    .internal_decision(View::Referee)
                    .unwrap()
                    .token
                    .decision;
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(state[seat(actor)]["pp"]["current"], 4);
                assert_eq!(state["objects"]["m0"]["acted"], false);
                let q = question(&game, actor);
                let before = exact_authority(&game);
                for answer in [
                    Answer::CostMaterials(vec![0, 0]),
                    Answer::CostMaterials(vec![99]),
                    Answer::Targets(vec![0]),
                ] {
                    assert_eq!(
                        submit(&mut game, actor, &q, answer),
                        SubmissionKind::Rejected
                    );
                }
                assert!(
                    exact_authority(&game) == before,
                    "invalid exchange materials record nothing"
                );
                apply(&mut game, actor, Answer::CostMaterials(vec![0]));
                let saved = declarations(&game);
                assert_eq!(saved[0], optional);
                assert!(
                    matches!(&saved[1], PlayDeclaration::ExchangeMaterials {player,decision:saved_decision,objects} if *player==actor.player() && *saved_decision==decision && objects.len()==1),
                    "the accepted exchange question retains its material identity"
                );
                assert_eq!(
                    saved.len(),
                    2,
                    "a single implicit route is not an answered route question"
                );
            }
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state[seat(actor)]["pp"]["current"],
                if accepted { 2 } else { 0 }
            );
            assert_eq!(state["objects"]["m0"]["acted"], accepted);
            assert_eq!(state["objects"]["m1"]["acted"], false);
            closed_wire(&game, actor);
            finish_exchange(&mut game, material_receipt);
        }
    }
}

#[test]
fn an_additional_hand_fee_has_its_own_declaration_and_payment_purpose() {
    let program = spell_program(json!({"additional_cost":{
        "discard":{"sel":{"who":"self","zone":"hand","kind":"spell"},"n":1}
    }}));
    let mut raw = program.card().clone();
    let mut permission = quick::permit();
    permission.node.id = AbilityId(9000);
    permission.node.scope = ScopeId(9000);
    raw.abilities.push(permission);
    let program = WellFormedCardProgram::check(raw).unwrap();
    let outer_receipt = program.card().card_play_costs[0]
        .node
        .additional
        .as_ref()
        .unwrap()
        .node
        .receipt;
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = spell_game(&program, actor);
        apply(&mut game, actor, Answer::CostMaterials(vec![0]));
        assert_eq!(hand_choices(&game).len(), 1);
        assert_paid_hand_choices(&game, actor);
        let choices = hand_choices(&game);
        let costs = game.internal_cost_spec_payments().unwrap();
        assert!(
            costs.iter().any(|cost| cost.purpose
                == CostPaymentPurpose::Additional {
                    receipt: outer_receipt
                }
                && cost
                    .steps
                    .iter()
                    .any(|step| step.instruction.receipt == choices[0].0)),
            "the additional producer retains its own cost instruction receipt"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()[seat(actor)]["cemetery"],
            json!(["h0"])
        );
        finish_may(&mut game);
    }
}
