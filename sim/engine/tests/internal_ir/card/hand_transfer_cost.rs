use super::*;
use alloc::collections::BTreeMap;
use sve_engine::EngineFailure;
use sve_engine::game::internal::InputDomain;

fn position(operation: &str, full_ex: bool) -> (Game, AbilityId) {
    let tree = evolution_linked_movement::tree(&[(
        "activate",
        json!({
            "cost":{"steps":[{"pp":1},{"act_self":true},{operation:{"sel":{"who":"self","zone":"hand","kind":"spell"},"n":1_u8}}]},
            "do":[{"life":{"who":"self","delta":1_i8}}]
        }),
    )]);
    let programs = lowerer().lower(&tree).unwrap();
    let program = &programs["unit-follower"];
    let mut initial = setup(1, json!([{"id":"top","card":"unit-spell"}]));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"h0","card":"unit-spell"}, {"id":"h1","card":"unit-spell"},
        {"id":"wrong","card":"unit-follower"}
    ]);
    if full_ex {
        initial["players"]["P1"]["zones"]["ex"] = json!(
            (0_u8..5)
                .map(|n| json!({"id":format!("ex{n}"),"card":"unit-follower"}))
                .collect::<Vec<_>>()
        );
    }
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "private-transfer-cost",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    (game, program.card().abilities[0].node.id)
}

#[test]
fn private_hand_transfer_selects_once_and_moves_before_the_body_without_revelation() {
    for operation in ["to_ex", "to_deck_bottom"] {
        let (mut game, ability) = position(operation, false);
        let CardPlay::Awaiting(q) = game
            .activate_internal_card("source", ability, PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("private material question");
        };
        let InputDomain::CostMaterials(domain) = &q.domain else {
            panic!("hand materials");
        };
        assert_eq!(domain.members.len(), 2);
        assert!(game.internal_decision(View::P2).is_none());
        let before = exact_authority(&game);
        for handles in [vec![], vec![0, 0], vec![99]] {
            game.answer_internal_card(q.actor, q.token, Answer::CostMaterials(handles))
                .unwrap_err();
            assert_eq!(exact_authority(&game), before);
        }
        game.answer_internal_card(
            PlayerId { seat: 2 },
            q.token,
            Answer::CostMaterials(vec![1]),
        )
        .unwrap_err();
        assert_eq!(exact_authority(&game), before);
        let result = resolved(
            game.answer_internal_card(q.actor, q.token, Answer::CostMaterials(vec![1]))
                .unwrap(),
        );
        assert!(
            !result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Discard(_)))
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["h0", "wrong"]));
        assert_eq!(state["P1"]["leader"]["life"], 21_u8);
        assert_eq!(state["objects"]["source"]["acted"], true);
        let opponent = game.projection(View::P2).unwrap();
        assert!(opponent["objects"]["h0"].is_null());
        assert!(opponent["objects"]["wrong"].is_null());
        if operation == "to_ex" {
            assert_eq!(state["P1"]["ex"], json!(["h1"]));
            assert_eq!(opponent["objects"]["h1"]["card"], "unit-spell");
        } else {
            assert_eq!(state["P1"]["deck"], json!(["top", "h1"]));
            assert!(!opponent.to_string().contains("unit-spell"));
        }
        let after = exact_authority(&game);
        game.answer_internal_card(q.actor, q.token, Answer::CostMaterials(vec![0]))
            .unwrap_err();
        assert_eq!(exact_authority(&game), after);
    }
}

#[test]
fn a_full_ex_area_cannot_pay_a_private_hand_transfer_and_changes_nothing() {
    let (mut game, ability) = position("to_ex", true);
    let before = game.projection(View::Referee).unwrap();
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("private materials")
    };
    let error = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![0]),
        )
        .unwrap_err();
    assert!(matches!(error, EngineFailure::IllegalPlay(_)));
    assert_eq!(game.projection(View::Referee).unwrap(), before);
}

#[test]
fn private_transfer_lowering_rejects_unused_cardinalities_and_result_bindings() {
    for operation in ["to_ex", "to_deck_bottom"] {
        for material in [
            json!({"sel":{"who":"opp","zone":"hand"},"n":1}),
            json!({"sel":{"who":"self","zone":"hand"},"n":2}),
            json!({"sel":{"who":"self","zone":"hand"},"upto":1}),
            json!({"sel":{"who":"self","zone":"hand"},"n":1,"result":"paid"}),
        ] {
            let tree = evolution_linked_movement::tree(&[(
                "activate",
                json!({"cost":{operation:material},"do":[]}),
            )]);
            let error = lowerer().lower(&tree).unwrap_err();
            assert_eq!(error.code, "unsupported_construct");
            assert!(error.path.starts_with("/cards/unit-follower/abilities/0"));
        }
    }
}

#[test]
fn paid_private_transfer_bindings_cannot_be_read_by_the_ability_body() {
    for operation in ["to_ex", "to_deck_bottom"] {
        for head in ["activate", "fanfare"] {
            let tree = evolution_linked_movement::tree(&[(
                head,
                json!({
                    "cost":{operation:{"sel":{"who":"self","zone":"hand"},"n":1}},"do":[]
                }),
            )]);
            let mut program = lowerer()
                .lower(&tree)
                .unwrap()
                .remove("unit-follower")
                .unwrap()
                .card()
                .clone();
            let ability = &mut program.abilities[0];
            let ChoiceDomain::Objects { binding, .. } =
                ability.node.play.as_ref().unwrap().parameters[0].domain
            else {
                panic!("private materials")
            };
            let mut leak = instruction(
                999,
                Action::Reveal {
                    objects: Selector::Bound { reference: binding },
                },
            );
            let Node::Instruction { instruction } = &mut leak.node else {
                panic!("instruction")
            };
            instruction.source = ability.node.source.clone();
            leak.source = ability.source.clone();
            ability.node.body.as_mut().unwrap().steps.push(leak);
            assert_eq!(
                WellFormedCardProgram::check(program).unwrap_err().reason,
                WellFormedReason::UnavailableBinding
            );
        }
    }
}

#[test]
fn private_hand_transfers_do_not_extend_mode_or_additional_cost_entries() {
    for operation in ["to_ex", "to_deck_bottom"] {
        let cost = json!({operation:{"sel":{"who":"self","zone":"hand"},"n":1}});
        let tree = evolution_linked_movement::tree(&[(
            "fanfare",
            json!({
                "cost":cost,"do":[{"choice":{"n":1,"options":[[{"draw":{"n":1}}],[{"life":{"who":"self","delta":1}}]]}}]
            }),
        )]);
        let error = lowerer().lower(&tree).unwrap_err();
        assert_eq!(error.code, "ill_formed");
        assert!(error.path.starts_with("/cards/unit-follower/abilities/0"));
        let mut tree = evolution_linked_movement::tree(&[
            ("play", json!({"additional_cost":cost})),
            ("spell", json!({"do":[{"draw":{"n":1}}]})),
        ]);
        let seed = tree["cards"]["unit-follower"].take();
        tree["cards"] = json!({"unit-spell":seed});
        let error = lowerer()
            .lower_with_printed_costs(&tree, &BTreeMap::from([("unit-spell".to_owned(), Nat(1))]))
            .unwrap_err();
        assert_eq!(error.code, "unsupported_construct");
        assert!(error.path.starts_with("/cards/unit-spell/abilities/0"));
    }
}
