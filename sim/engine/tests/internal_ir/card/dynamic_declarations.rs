use super::*;
use sve_engine::game::internal::InputDomain;
use sve_engine::ir::lowering::LoweringError;

fn lower_body(body: &Value) -> Result<WellFormedCardProgram, LoweringError> {
    let mut tree = fanfare::expanded::tree().clone();
    let mut ability = tree["cards"]["BP08-038"]["abilities"][0].clone();
    ability["fanfare"] = body.clone();
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[ability]}});
    Ok(lowerer().lower(&tree)?.remove("unit-follower").unwrap())
}
#[test]
fn dynamic_numeric_declarations_reject_hidden_foreign_and_source_dependent_reads() {
    for number in [
        json!({"count":{"who":"self","zone":"hand"}}),
        json!({"count":{"who":"self","zone":"deck"}}),
        json!({"count":{"who":"opp","zone":"field"}}),
        json!({"resource":{"who":"opp","kind":"pp_max"}}),
        json!({"hp_of":"this"}),
    ] {
        for field in ["cost_le", "orig_cost_le"] {
            let mut from = json!({"who":"opp","zone":"field","kind":"follower"});
            from[field] = number.clone();
            lower_body(&json!({"targets":[{"bind":"target","from":from,"n":1}],"do":[{"destroy":{"ref":"target"}}]})).unwrap_err();
            let mut sel = json!({"who":"self","zone":"deck","kind":"follower"});
            sel[field] = number.clone();
            lower_body(&json!({"do":[{"search":{"sel":sel,"n":1,"to":"hand"}}]})).unwrap_err();
        }
        for field in ["n", "upto"] {
            let mut target =
                json!({"bind":"target","from":{"who":"opp","zone":"field","kind":"follower"}});
            target[field] = number.clone();
            lower_body(&json!({"targets":[target],"do":[{"destroy":{"ref":"target"}}]}))
                .unwrap_err();
        }
    }
}
#[test]
fn exact_and_upper_public_target_counts_are_frozen_in_the_formal_question() {
    for field in ["n", "upto"] {
        let mut target =
            json!({"bind":"target","from":{"who":"opp","zone":"field","kind":"follower"}});
        target["from"][if field == "n" {
            "orig_cost_le"
        } else {
            "cost_le"
        }] = json!({"count":{"who":"self","zone":"ex"}});
        target[field] =
            json!({"count":{"who":"self","zone":"field","kind":"follower","other":true}});
        let program =
            lower_body(&json!({"targets":[target],"do":[{"destroy":{"ref":"target"}}]})).unwrap();
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"extra","card":"unit-follower"}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"unit-follower"}]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"unit-follower"}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy1","card":"unit-follower"},{"id":"enemy2","card":"unit-follower"}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "numeric-targets",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program).unwrap();
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("printed fanfare");
        };
        let CardPlay::Awaiting(question) = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap()
        else {
            panic!("formal target count");
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("public object targets");
        };
        assert_eq!(
            domain.cardinality,
            Cardinality {
                minimum: Nat(u64::from(field == "n")),
                maximum: Some(Nat(1))
            },
            "only the other own follower determines the quantity"
        );
        let before = game.projection(View::Referee).unwrap();
        assert!(
            game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0, 1]))
                .is_err(),
            "two targets exceed the evaluated count"
        );
        if field == "n" {
            assert!(
                game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![]))
                    .is_err(),
                "an exact count also fixes its minimum"
            );
        }
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            before,
            "invalid cardinalities preserve the authority state"
        );
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["cemetery"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}
#[test]
fn hand_written_dynamic_count_constraints_cannot_hide_a_hand_read() {
    let program=lower_body(&json!({"targets":[{"bind":"target","from":{"who":"opp","zone":"field","kind":"follower"},"upto":{"count":{"who":"self","zone":"field"}}}],"do":[{"destroy":{"ref":"target"}}]})).unwrap();
    let mut bad = program.card().clone();
    let ChoiceDomain::Objects { constraint, .. } =
        &mut bad.abilities[0].node.play.as_mut().unwrap().parameters[0].domain
    else {
        panic!("existing object constraint");
    };
    let Expr::Compute(operation) = &mut constraint.aggregate_bounds[0].value else {
        panic!("signed count");
    };
    let IntOperation::FromNat {
        value: Expr::Live(read),
    } = operation.as_mut()
    else {
        panic!("live public count");
    };
    let NatRead::ZoneCount {
        selector: Selector::Zone { zone, .. },
    } = read.as_mut()
    else {
        panic!("zone selector");
    };
    *zone = ZoneKind::Hand;
    assert!(
        WellFormedCardProgram::check(bad).is_err(),
        "lowerer bypass cannot grant a hidden numeric declaration"
    );
}

#[test]
fn mode_targets_keep_arbitrary_player_captures_closed() {
    let program=lower_body(&json!({"targets":[{"option":1,"bind":"target","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],"do":[{"choice":{"n":1,"options":[[{"damage":{"to":{"any_of":["target",{"leader_of":"target"}]},"n":2}}],[{"draw":{"n":1}}]]}}]})).unwrap();
    let mut valid = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "mode-controller-positive",
    )
    .unwrap();
    valid.load_internal_card("unit-follower", &program).unwrap();
    let mut bad = program.card().clone();
    let Node::Control {
        control: Control::Mode { modes, .. },
    } = &mut bad.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("formal modes");
    };
    let scope = modes[0].body.scope;
    let parameter = &mut modes[0].parameters[0];
    let ChoiceDomain::Objects { binding, .. } = &parameter.domain else {
        panic!("public target");
    };
    parameter.captures.push(Capture::Player {
        at: CaptureTime::AtSelection {
            decision: parameter.id,
        },
        value: Expr::Live(Box::new(PlayerRead::Role {
            object: ObjectView::Selected {
                selection: *binding,
            },
            role: Role::Owner,
        })),
        into: CaptureRef {
            binding: Binding::new(999, scope),
        },
    });
    let program = WellFormedCardProgram::check(bad).unwrap();
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "mode-capture-boundary",
    )
    .unwrap();
    assert!(
        game.load_internal_card("unit-follower", &program).is_err(),
        "only the selected target controller capture is admitted inside modes"
    );
}

#[test]
fn mode_recipient_targets_keep_selection_captures_closed() {
    let program = lower_body(&json!({"targets":[{"option":1,"bind":"target","from":{"who":"opp","zone":"field","kind":"follower","leader_or":true},"n":1}],"do":[{"choice":{"n":1,"options":[[{"damage":{"to":"target","n":2}}],[{"draw":{"n":1}}]]}}]})).unwrap();
    let mut valid = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "mode-recipient-positive",
    )
    .unwrap();
    valid.load_internal_card("unit-follower", &program).unwrap();
    let mut bad = program.card().clone();
    let Node::Control {
        control: Control::Mode { modes, .. },
    } = &mut bad.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("formal modes")
    };
    let scope = modes[0].body.scope;
    let parameter = &mut modes[0].parameters[0];
    assert!(matches!(parameter.domain, ChoiceDomain::Recipients { .. }));
    parameter.captures.push(Capture::Player {
        at: CaptureTime::AtSelection {
            decision: parameter.id,
        },
        value: Expr::Live(Box::new(PlayerRead::Active {})),
        into: CaptureRef {
            binding: Binding::new(999, scope),
        },
    });
    let program = WellFormedCardProgram::check(bad).unwrap();
    let mut rejected = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "mode-recipient-capture-boundary",
    )
    .unwrap();
    assert!(
        rejected
            .load_internal_card("unit-follower", &program)
            .is_err(),
        "recipient mode support cannot admit arbitrary selection captures"
    );
}
