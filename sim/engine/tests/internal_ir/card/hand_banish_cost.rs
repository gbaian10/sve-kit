#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate private material payment scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{DecisionView, InputDomain};

#[test]
fn a_single_private_hand_banishment_lowers_as_a_fanfare_cost() {
    let tree = evolution_linked_movement::tree(&[(
        "fanfare",
        json!({
            "cost":{"banish":{"sel":{"who":"self","zone":"hand","kind":"spell"},"n":1}},
            "do":[{"draw":{"n":1}}]
        }),
    )]);
    lowerer().lower(&tree).unwrap();
}

#[test]
fn a_formal_mode_owns_one_optional_private_hand_banishment_cost() {
    let tree = evolution_linked_movement::tree(&[(
        "evolve_time",
        json!({"do":[{"choice":{"n":1,"options":[
            [{"pay":{"cost":{"banish":{"sel":{"who":"self","zone":"hand"},"n":1}},"do":[{"draw":{"n":1}}]}}],
            [{"life":{"who":"self","delta":1}}]
        ]}}]}),
    )]);
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "hand-banish-mode",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
}

fn payment_program(zone: &str, modal: bool) -> WellFormedCardProgram {
    let mut material = json!({"sel":{"who":"self","zone":zone,"kind":"spell"},"n":1});
    if zone != "hand" {
        material["sel"]["kind"] = json!("follower");
        material["sel"]["other"] = json!(true);
    }
    let fee = json!({"banish":material});
    let body = if modal {
        json!({"cost":{"pp":0},"do":[{"choice":{"n":1,"options":[
            [{"pay":{"cost":fee,"do":[{"draw":{"n":1}}]}}],
            [{"life":{"who":"self","delta":1}}]
        ]}}]})
    } else {
        json!({"cost":fee,"do":[{"draw":{"n":1}}]})
    };
    lowerer()
        .lower(&evolution_linked_movement::tree(&[("activate", body)]))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn initial() -> Value {
    let mut initial = setup(1, json!([{"id":"draw","card":"unit-spell"}]));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"fuel","card":"unit-spell"},{"id":"wrong","card":"unit-follower"}]);
    initial
}

fn board(program: &WellFormedCardProgram, initial: &Value) -> Game {
    let mut game = Game::new(
        catalog(),
        initial,
        &Value::Null,
        &Value::Null,
        "hand-banish-cost",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("payment question");
    };
    q
}

fn reject_invalid_material_answers(game: &mut Game, q: &DecisionView) {
    let before = format!("{game:?}");
    for handles in [vec![], vec![0, 0], vec![u64::MAX]] {
        game.answer_internal_card(q.actor, q.token, Answer::CostMaterials(handles))
            .unwrap_err();
        assert!(
            format!("{game:?}") == before,
            "invalid selections leave every authority field unchanged"
        );
    }
    game.answer_internal_card(
        PlayerId { seat: 2 },
        q.token,
        Answer::CostMaterials(vec![0]),
    )
    .unwrap_err();
    assert!(
        format!("{game:?}") == before,
        "the other player cannot spend private materials"
    );
}

#[test]
fn private_banish_payment_selects_once_is_atomic_and_precedes_play_and_draw() {
    let program = payment_program("hand", false);
    let mut game = Game::new(
        catalog(),
        &initial(),
        &Value::Null,
        &Value::Null,
        "hand-cost-observer",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    let observer = equipment_played::tree(
        &json!({"auto":{
            "on":{"any_of":[
            {"discarded":{"ref":"this","from":"hand","who":"self"}},
            {"banished":{"ref":"this","from":"hand","who":"self"}}
        ]},
            "do":[{"life":{"who":"self","delta":1}}]
        }}),
        "unit-spell",
    );
    game.load_internal_card(
        "unit-spell",
        &lowerer()
            .lower(&observer)
            .unwrap()
            .remove("unit-spell")
            .unwrap(),
    )
    .unwrap();
    let q = question(
        game.activate_internal_card(
            "source",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    let InputDomain::CostMaterials(domain) = &q.domain else {
        panic!("private hand materials");
    };
    assert_eq!(domain.members.len(), 1);
    assert!(game.internal_decision(View::P2).is_none());
    assert!(game.projection(View::P2).unwrap()["objects"]["fuel"].is_null());
    reject_invalid_material_answers(&mut game, &q);
    let result = resolved(
        game.answer_internal_card(q.actor, q.token, Answer::CostMaterials(vec![0]))
            .unwrap(),
    );
    let movement = result
        .progress
        .facts
        .iter()
        .position(|fact| matches!(fact, Fact::Movement(m) if m.method == MoveMethod::Banish))
        .unwrap();
    let played = result
        .progress
        .facts
        .iter()
        .position(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
        .unwrap();
    let drawn = result
        .progress
        .facts
        .iter()
        .position(|fact| matches!(fact, Fact::Draw(_)))
        .unwrap();
    assert!(movement < played && played < drawn);
    assert!(
        !result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Discard(_)))
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["banish"], json!(["fuel"]));
    assert_eq!(state["P1"]["hand"], json!(["wrong", "draw"]));
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("the banished card observes its real hand departure");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "unit-spell");
    resolved(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        21_u8
    );
}

#[test]
fn a_hand_banish_mode_remains_selectable_when_empty_and_decline_does_not_ask_for_materials() {
    for has_hand in [false, true] {
        for accept in [false, true] {
            let program = payment_program("hand", true);
            let mut initial = initial();
            if !has_hand {
                initial["players"]["P1"]["zones"]["hand"] = json!([]);
            }
            let mut game = board(&program, &initial);
            let origin = game.projection(View::Referee).unwrap();
            let q = question(
                game.activate_internal_card(
                    "source",
                    program.card().abilities[0].node.id,
                    PlayerId { seat: 1 },
                )
                .unwrap(),
            );
            let InputDomain::Modes { modes, .. } = &q.domain else {
                panic!("formal mode declaration");
            };
            assert!(
                modes.contains(&ModeId(1)),
                "an optional hand fee does not make the mode illegal when empty"
            );
            let optional = question(
                game.answer_internal_card(q.actor, q.token, Answer::Modes(vec![ModeId(1)]))
                    .unwrap(),
            );
            assert!(matches!(optional.domain, InputDomain::OptionalCost));
            if accept && !has_hand {
                game.answer_internal_card(
                    optional.actor,
                    optional.token,
                    Answer::OptionalCost(true),
                )
                .unwrap_err();
                assert!(
                    game.projection(View::Referee).unwrap() == origin,
                    "an unaffordable accepted fee cancels the whole activation before any effect"
                );
                assert!(game.internal_decision(View::P1).is_none());
                continue;
            }
            let pay = accept && has_hand;
            let next = game
                .answer_internal_card(optional.actor, optional.token, Answer::OptionalCost(pay))
                .unwrap();
            if pay {
                let materials = question(next);
                assert!(matches!(materials.domain, InputDomain::CostMaterials(_)));
                resolved(
                    game.answer_internal_card(
                        materials.actor,
                        materials.token,
                        Answer::CostMaterials(vec![0]),
                    )
                    .unwrap(),
                );
            } else {
                resolved(next);
            }
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["banish_count"], usize::from(pay));
            assert_eq!(state["P1"]["deck_count"], usize::from(!pay));
        }
    }
}

#[test]
fn public_field_ex_and_cemetery_banish_fees_keep_their_original_payment_route() {
    for zone in ["field", "ex", "cemetery"] {
        let program = payment_program(zone, false);
        let mut initial = initial();
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        if zone == "field" {
            initial["players"]["P1"]["zones"][zone]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"fuel","card":"unit-follower"}));
        } else {
            initial["players"]["P1"]["zones"][zone] = json!([{"id":"fuel","card":"unit-follower"}]);
        }
        let mut game = board(&program, &initial);
        let q = question(
            game.activate_internal_card(
                "source",
                program.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap(),
        );
        assert!(matches!(q.domain, InputDomain::CostMaterials(_)));
        let result = resolved(
            game.answer_internal_card(q.actor, q.token, Answer::CostMaterials(vec![0]))
                .unwrap(),
        );
        assert!(
            result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact,Fact::Movement(m) if m.method==MoveMethod::Banish))
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["banish"],
            json!(["fuel"])
        );
    }
}

#[test]
fn hand_banish_fees_reject_other_players_counts_results_and_spell_additional_costs() {
    for material in [
        json!({"sel":{"who":"opp","zone":"hand"},"n":1}),
        json!({"sel":{"who":"self","zone":"hand"},"n":2}),
        json!({"sel":{"who":"self","zone":"hand"},"upto":1}),
        json!({"sel":{"who":"self","zone":"hand"},"any":true}),
        json!({"sel":{"who":"self","zone":"hand"},"n":1,"result":"paid"}),
    ] {
        lowerer()
            .lower(&evolution_linked_movement::tree(&[(
                "fanfare",
                json!({"cost":{"banish":material},"do":[]}),
            )]))
            .unwrap_err();
    }
    let mut expanded = evolution_linked_movement::tree(&[
        (
            "play",
            json!({"additional_cost":{"banish":{"sel":{"who":"self","zone":"hand"},"n":1}}}),
        ),
        ("spell", json!({"do":[]})),
    ]);
    let entry = expanded["cards"]["unit-follower"].take();
    expanded["cards"] = json!({"unit-spell":entry});
    lowerer()
        .lower_with_printed_costs(
            &expanded,
            &BTreeMap::from([("unit-spell".to_owned(), Nat(1))]),
        )
        .unwrap_err();
}

#[test]
fn the_cost_material_binding_is_retired_before_the_body() {
    let mut program = payment_program("hand", false).card().clone();
    let ability = &mut program.abilities[0];
    let ChoiceDomain::Objects { binding, .. } =
        ability.node.play.as_ref().unwrap().parameters[0].domain
    else {
        panic!("private material producer");
    };
    let mut leak = damage(
        99,
        RecipientSet::Objects {
            objects: Selector::Bound { reference: binding },
        },
        1,
    );
    let Node::Instruction { instruction } = &mut leak.node else {
        panic!("damage instruction");
    };
    instruction.source = ability.node.source.clone();
    leak.source = ability.source.clone();
    ability.node.body.as_mut().unwrap().steps.push(leak);
    assert_eq!(
        WellFormedCardProgram::check(program).unwrap_err().reason,
        WellFormedReason::UnavailableBinding
    );
}

fn modal_tree(head: &str, count: u8) -> Value {
    let mut body = json!({"do":[{"choice":{"n":count,"options":[
        [{"pay":{"cost":{"banish":{"sel":{"who":"self","zone":"hand"},"n":1}},"do":[{"draw":{"n":1}}]}}],
        [{"life":{"who":"self","delta":1}}]
    ]}}]});
    if head == "activate" {
        body["cost"] = json!({"pp":0});
    }
    let mut expanded = evolution_linked_movement::tree(&[(head, body)]);
    if head == "spell" {
        let entry = expanded["cards"]["unit-follower"].take();
        expanded["cards"] = json!({"unit-spell":entry});
    }
    expanded
}

#[test]
fn private_hand_banish_modes_do_not_open_spell_entries() {
    let expanded = modal_tree("spell", 1);
    let mut control = expanded.clone();
    control["cards"]["unit-spell"]["abilities"][0]["spell"]["do"][0]["choice"]["options"][0] =
        json!([{"life":{"who":"self","delta":2}}]);
    let printed_costs = BTreeMap::from([("unit-spell".to_owned(), Nat(1))]);
    let control_program = lowerer()
        .lower_with_printed_costs(&control, &printed_costs)
        .unwrap()
        .remove("unit-spell")
        .unwrap();
    let mut game = Game::new(
        catalog(),
        &initial(),
        &Value::Null,
        &Value::Null,
        "closed-spell-mode-fee",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &control_program)
        .unwrap();
    let program = lowerer()
        .lower_with_printed_costs(&expanded, &printed_costs)
        .unwrap()
        .remove("unit-spell")
        .unwrap();
    let mut rejected = Game::new(
        catalog(),
        &initial(),
        &Value::Null,
        &Value::Null,
        "private-spell-mode-fee",
    )
    .unwrap();
    rejected
        .load_internal_card("unit-spell", &program)
        .unwrap_err();
}

#[test]
fn private_hand_banish_modes_do_not_open_multiple_choice_entries() {
    let program = lowerer()
        .lower(&modal_tree("activate", 2))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut game = Game::new(
        catalog(),
        &initial(),
        &Value::Null,
        &Value::Null,
        "closed-multiple-mode-fee",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program)
        .unwrap_err();
}

#[test]
fn private_hand_banish_modes_do_not_open_quick_entries() {
    let mut expanded = modal_tree("activate", 1);
    expanded["cards"]["unit-follower"]["abilities"][0]["activate"]["quick"] = json!(true);
    let program = lowerer()
        .lower(&expanded)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut game = Game::new(
        catalog(),
        &initial(),
        &Value::Null,
        &Value::Null,
        "closed-quick-mode-fee",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program)
        .unwrap_err();
}
