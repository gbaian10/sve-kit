use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{DecisionView, InputDomain};

pub(super) fn candidate(card_no: &str, play: Value, spell: bool) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut header = tree["cards"]["BP08-038"]["abilities"][0].clone();
    header.as_object_mut().unwrap().remove("fanfare");
    header["play"] = play;
    let mut entries = vec![header.clone()];
    if spell {
        header.as_object_mut().unwrap().remove("play");
        header["spell"] = json!({"do":[{"if":{"cond":{"paid":{"receipt":"exchange"}},"then":[{"draw":{"n":1}}],"else":[{"draw":{"n":0}}]}}]});
        entries.push(header);
    }
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({(card_no):{"meta":meta,"abilities":entries}});
    tree
}
pub(super) fn lower(tree: &Value, cost: u64) -> WellFormedCardProgram {
    lowerer()
        .lower_with_printed_costs(tree, &BTreeMap::from([("unit-spell".into(), Nat(cost))]))
        .unwrap()
        .into_values()
        .next()
        .unwrap()
}
pub(super) fn initial(card_no: &str, zone: &str, materials: usize, pp: u64) -> Value {
    let mut initial = setup(1, json!([{"id":"d","card":"unit-follower"}]));
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":card_no}]);
    initial["players"]["P1"]["zones"][zone] = json!((0..materials).map(|index|
        json!({"id":format!("m{index}"),"card":if zone=="cemetery" {"unit-spell"} else {"unit-follower"}})).collect::<Vec<_>>());
    initial["players"]["P1"]["pp"] = json!({"current":pp,"max":pp});
    initial
}
fn position(program: &WellFormedCardProgram, initial: &Value, cost: i64, card_no: &str) -> Game {
    let mut game = Game::new(
        catalog_with_cost(&json!(cost.to_string())),
        initial,
        &Value::Null,
        &Value::Null,
        "exchange",
    )
    .unwrap();
    game.load_internal_card(card_no, program).unwrap();
    game.load_internal_card(
        if card_no == "unit-spell" {
            "unit-follower"
        } else {
            "unit-spell"
        },
        &card(vec![]),
    )
    .unwrap();
    game
}
#[expect(
    clippy::panic,
    reason = "A missing public question must fail the fixture."
)]
fn awaiting(result: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = result else {
        panic!("expected exchange question");
    };
    question
}
fn start(game: &mut Game) -> DecisionView {
    let question = awaiting(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
    assert_eq!(
        question.domain,
        InputDomain::OptionalCost,
        "exchange acceptance precedes payment"
    );
    question
}
fn materials(game: &mut Game, question: &DecisionView) -> DecisionView {
    let next = awaiting(
        game.answer_internal_card(question.actor, question.token, Answer::OptionalCost(true))
            .unwrap(),
    );
    assert_ne!(
        question.token.decision, next.token.decision,
        "material input has its own decision identity"
    );
    assert!(
        matches!(next.domain, InputDomain::CostMaterials(_)),
        "accepted exchange selects materials at payment time"
    );
    next
}

#[test]
fn exchange_act_composes_cost_and_same_spell_body_reads_acceptance_after_payment() {
    let mut tree = candidate(
        "unit-spell",
        json!({"optional_cost":{"act":{"sel":{"who":"self","zone":"field"},"n":2},"result":"exchange"},"optional_cost_delta":-2,"cost_delta":-1}),
        true,
    );
    tree["cards"]["unit-spell"]["abilities"][1]["spell"]["do"]
        .as_array_mut()
        .unwrap()
        .insert(
            0,
            json!({"choice":{"n":1,"options":[[{"draw":{"n":0}}],[{"draw":{"n":0}}]]}}),
        );
    let program = lower(&tree, 4);
    for accepted in [true, false] {
        let mut game = position(
            &program,
            &initial("unit-spell", "field", 2, if accepted { 1 } else { 3 }),
            4,
            "unit-spell",
        );
        let decision = start(&mut game);
        let before = exact_authority(&game);
        let mut stale = decision.token;
        stale.epoch = stale.epoch.saturating_add(1);
        game.answer_internal_card(decision.actor, stale, Answer::OptionalCost(accepted))
            .unwrap_err();
        assert_eq!(before, exact_authority(&game));
        let modes = awaiting(
            game.answer_internal_card(
                decision.actor,
                decision.token,
                Answer::OptionalCost(accepted),
            )
            .unwrap(),
        );
        assert!(matches!(modes.domain, InputDomain::Modes { .. }));
        let payment = game
            .answer_internal_card(modes.actor, modes.token, Answer::Modes(vec![ModeId(1)]))
            .unwrap();
        let result = if accepted {
            let selection = awaiting(payment);
            let before = exact_authority(&game);
            game.answer_internal_card(
                selection.actor,
                selection.token,
                Answer::CostMaterials(vec![0, 0]),
            )
            .unwrap_err();
            assert_eq!(before, exact_authority(&game));
            game.answer_internal_card(
                selection.actor,
                selection.token,
                Answer::CostMaterials(vec![0, 1]),
            )
            .unwrap()
        } else {
            payment
        };
        let result = resolved(result);
        let receipt = program.card().card_play_costs[0]
            .node
            .exchange
            .as_ref()
            .unwrap()
            .node
            .receipt;
        assert!(
            matches!(&result.progress.receipts[&receipt], InstructionOutcome::ExchangePayment(payment) if payment.complete == Bool(accepted))
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], 0);
        assert!(result.progress.receipts.values().any(|outcome| matches!(outcome,
            InstructionOutcome::Payment { quantity: Quantity { amount, unit: QuantityUnit::Pp }, .. }
                if amount.0 == if accepted { 1 } else { 3 })));
        assert_eq!(view["objects"]["m0"]["acted"], accepted);
        assert_eq!(view["objects"]["m1"]["acted"], accepted);
        assert_eq!(
            view["P1"]["hand"].as_array().unwrap().len(),
            usize::from(accepted)
        );
    }
}

#[test]
fn exchange_set_then_delta_banishes_exact_materials_or_restores_the_entire_play() {
    let tree = candidate(
        "unit-spell",
        json!({"optional_cost":{"banish":{"sel":{"who":"self","zone":"cemetery","kind":"spell"},"n":10},"result":"exchange"},"optional_cost_set":7,"cost_delta":-5}),
        true,
    );
    let program = lower(&tree, 10);
    let mut game = position(
        &program,
        &initial("unit-spell", "cemetery", 10, 2),
        10,
        "unit-spell",
    );
    let decision = start(&mut game);
    let selection = materials(&mut game, &decision);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["resolution"], json!(["s"]));
    assert!(
        matches!(&selection.domain, InputDomain::CostMaterials(domain) if domain.members.len() == 10)
    );
    let result = resolved(
        game.answer_internal_card(
            selection.actor,
            selection.token,
            Answer::CostMaterials((0..10).collect()),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        0
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["banish"]
            .as_array()
            .unwrap()
            .len(),
        10
    );
    assert!(result.progress.facts.iter().any(
        |fact| matches!(fact, Fact::Movement(movement) if movement.method == MoveMethod::Banish)
    ));
    let mut short = position(
        &program,
        &initial("unit-spell", "cemetery", 9, 2),
        10,
        "unit-spell",
    );
    let origin = short.clone();
    let question = start(&mut short);
    short
        .answer_internal_card(question.actor, question.token, Answer::OptionalCost(true))
        .unwrap_err();
    transactions::assert_restored(&short, &origin);
    let mut poor = position(
        &program,
        &initial("unit-spell", "cemetery", 10, 1),
        10,
        "unit-spell",
    );
    let origin = poor.clone();
    let question = start(&mut poor);
    let question = materials(&mut poor, &question);
    poor.answer_internal_card(
        question.actor,
        question.token,
        Answer::CostMaterials((0..10).collect()),
    )
    .unwrap_err();
    transactions::assert_restored(&poor, &origin);
}

#[test]
fn exchange_follower_pays_materials_before_capacity_and_keeps_the_pre_payment_cost() {
    let tree = candidate(
        "unit-follower",
        json!({"optional_cost":{"to_cemetery":{"sel":{"who":"self","zone":"field"},"n":4}},"optional_cost_delta":-4,
        "cost_delta":{"if":{"cmp":{"left":{"count":{"who":"self","zone":"field"}},"op":"ge","right":5}},"delta":-1}}),
        false,
    );
    let program = lower(&tree, 0);
    let mut game = position(
        &program,
        &initial("unit-follower", "field", 5, 1),
        6,
        "unit-follower",
    );
    let question = start(&mut game);
    let question = materials(&mut game, &question);
    assert!(matches!(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![0, 1, 2, 3])
        )
        .unwrap(),
        CardPlay::Entered(_)
    ));
    let view = game.projection(View::Referee).unwrap();
    assert!(
        view["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("s"))
    );
    assert_eq!(view["P1"]["pp"]["current"], 0);
    assert_eq!(view["P1"]["cemetery"].as_array().unwrap().len(), 4);
    assert_eq!(view["P1"]["field"].as_array().unwrap().len(), 2);
}

#[test]
fn exchange_rejects_early_receipt_reads_before_any_payment() {
    let tree = candidate(
        "unit-spell",
        json!({"optional_cost":{"to_cemetery":{"sel":{"who":"self","zone":"field"},"n":2},"result":"exchange"},"optional_cost_delta":-2}),
        true,
    );
    let program = lower(&tree, 4);
    let mut bad = program.card().clone();
    let exchange = bad.card_play_costs[0]
        .node
        .exchange
        .as_ref()
        .unwrap()
        .node
        .clone();
    let span = bad.card_play_costs[0].source.clone();
    bad.card_play_costs[0].node.modifiers.push(Located {
        source: span,
        node: PlayCostModifier {
            change: CostChange::Delta {
                amount: Expr::Literal(Int(-1)),
            },
            condition: Some(Expr::Frozen(Box::new(BoolFrozen::PaymentComplete {
                receipt: PaymentReference::CardPlay {
                    face: FaceId("base".into()),
                    receipt: exchange.receipt,
                },
            }))),
        },
    });
    WellFormedCardProgram::check(bad).unwrap_err();
    let requirements = Requirements::derive(&Located {
        source: source("unit-spell"),
        node: program.card().clone(),
    });
    assert!(
        requirements
            .required
            .iter()
            .any(|requirement| requirement.capability.code == "card.play-cost.exchange")
    );
    assert!(
        requirements
            .required
            .iter()
            .any(|requirement| requirement.capability.code == "read.card-play.payment-complete")
    );
}

#[test]
fn exchange_material_lastwords_wait_until_payment_and_the_spell_body_finish() {
    let program = lower(
        &candidate(
            "unit-spell",
            json!({"optional_cost":{"to_cemetery":{"sel":{"who":"self","zone":"field"},"n":2},"result":"exchange"},"optional_cost_delta":-2}),
            true,
        ),
        4,
    );
    let mut initial = initial("unit-spell", "field", 2, 4);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"d0","card":"unit-follower"},{"id":"d1","card":"unit-follower"},{"id":"d2","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog_with_cost(&json!("4")),
        &initial,
        &Value::Null,
        &Value::Null,
        "trigger-rejection",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &program).unwrap();
    game.load_internal_card(
        "unit-follower",
        &card(vec![triggers::lastword(ability(1, vec![draw(1, 1)]))]),
    )
    .unwrap();
    let accept = start(&mut game);
    let material = materials(&mut game, &accept);
    game.internal_pending_choice().unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        4
    );
    resolved(
        game.answer_internal_card(
            material.actor,
            material.token,
            Answer::CostMaterials(vec![0, 1]),
        )
        .unwrap(),
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["pp"]["current"], 2);
    assert_eq!(after["P1"]["hand"], json!(["d0"]));
    assert_eq!(after["P1"]["cemetery"], json!(["m0", "m1", "s"]));
    for expected in [2, 1] {
        game.resume_internal_card().unwrap();
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("material Lastwords")
        };
        assert_eq!(options.len(), expected);
        resolved(
            game.answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
    }
    game.resume_internal_card().unwrap();
    assert!(game.internal_pending_choice().unwrap().is_none());
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["d0", "d1", "d2"])
    );
}

#[test]
fn posture_exchange_accepts_phase_observers_without_firing_them_during_payment() {
    let program = lower(
        &candidate(
            "unit-spell",
            json!({"optional_cost":{"act":{"sel":{"who":"self","zone":"field"},"n":1},"result":"exchange"},"optional_cost_delta":-2}),
            true,
        ),
        4,
    );
    let mut observer = candidate("unit-follower", Value::Null, false);
    let entry = &mut observer["cards"]["unit-follower"]["abilities"][0];
    entry.as_object_mut().unwrap().remove("play");
    entry["auto"] = json!({"on":{"phase":{"which":"end","whose":"self"}},"do":[{"draw":{"n":1}}]});
    let observer = lower(&observer, 1);
    let mut game = Game::new(
        catalog_with_cost(&json!("4")),
        &initial("unit-spell", "field", 1, 4),
        &Value::Null,
        &Value::Null,
        "posture-material-trigger",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &program).unwrap();
    game.load_internal_card("unit-follower", &observer).unwrap();
    let accept = start(&mut game);
    let material = materials(&mut game, &accept);
    resolved(
        game.answer_internal_card(
            material.actor,
            material.token,
            Answer::CostMaterials(vec![0]),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    assert!(game.internal_pending_choice().unwrap().is_none());
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["objects"]["m0"]["acted"], true);
    assert_eq!(after["P1"]["pp"]["current"], 2);
    assert_eq!(after["P1"]["hand"], json!(["d"]));
}

#[test]
fn exchange_play_does_not_assume_unloaded_cemetery_materials_have_no_abilities() {
    let tree = candidate(
        "unit-follower",
        json!({"optional_cost":{"banish":{"sel":{"who":"self","zone":"cemetery","kind":"spell"},"n":1}},"optional_cost_delta":-1}),
        false,
    );
    let program = lower(&tree, 0);
    assert!(program.card().card_play_costs[0].node.exchange.is_some());
    let mut game = Game::new(
        catalog_with_cost(&json!("0")),
        &initial("unit-follower", "cemetery", 1, 6),
        &Value::Null,
        &Value::Null,
        "unloaded-exchange-material",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    let before = exact_authority(&game);
    let observations = [View::P1, View::P2, View::Referee]
        .map(|view| game.projection(view).map_err(|failure| failure.to_string()));
    let error = game
        .play_internal_card("s", PlayerId { seat: 1 })
        .unwrap_err();
    assert!(
        matches!(error, sve_engine::EngineFailure::Unsupported(reason)
        if reason.contains("unit-spell/0") && reason.contains("current ability definitions unavailable"))
    );
    assert_eq!(exact_authority(&game), before);
    assert_eq!(
        [View::P1, View::P2, View::Referee]
            .map(|view| game.projection(view).map_err(|failure| failure.to_string())),
        observations
    );
    assert_eq!(game.internal_card_stage(), None);
}

fn hand_exchange(count: u64, threshold: u64) -> Value {
    let mut tree = candidate(
        "unit-spell",
        json!({
            "optional_cost":{"discard":{"sel":{"who":"self","zone":"hand"},"n":count,"result":"fuel"},"result":"exchange"},
            "optional_cost_delta":-2_i64
        }),
        true,
    );
    tree["cards"]["unit-spell"]["abilities"][1]["spell"]["do"] = json!([
        {"if":{"cond":{"cmp":{"left":{"count":{"in":{"objects_of":"fuel","view":"before"},"orig_cost_ge":threshold}},"op":"ge","right":1_u8}},"then":[{"draw":{"n":1_u8}}]}}
    ]);
    tree
}

#[test]
fn hand_exchange_uses_discarded_printed_cost_after_payment_and_declining_has_no_members() {
    for (accepted, threshold, draws) in [(true, 7, 1), (true, 8, 0), (false, 7, 0)] {
        let program = lower(&hand_exchange(1, threshold), 7);
        let mut initial = initial("unit-spell", "field", 0, 7);
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"s","card":"unit-spell"},
            {"id":"m","card":"unit-follower","state":{"cost":1}}
        ]);
        let mut game = hand_position(&program, &initial);
        let decision = start(&mut game);
        let play = game
            .answer_internal_card(
                decision.actor,
                decision.token,
                Answer::OptionalCost(accepted),
            )
            .unwrap();
        let play = if accepted {
            let question = awaiting(play);
            let InputDomain::CostMaterials(domain) = &question.domain else {
                panic!("hand materials")
            };
            assert_eq!(
                domain.members.len(),
                1,
                "the card being played cannot pay for itself"
            );
            assert!(game.internal_decision(View::P2).is_none());
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                7
            );
            let before = exact_authority(&game);
            for (actor, handles) in [
                (PlayerId { seat: 2 }, vec![0]),
                (question.actor, vec![0, 0]),
                (question.actor, vec![99]),
            ] {
                game.answer_internal_card(actor, question.token, Answer::CostMaterials(handles))
                    .unwrap_err();
                assert_eq!(exact_authority(&game), before);
            }
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(vec![0]),
            )
            .unwrap()
        } else {
            play
        };
        resolved(play);
        game.resume_internal_card().unwrap();
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "discarding from hand does not trigger field departure abilities"
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], if accepted { 2 } else { 0 });
        assert_eq!(
            view["P1"]["cemetery"],
            if accepted {
                json!(["m", "s"])
            } else {
                json!(["s"])
            }
        );
        assert_eq!(
            view["P1"]["deck"].as_array().unwrap().len(),
            1_usize - draws
        );
    }
}

#[test]
fn hand_exchange_cannot_pay_with_the_source_and_failed_pp_preserves_all_hand_cards() {
    let program = lower(&hand_exchange(1, 7), 7);
    for has_material in [true, false] {
        let mut initial = initial("unit-spell", "field", 0, if has_material { 4 } else { 7 });
        if has_material {
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"m","card":"unit-follower"}));
        }
        let mut game = position(&program, &initial, 7, "unit-spell");
        let before = game.projection(View::Referee).unwrap();
        let decision = start(&mut game);
        let result =
            game.answer_internal_card(decision.actor, decision.token, Answer::OptionalCost(true));
        if has_material {
            let material = awaiting(result.unwrap());
            game.answer_internal_card(
                material.actor,
                material.token,
                Answer::CostMaterials(vec![0]),
            )
            .unwrap_err();
        } else {
            result.unwrap_err();
        }
        let view = game.projection(View::Referee).unwrap();
        for key in ["P1", "P2", "objects"] {
            assert_eq!(view[key], before[key]);
        }
    }
}

#[test]
fn hand_exchange_rejects_variable_or_mixed_materials_and_receipt_reads_before_payment() {
    for material in [
        json!({"sel":{"who":"self","zone":"hand"},"upto":1}),
        json!({"sel":{"who":"self","zone":"hand"},"n":0}),
    ] {
        let mut tree = hand_exchange(1, 7);
        tree["cards"]["unit-spell"]["abilities"][0]["play"]["optional_cost"]["discard"] = material;
        lowerer()
            .lower_with_printed_costs(&tree, &BTreeMap::from([("unit-spell".to_owned(), Nat(7))]))
            .unwrap_err();
    }
    let mut tree = hand_exchange(1, 7);
    tree["cards"]["unit-spell"]["abilities"][0]["play"]["optional_cost"]["act"] =
        json!({"sel":{"who":"self","zone":"field"},"n":1});
    lowerer()
        .lower_with_printed_costs(&tree, &BTreeMap::from([("unit-spell".to_owned(), Nat(7))]))
        .unwrap_err();
    let mut unsupported = hand_exchange(1, 7);
    unsupported["cards"]["unit-spell"]["abilities"][1]["spell"]["do"] =
        json!([{"draw":{"n":{"count_of":"fuel"}}}]);
    lowerer()
        .lower_with_printed_costs(
            &unsupported,
            &BTreeMap::from([("unit-spell".to_owned(), Nat(7))]),
        )
        .unwrap_err();
    let program = lower(&hand_exchange(1, 7), 7);
    let mut raw = program.card().clone();
    let Node::Control {
        control: Control::If { condition, .. },
    } = &raw.abilities[0].node.body.as_ref().unwrap().steps[0].node
    else {
        panic!("receipt condition")
    };
    let condition = condition.clone();
    let span = raw.card_play_costs[0].source.clone();
    raw.card_play_costs[0].node.modifiers.push(Located {
        source: span,
        node: PlayCostModifier {
            change: CostChange::Delta {
                amount: Expr::Literal(Int(-1)),
            },
            condition: Some(condition),
        },
    });
    WellFormedCardProgram::check(raw).unwrap_err();
}

fn hand_position(program: &WellFormedCardProgram, initial: &Value) -> Game {
    let mut game = Game::new(
        catalog_with_cost(&json!("7")),
        initial,
        &Value::Null,
        &Value::Null,
        "hand-exchange-leave",
    )
    .unwrap();
    game.load_internal_card("unit-spell", program).unwrap();
    game.load_internal_card(
        "unit-follower",
        &card(vec![triggers::lastword(ability(1, vec![draw(1, 1)]))]),
    )
    .unwrap();
    game
}

#[test]
fn hand_exchange_body_can_destroy_a_follower_and_enqueue_its_lastword_after_payment() {
    let mut tree = hand_exchange(1, 7);
    tree["cards"]["unit-spell"]["abilities"][1]["spell"]["do"] =
        json!([{"destroy":{"ref":{"who":"opp","zone":"field","kind":"follower"}}}]);
    let program = lower(&tree, 7);
    let mut initial = initial("unit-spell", "field", 0, 5);
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"s","card":"unit-spell"},{"id":"m","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"lastword-draw","card":"unit-spell"}]);
    let mut game = hand_position(&program, &initial);
    let accept = start(&mut game);
    let material = materials(&mut game, &accept);
    resolved(
        game.answer_internal_card(
            material.actor,
            material.token,
            Answer::CostMaterials(vec![0]),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["pp"]["current"], 0);
    assert_eq!(view["P1"]["cemetery"], json!(["m", "s"]));
    assert_eq!(view["P2"]["cemetery"], json!(["f"]));
    game.resume_internal_card().unwrap();
    let pending = game.internal_pending_choice().unwrap().unwrap();
    assert_eq!(pending.actor, PlayerId { seat: 2 });
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("Lastword after the spell body")
    };
    assert_eq!(
        options.len(),
        1,
        "only the field departure triggers a Lastword"
    );
    resolved(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["hand"],
        json!(["lastword-draw"])
    );
}

#[test]
fn exchange_name_contains_keeps_its_filter_and_rejects_unmatched_materials() {
    let program = lower(
        &candidate(
            "unit-spell",
            json!({
                "optional_cost":{"act":{"sel":{"who":"self","zone":"field","name_contains":"follower"},"n":1},"result":"exchange"},
                "optional_cost_delta":-3
            }),
            true,
        ),
        4,
    );
    let mut initial = initial("unit-spell", "field", 1, 1);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"wrong","card":"unit-amulet"}));
    let mut game = position(&program, &initial, 4, "unit-spell");
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    let accepted = start(&mut game);
    let question = materials(&mut game, &accepted);
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("materials")
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card.as_deref(), Some("unit-follower"));
    let before = game.projection(View::Referee).unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::CostMaterials(vec![1]),
    )
    .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![0]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 0);
    assert_eq!(state["objects"]["m0"]["acted"], true);
    assert_eq!(state["objects"]["wrong"]["acted"], false);
    assert_eq!(state["P1"]["hand"], json!(["d"]));
}

fn observed_act_exchange(pp: u64) -> Game {
    let program = lower(
        &candidate(
            "unit-spell",
            json!({"optional_cost":{"act":{"sel":{"who":"self","zone":"field"},"n":1},"result":"exchange"},"optional_cost_delta":-2}),
            true,
        ),
        4,
    );
    let mut observer = candidate("unit-follower", Value::Null, false);
    let entry = &mut observer["cards"]["unit-follower"]["abilities"][0];
    entry.as_object_mut().unwrap().remove("play");
    entry["auto"] = json!({"on":{"acted":{"ref":"this"}},"per_turn":1,"do":[{"life":{"who":"self","delta":1}}]});
    let observer = lower(&observer, 1);
    let mut input = initial("unit-spell", "field", 1, pp);
    input["players"]["P1"]["leader"]["life"] = json!(10);
    let mut game = Game::new(
        catalog_with_cost(&json!("4")),
        &input,
        &Value::Null,
        &Value::Null,
        "exchange-posture-observer",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &program).unwrap();
    game.load_internal_card("unit-follower", &observer).unwrap();
    game
}

#[test]
fn posture_exchange_defers_its_trigger_and_insufficient_pp_restores_play_before_capture() {
    for pp in [1, 4] {
        let mut game = observed_act_exchange(pp);
        let origin = game.clone();
        let accept = start(&mut game);
        let material = materials(&mut game, &accept);
        game.internal_pending_choice().unwrap_err();
        let paid = game.answer_internal_card(
            material.actor,
            material.token,
            Answer::CostMaterials(vec![0]),
        );
        if pp == 1 {
            paid.unwrap_err();
            transactions::assert_restored(&game, &origin);
            assert!(game.internal_pending_choice().unwrap().is_none());
            continue;
        }
        resolved(paid.unwrap());
        let before_trigger = game.projection(View::Referee).unwrap();
        assert_eq!(before_trigger["P1"]["pp"]["current"], 2);
        assert_eq!(before_trigger["P1"]["hand"], json!(["d"]));
        assert_eq!(before_trigger["objects"]["m0"]["acted"], true);
        assert_eq!(before_trigger["P1"]["leader"]["life"], 10);
        game.resume_internal_card().unwrap();
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("committed act trigger")
        };
        assert_eq!(options.len(), 1);
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            11
        );
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}

fn follower_with_departing_material() -> Game {
    let mut source = candidate(
        "unit-follower",
        json!({"optional_cost":{"to_cemetery":{"sel":{"who":"self","zone":"field","kind":"amulet"},"n":1}},"optional_cost_delta":-1}),
        false,
    );
    source["cards"]["unit-follower"]["abilities"][0]["line"] = json!(1);
    let mut fanfare = source["cards"]["unit-follower"]["abilities"][0].clone();
    fanfare.as_object_mut().unwrap().remove("play");
    fanfare["line"] = json!(2);
    fanfare["fanfare"] = json!({"do":[{"life":{"who":"self","delta":2}}]});
    source["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(fanfare);
    let source = lowerer()
        .lower(&source)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let material = lowerer()
        .lower(&same_name::tree(
            &json!({"lastword":{"do":[{"draw":{"n":1}}]}}),
            "unit-amulet",
        ))
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let mut input = initial("unit-follower", "field", 0, 0);
    input["players"]["P1"]["leader"]["life"] = json!(10);
    input["players"]["P1"]["zones"]["field"] = json!([{"id":"material","card":"unit-amulet"}]);
    let mut game = Game::new(
        catalog(),
        &input,
        &Value::Null,
        &Value::Null,
        "exchange-entry-and-exit",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &source).unwrap();
    game.load_internal_card("unit-amulet", &material).unwrap();
    game
}

#[test]
fn follower_exchange_material_lastword_and_fanfare_share_the_next_pending_order_in_either_order() {
    for first in ["unit-follower", "unit-amulet"] {
        let mut game = follower_with_departing_material();
        let accept = start(&mut game);
        let material = materials(&mut game, &accept);
        game.internal_pending_choice().unwrap_err();
        assert!(matches!(
            game.answer_internal_card(
                material.actor,
                material.token,
                Answer::CostMaterials(vec![0])
            )
            .unwrap(),
            CardPlay::Entered(_)
        ));
        for (index, card) in [
            first,
            if first == "unit-follower" {
                "unit-amulet"
            } else {
                "unit-follower"
            },
        ]
        .iter()
        .enumerate()
        {
            finish_exchange_checks(&mut game);
            let question = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &question.domain else {
                panic!("entry and departure share a pending order")
            };
            assert_eq!(options.len(), 2 - index);
            let handle = options
                .iter()
                .find(|option| option.card == *card)
                .unwrap()
                .handle;
            resolved(
                game.answer_internal_card(question.actor, question.token, Answer::Pending(handle))
                    .unwrap(),
            );
            if index == 0 {
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(
                    state["P1"]["leader"]["life"],
                    if first == "unit-follower" { 12 } else { 10 }
                );
                assert_eq!(
                    state["P1"]["hand"],
                    if first == "unit-amulet" {
                        json!(["d"])
                    } else {
                        json!([])
                    }
                );
            }
        }
        finish_exchange_checks(&mut game);
        assert!(game.internal_pending_choice().unwrap().is_none());
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            12
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["d"])
        );
    }
}

fn finish_exchange_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

#[test]
fn pp_increase_exchange_quotes_before_clamping_and_body_reads_the_committed_choice() {
    for (discount, accepted, payment) in [(-3_i64, true, 2_u8), (-3, false, 0), (-9, true, 0)] {
        let tree = candidate(
            "unit-spell",
            json!({
                "optional_cost":{"pp":4,"result":"exchange"},"cost_delta":discount
            }),
            true,
        );
        let program = lower(&tree, 1);
        let mut game = position(
            &program,
            &initial("unit-spell", "field", 0, 5),
            1,
            "unit-spell",
        );
        let question = start(&mut game);
        let before = exact_authority(&game);
        game.answer_internal_card(
            PlayerId { seat: 2 },
            question.token,
            Answer::OptionalCost(accepted),
        )
        .unwrap_err();
        assert_eq!(
            exact_authority(&game),
            before,
            "a foreign actor cannot accept the increase"
        );
        let result = resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::OptionalCost(accepted),
            )
            .unwrap(),
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["pp"]["current"],
            5 - payment,
            "increase joins discounts before the zero floor"
        );
        assert_eq!(
            view["P1"]["hand"].as_array().unwrap().len(),
            usize::from(accepted),
            "the receipt tracks the accepted increase even when the final price is zero"
        );
        let exchange = &program.card().card_play_costs[0]
            .node
            .exchange
            .as_ref()
            .unwrap()
            .node;
        let InstructionOutcome::ExchangePayment(receipt) =
            &result.progress.receipts[&exchange.receipt]
        else {
            panic!("exchange receipt")
        };
        assert_eq!(receipt.complete, Bool(accepted));
        assert_eq!(
            receipt.steps.len(),
            usize::from(accepted),
            "there is only one PP payment"
        );
    }
}

#[test]
fn pp_increase_exchange_unaffordable_acceptance_restores_the_entire_play() {
    let tree = candidate(
        "unit-spell",
        json!({"optional_cost":{"pp":4,"result":"exchange"}}),
        true,
    );
    let program = lower(&tree, 1);
    let mut game = position(
        &program,
        &initial("unit-spell", "field", 0, 4),
        1,
        "unit-spell",
    );
    let origin = game.clone();
    let question = start(&mut game);
    game.answer_internal_card(question.actor, question.token, Answer::OptionalCost(true))
        .unwrap_err();
    transactions::assert_restored(&game, &origin);
}

#[test]
fn pp_increase_exchange_rejects_mixed_empty_or_forged_material_routes() {
    for play in [
        json!({"optional_cost":{"pp":0}}),
        json!({"optional_cost":{"pp":4},"optional_cost_delta":-1}),
        json!({"optional_cost":{"pp":4},"optional_cost_set":2}),
        json!({"optional_cost":{"pp":4,"act":{"sel":{"who":"self","zone":"field"},"n":1}}}),
        json!({"optional_cost":{"pp":4,"one_of":[]}}),
        json!({"optional_cost":{"one_of":[{"pp":4},{"pp":3}]},"optional_cost_delta":-1}),
        json!({"optional_cost":{}}),
    ] {
        lowerer()
            .lower(&candidate("unit-spell", play, false))
            .unwrap_err();
    }
    let program = lower(
        &candidate("unit-spell", json!({"optional_cost":{"pp":4}}), false),
        1,
    );
    for delta in [0, -1] {
        let mut bad = program.card().clone();
        bad.card_play_costs[0]
            .node
            .exchange
            .as_mut()
            .unwrap()
            .node
            .change = CostChange::Delta {
            amount: Expr::Literal(Int(delta)),
        };
        WellFormedCardProgram::check(bad).unwrap_err();
    }
    let material = lower(
        &candidate(
            "unit-spell",
            json!({"optional_cost":{"act":{"sel":{"who":"self","zone":"field"},"n":1}},"optional_cost_delta":-1}),
            false,
        ),
        1,
    );
    let mut bad = material.card().clone();
    bad.card_play_costs[0]
        .node
        .exchange
        .as_mut()
        .unwrap()
        .node
        .change = CostChange::Delta {
        amount: Expr::Literal(Int(4)),
    };
    WellFormedCardProgram::check(bad).unwrap_err();
}
