use super::*;
use sve_engine::game::internal::InputDomain;

fn tree(head: &str, body: Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry[head] = body;
    tree["cards"]["BP08-038"]["abilities"] = json!([entry]);
    tree
}

#[test]
fn every_trigger_head_lowers_targets_and_payments() {
    let target =
        json!({"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1});
    for (head, subscription) in [
        ("fanfare", Value::Null),
        ("lastword", Value::Null),
        ("evolve_time", Value::Null),
        ("super_evolve_time", Value::Null),
        ("attack_time", Value::Null),
        ("auto", json!({"phase":{"whose":"self","which":"end"}})),
        ("auto", json!({"damaged":{"recipient":"this"}})),
        ("auto", json!({"left_field":{"ref":"this"}})),
        ("auto", json!({"enter_field":{"ref":"this"}})),
    ] {
        for mut body in [
            json!({"targets":[target],"do":[{"buff":{"on":"enemy","power":-1}}]}),
            json!({"cost":{"pp":1},"do":[{"life":{"who":"self","delta":1}}]}),
            json!({"cost":{"pp":1},"targets":[target],"do":[{"banish":"enemy"}]}),
            json!({"optional_cost":{"pp":1,"result":"paid"},"targets":[target],"do":[{"if":{"cond":{"paid":{"receipt":"paid"}},"then":[{"destroy":"enemy"}]}}]}),
            json!({"do":[{"choose":{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}},{"buff":{"on":"enemy","power":-1}}]}),
        ] {
            if !subscription.is_null() {
                body["on"] = subscription.clone();
            }
            lowerer().lower(&tree(head, body)).unwrap();
        }
    }
}

#[test]
fn ordinary_target_support_keeps_unrelated_play_constructs_closed() {
    let target =
        json!({"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1});
    let mut modal_target = target.clone();
    modal_target["option"] = json!(1);
    for body in [
        json!({"targets":[modal_target],"do":[{"choice":{"n":1,"options":[[{"buff":{"on":"enemy","power":1}}],[{"draw":{"n":1}}]]}}]}),
        json!({"targets":[target],"do":[{"damage":{"to":"enemy","n":2,"distribute":true}}]}),
        json!({"targets":[target],"do":[{"buff":{"on":"enemy","power":{"at":"resolution","value":{"count":{"who":"self","zone":"cemetery"}}}}}]}),
    ] {
        let mut activated = body.clone();
        activated["cost"] = json!({"pp":1});
        lowerer().lower(&tree("activate", activated)).unwrap();
        for head in ["attack_time", "super_evolve_time", "lastword"] {
            lowerer().lower(&tree(head, body.clone())).err().unwrap();
        }
    }
}

#[test]
fn ordinary_fanfare_commits_only_after_its_target_is_chosen() {
    let mut expanded = tree(
        "fanfare",
        json!({
            "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
            "do":[{"buff":{"on":"enemy","power":-1}}]
        }),
    );
    expanded["cards"] = json!({"unit-follower":expanded["cards"]["BP08-038"]});
    let typed = lowerer()
        .lower(&expanded)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "ordinary-targets",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &typed).unwrap();
    assert!(matches!(
        game.play_internal_card("s", PlayerId { seat: 1 }).unwrap(),
        CardPlay::Entered(_)
    ));
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let CardPlay::Awaiting(targets) = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(1))
        .unwrap()
    else {
        panic!("target declaration")
    };
    assert!(matches!(targets.domain, InputDomain::Targets(_)));
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["power"],
        2
    );
    let result = resolved(
        game.answer_internal_card(targets.actor, targets.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["power"],
        1
    );
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
            .count(),
        1
    );
}

fn program(head: &str, body: Value) -> WellFormedCardProgram {
    let mut expanded = tree(head, body);
    expanded["cards"] = json!({"unit-follower":expanded["cards"]["BP08-038"]});
    lowerer()
        .lower(&expanded)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

#[test]
fn phase_targets_precede_head_payment_and_failed_declarations_cancel() {
    let program = program(
        "auto",
        json!({
            "on":{"phase":{"whose":"self","which":"end"}},
            "cost":{"pp":1},
            "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
            "do":[{"damage":{"to":"enemy","n":1}}]
        }),
    );
    for (accept, has_target) in [(true, true), (false, true), (true, false)] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["pp"] = json!({"current":3,"max":3});
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        if !has_target {
            initial["players"]["P2"]["zones"]["field"] = json!([]);
        }
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "phase-targets",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program).unwrap();
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let result = game
            .answer_internal_card(pending.actor, pending.token, Answer::Pending(1))
            .unwrap();
        let CardPlay::Awaiting(payment) = result else {
            panic!("head declaration")
        };
        assert_eq!(payment.domain, InputDomain::OptionalCost);
        let result = game
            .answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(accept))
            .unwrap();
        if accept && has_target {
            let CardPlay::Awaiting(targets) = result else {
                panic!("targets before payment")
            };
            assert!(matches!(targets.domain, InputDomain::Targets(_)));
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                3
            );
            resolved(
                game.answer_internal_card(targets.actor, targets.token, Answer::Targets(vec![0]))
                    .unwrap(),
            );
        } else {
            assert!(matches!(result, CardPlay::Cancelled));
        }
        if has_target {
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["f"]["damage"],
                u8::from(accept)
            );
        }
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            3 - u8::from(accept && has_target)
        );
    }
}

#[test]
fn damaged_targets_are_declared_by_the_controller_on_the_opponents_turn() {
    let program = program(
        "auto",
        json!({
            "on":{"damaged":{"recipient":"this"}},
            "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
            "do":[{"buff":{"on":"enemy","power":-1}}]
        }),
    );
    let mut initial = setup(1, json!([]));
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower","state":{"acted":true}}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"f","card":"unit-follower","state":{"entered_this_turn":false}}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "opponent-damage",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    game.internal_rule_action(&json!({"do":"attack","attacker":"f","target":"s"}))
        .unwrap();
    quick::pass(&mut game);
    let pending = game.internal_pending_choice().unwrap().unwrap();
    assert_eq!(pending.actor, PlayerId { seat: 2 });
    let CardPlay::Awaiting(active_targets) = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(1))
        .unwrap()
    else {
        panic!("active player targets")
    };
    resolved(
        game.answer_internal_card(
            active_targets.actor,
            active_targets.token,
            Answer::Targets(vec![0]),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let CardPlay::Awaiting(targets) = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(1))
        .unwrap()
    else {
        panic!("opponent turn targets")
    };
    assert_eq!(targets.actor, PlayerId { seat: 1 });
    resolved(
        game.answer_internal_card(targets.actor, targets.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["f"]["power"],
        1
    );
}

#[test]
fn ex_follower_phase_is_rejected_before_play() {
    let program = program(
        "auto",
        json!({"zone":"ex", "on":{"phase":{"whose":"self","which":"end"}}, "do":[]}),
    );
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "ex-follower",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program)
        .unwrap_err();
}
