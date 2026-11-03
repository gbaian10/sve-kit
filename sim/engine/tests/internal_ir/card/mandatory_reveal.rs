use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{DecisionView, InputDomain};
use sve_engine::ir::lowering::LoweringError;

fn lower(tree: &Value) -> Result<BTreeMap<String, WellFormedCardProgram>, LoweringError> {
    lowerer().lower_with_printed_costs(tree, &BTreeMap::from([("unit-spell".to_owned(), Nat(1))]))
}

fn tree(cost: &Value, steps: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    let mut spell = entry.clone();
    entry["play"] = json!({"additional_cost":cost});
    spell["line"] = json!(2_u8);
    spell["spell"] = json!({"do":steps});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-spell":{"meta":meta,"abilities":[entry,spell]}});
    tree
}

fn cost(count: u8, bottom: bool) -> Value {
    let mut reveal = json!({"sel":{"who":"self","zone":"hand","kind":"follower"},"n":count});
    if bottom {
        reveal["result"] = json!("shown");
        json!({"steps":[{"reveal":reveal},{"to_deck_bottom":{"sel":{"in":"shown"},"n":1_u8}}]})
    } else {
        json!({"reveal":reveal})
    }
}

fn program(count: u8, bottom: bool, pause: bool) -> WellFormedCardProgram {
    let steps = if pause {
        json!([{"choose":{"bind":"picked","from":{"who":"self","zone":"hand"},"n":1_u8}},
            {"reveal":{"ref":"picked","bind":"again"}}])
    } else {
        json!([{"draw":{"n":1_u8}}])
    };
    lower(&tree(&cost(count, bottom), &steps))
        .unwrap()
        .remove("unit-spell")
        .unwrap()
}

fn board(count: u8, material: u8, bottom: bool, pause: bool, pp: u8) -> Game {
    board_for(&program(count, bottom, pause), material, pp)
}

fn board_for(program: &WellFormedCardProgram, material: u8, pp: u8) -> Game {
    let mut initial = setup(1, json!([{"id":"drawn","card":"unit-follower"}]));
    let mut hand = vec![
        json!({"id":"source","card":"unit-spell"}),
        json!({"id":"wrong","card":"unit-spell"}),
    ];
    hand.extend((0_u8..material).map(|i| json!({"id":format!("h{i}"),"card":"unit-follower"})));
    initial["players"]["P1"]["zones"]["hand"] = json!(hand);
    initial["players"]["P1"]["pp"] = json!({"current":pp,"max":2_u8});
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "mandatory-reveal",
    )
    .unwrap();
    game.load_internal_card("unit-spell", program).unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    game
}

#[expect(
    clippy::panic,
    reason = "A missing question fails the private payment scenario."
)]
fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected hand cost choice");
    };
    question
}

#[test]
fn mandatory_reveal_filters_private_candidates_and_expires_at_resolution_end() {
    let mut game = board(2, 3, false, true, 2);
    let cost = awaiting(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::CostMaterials(domain) = &cost.domain else {
        panic!("cost material domain");
    };
    assert_eq!(domain.members.len(), 3, "only eligible other cards");
    assert!(
        game.internal_decision(View::P2).is_none(),
        "hand candidates remain private"
    );
    let before = exact_authority(&game);
    for handles in [vec![], vec![0], vec![0, 0], vec![0, 99]] {
        game.answer_internal_card(cost.actor, cost.token, Answer::CostMaterials(handles))
            .unwrap_err();
        assert_eq!(
            exact_authority(&game),
            before,
            "invalid answers preserve the pending transaction"
        );
    }
    game.answer_internal_card(
        PlayerId { seat: 2 },
        cost.token,
        Answer::CostMaterials(vec![0, 1]),
    )
    .unwrap_err();
    assert_eq!(
        exact_authority(&game),
        before,
        "the opponent cannot pay another player's cost"
    );
    let body = awaiting(
        game.answer_internal_card(cost.actor, cost.token, Answer::CostMaterials(vec![0, 1]))
            .unwrap(),
    );
    let view = game.projection(View::P2).unwrap();
    assert_eq!(
        view["objects"]["h0"]["card"], "unit-follower",
        "first selected identity is public"
    );
    assert_eq!(
        view["objects"]["h1"]["card"], "unit-follower",
        "second selected identity is public"
    );
    assert!(
        view["objects"]["h2"].is_null(),
        "unselected hand cards stay concealed"
    );
    assert!(
        view["objects"]["wrong"].is_null(),
        "ineligible cards stay concealed"
    );
    resolved(
        game.answer_internal_card(body.actor, body.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    let after = game.projection(View::P2).unwrap();
    assert!(
        after["objects"]["h0"].is_null(),
        "revelation expires with this resolution"
    );
    assert!(
        after["objects"]["h1"].is_null(),
        "no stable public hand identity remains"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        1_u8,
        "printed PP is paid once"
    );
}

#[test]
fn revealing_then_bottoming_moves_the_same_selected_card_before_drawing() {
    let mut game = board(1, 2, true, false, 2);
    let cost = awaiting(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
    );
    resolved(
        game.answer_internal_card(cost.actor, cost.token, Answer::CostMaterials(vec![1]))
            .unwrap(),
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["P1"]["deck"],
        json!(["h1"]),
        "the selected card goes beneath the existing draw"
    );
    assert_eq!(
        after["P1"]["hand"],
        json!(["wrong", "h0", "drawn"]),
        "unselected cards remain and the old top is drawn"
    );
    assert!(
        game.projection(View::P2).unwrap()["objects"]["h1"].is_null(),
        "the revealed card's hidden deck location is not exposed"
    );
    let packet = game.projection(View::P2).unwrap();
    assert!(
        packet["P1"]["deck"]
            .as_array()
            .unwrap()
            .iter()
            .all(|card| card == &json!({"filler":1_u8})),
        "hidden deck positions carry no revealed object id"
    );
    assert!(
        !packet["knowledge"]["located"]
            .as_array()
            .unwrap()
            .contains(&json!("h1")),
        "historical identity cannot locate the card in the deck"
    );
    assert!(
        packet.get("events").is_none() && packet.get("log").is_none(),
        "outgoing projection excludes authority event logs"
    );
}

#[test]
fn body_overflow_rolls_back_public_revelation_and_bottom_movement_with_the_play() {
    for bottom in [false, true] {
        let mut raw = program(1, bottom, false).card().clone();
        let ability = raw.abilities.first_mut().unwrap();
        let overflow = draw_value(
            9001,
            Expr::Compute(Box::new(IntOperation::Arithmetic {
                operation: Arithmetic::Add,
                arguments: vec![Expr::Literal(Int(i64::MAX)), Expr::Literal(Int(1))],
            })),
        );
        ability.node.body.as_mut().unwrap().steps.push(overflow);
        locate_steps(
            &mut ability.node.body.as_mut().unwrap().steps,
            &ability.source,
            &ability.node.source,
        );
        let checked = WellFormedCardProgram::check(raw).unwrap();
        let mut game = board_for(&checked, 1, 2);
        let before = game.projection(View::Referee).unwrap();
        let question = awaiting(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
        );
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![0]),
        )
        .unwrap_err();
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            before,
            "body failure restores PP, all cards and knowledge"
        );
        assert!(
            game.internal_decision(View::Referee).is_none(),
            "failed body leaves no continuation"
        );
        assert!(
            game.projection(View::P2).unwrap()["objects"]["h0"].is_null(),
            "an unsuccessful play publishes no hand identity"
        );
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![0]),
        )
        .unwrap_err();
    }
}

#[test]
fn missing_material_and_late_pp_failure_restore_the_play_without_revealing() {
    for (count, material, pp) in [(2_u8, 1_u8, 2_u8), (1, 1, 0)] {
        let mut game = board(count, material, false, false, pp);
        let before = game.projection(View::Referee).unwrap();
        let played = game.play_internal_card("source", PlayerId { seat: 1 });
        if material < count {
            played.unwrap_err();
        } else {
            let q = awaiting(played.unwrap());
            game.answer_internal_card(q.actor, q.token, Answer::CostMaterials(vec![0]))
                .unwrap_err();
            assert!(
                game.internal_decision(View::Referee).is_none(),
                "failed payments leave no continuation"
            );
        }
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            before,
            "failed play restores visible state"
        );
        assert!(
            game.projection(View::P2).unwrap()["objects"]["h0"].is_null(),
            "failure reveals no hand identity"
        );
    }
}

#[test]
fn revelation_costs_reject_foreign_hands_variable_counts_and_unrelated_bottom_choices() {
    for bad in [
        json!({"reveal":{"sel":{"who":"opp","zone":"hand"},"n":1_u8}}),
        cost(0, false),
        cost(3, false),
        cost(2, true),
        json!({"reveal":{"sel":{"who":"self","zone":"hand"},"n":1_u8,"result":"unused"}}),
        json!({"to_deck_bottom":{"sel":{"in":"shown"},"n":1_u8}}),
        json!({"steps":[{"reveal":{"sel":{"who":"self","zone":"hand"},"n":1_u8}},{"pp":1_u8}]}),
        json!({"steps":[{"reveal":{"sel":{"who":"self","zone":"hand"},"n":1_u8,"result":"shown"}},{"to_deck_bottom":{"sel":{"in":"shown"},"n":2_u8}}]}),
        json!({"reveal":{"sel":{"who":"self","zone":"hand"},"upto":1_u8}}),
        json!({"steps":[{"reveal":{"sel":{"who":"self","zone":"hand"},"n":1_u8,"result":"shown"}},
            {"to_deck_bottom":{"sel":{"in":"another"},"n":1_u8}}]}),
    ] {
        assert!(
            lower(&tree(&bad, &json!([{"draw":{"n":1_u8}}]))).is_err(),
            "only exact mandatory own-hand costs are lowerable"
        );
    }
}

#[expect(
    clippy::panic,
    reason = "The shared valid fixture must retain its private material domain."
)]
fn private_parameter(scope: ScopeId) -> Decision {
    let lowered = program(1, false, false);
    let face = lowered.card().card_play_costs.first().unwrap();
    let mut decision = face
        .node
        .additional
        .as_ref()
        .unwrap()
        .node
        .play
        .parameters
        .first()
        .unwrap()
        .clone();
    let ChoiceDomain::Objects { binding, .. } = &mut decision.domain else {
        panic!("private material shape");
    };
    *binding = Binding::new(9000, scope);
    decision.id = DecisionId(9000);
    decision
}

#[test]
fn private_hand_materials_cannot_escape_to_root_mode_or_intrinsic_evolution() {
    let mut root = card(vec![play::spell(vec![draw(1, 1)])]).card().clone();
    WellFormedCardProgram::check(root.clone()).unwrap();
    let ability = root.abilities.first_mut().unwrap();
    ability
        .node
        .play
        .as_mut()
        .unwrap()
        .parameters
        .push(private_parameter(ability.node.scope));
    let rejected = WellFormedCardProgram::check(root).unwrap_err();
    assert_eq!(
        rejected.reason,
        WellFormedReason::UnsupportedShape,
        "private visibility is the rejected shape"
    );
    assert!(
        rejected.ir_path.contains("parameters"),
        "the parameter boundary rejects before binding or cost processing: {rejected}"
    );

    let all = lower(fanfare::expanded::tree()).unwrap();
    let mut evolution = all["synthetic-evolution"].card().clone();
    WellFormedCardProgram::check(evolution.clone()).unwrap();
    let ability = evolution.abilities.first_mut().unwrap();
    ability
        .node
        .play
        .as_mut()
        .unwrap()
        .parameters
        .push(private_parameter(ability.node.scope));
    let rejected = WellFormedCardProgram::check(evolution).unwrap_err();
    assert_eq!(
        rejected.reason,
        WellFormedReason::UnsupportedShape,
        "private visibility is the rejected shape"
    );
    assert!(
        rejected.ir_path.contains("parameters"),
        "the parameter boundary rejects before binding or cost processing: {rejected}"
    );

    let document = tree(
        &cost(1, false),
        &json!([{"choice":{"n":1_u8,"options":[[{"draw":{"n":0_u8}}],[{"draw":{"n":1_u8}}]]}}]),
    );
    let mut modes = lower(&document)
        .unwrap()
        .remove("unit-spell")
        .unwrap()
        .card()
        .clone();
    modes.card_play_costs.clear();
    WellFormedCardProgram::check(modes.clone()).unwrap();
    let node = &mut modes
        .abilities
        .first_mut()
        .unwrap()
        .node
        .body
        .as_mut()
        .unwrap()
        .steps
        .first_mut()
        .unwrap()
        .node;
    let Node::Control {
        control: Control::Mode { modes: choices, .. },
    } = node
    else {
        panic!("mode fixture");
    };
    let choice = choices.first_mut().unwrap();
    choice.parameters.push(private_parameter(choice.body.scope));
    let rejected = WellFormedCardProgram::check(modes).unwrap_err();
    assert_eq!(
        rejected.reason,
        WellFormedReason::UnsupportedShape,
        "private visibility is the rejected shape"
    );
    assert!(
        rejected.ir_path.contains("parameters"),
        "the parameter boundary rejects before binding or cost processing: {rejected}"
    );
}

#[test]
fn raw_revelation_cost_rejects_changed_card_destination_position_and_quick() {
    for kind in ["binding", "hand", "top"] {
        let mut raw = program(1, true, false).card().clone();
        let face = raw.card_play_costs.first_mut().unwrap();
        let CostSpec::Ordered { steps } =
            &mut face.node.additional.as_mut().unwrap().node.play.costs
        else {
            panic!("ordered costs");
        };
        let Action::Move {
            objects, movement, ..
        } = &mut steps.last_mut().unwrap().instruction.action
        else {
            panic!("deck bottom");
        };
        match kind {
            "binding" => {
                *objects = Selector::Bound {
                    reference: Binding::new(9999, ScopeId(9999)),
                }
            }
            "hand" => {
                movement.to = MoveDestination::Owners {
                    zone: OwnerZone::Hand,
                }
            }
            _ => movement.position = Some(Position::Top {}),
        }
        WellFormedCardProgram::check(raw).unwrap_err();
    }
    let mut raw = program(1, false, false).card().clone();
    raw.abilities.push(quick::permit());
    let checked = WellFormedCardProgram::check(raw).unwrap();
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "reveal-quick-denied",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &checked).unwrap_err();
}

#[test]
fn revelation_cannot_be_an_unused_additional_parameter_or_an_ordinary_pay_cost() {
    let mut unused = program(1, false, false).card().clone();
    let additional = unused
        .card_play_costs
        .first_mut()
        .unwrap()
        .node
        .additional
        .as_mut()
        .unwrap();
    let CostSpec::Ordered { steps } = &mut additional.node.play.costs else {
        panic!("ordered costs");
    };
    steps.first_mut().unwrap().instruction.action = payment::pp(0);
    WellFormedCardProgram::check(unused).unwrap_err();

    let mut ordinary = fanfare::expanded::tree().clone();
    ordinary["cards"]["BP08-038"]["abilities"][0]["fanfare"] = json!({"do":[{"choice":{"n":1_u8,"options":[
        [{"pay":{"cost":{"pp":1_u8},"do":[{"draw":{"n":1_u8}}]}}],
        [{"draw":{"n":0_u8}}]
    ]}}]});
    lower(&ordinary).unwrap();
    ordinary["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"][0]["choice"]["options"][0][0]
        ["pay"]["cost"] = cost(1, false);
    assert!(
        lower(&ordinary).is_err(),
        "the generic Pay boundary cannot lower mandatory revelation"
    );
}
