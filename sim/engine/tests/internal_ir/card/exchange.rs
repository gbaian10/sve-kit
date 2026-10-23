use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{DecisionView, InputDomain};
use sve_engine::ir::lowering::ExpandedLowerer;

fn candidate(card_no: &str, play: Value, spell: bool) -> Value {
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
fn lower(tree: &Value, cost: u64) -> WellFormedCardProgram {
    ExpandedLowerer::new()
        .unwrap()
        .lower_with_printed_costs(tree, &BTreeMap::from([("unit-spell".into(), Nat(cost))]))
        .unwrap()
        .into_values()
        .next()
        .unwrap()
}
fn initial(card_no: &str, zone: &str, materials: usize, pp: u64) -> Value {
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
fn exchange_rejects_early_receipt_and_material_triggers_before_any_observable_change() {
    let tree = candidate(
        "unit-spell",
        json!({"optional_cost":{"act":{"sel":{"who":"self","zone":"field"},"n":2},"result":"exchange"},"optional_cost_delta":-2}),
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
    let initial = initial("unit-spell", "field", 2, 4);
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
    let origin = game.clone();
    assert!(matches!(
        game.play_internal_card("s", PlayerId { seat: 1 }),
        Err(sve_engine::EngineFailure::Unsupported(_))
    ));
    transactions::assert_restored(&game, &origin);
}
