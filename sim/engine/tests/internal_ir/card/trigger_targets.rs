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
fn resolution_choices_already_support_signed_stat_modification() {
    lowerer()
        .lower(&tree(
            "attack_time",
            json!({"do":[
                {"choose":{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}},
                {"buff":{"on":"enemy","power":-1}}
            ]}),
        ))
        .unwrap();
}

#[test]
fn ordinary_trigger_targets_admit_signed_modification_and_banish() {
    for (head, action) in [
        ("attack_time", json!({"buff":{"on":"enemy","power":-1}})),
        ("evolve_time", json!({"banish":"enemy"})),
    ] {
        lowerer()
            .lower(&tree(
                head,
                json!({
                    "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
                    "do":[action]
                }),
            ))
            .unwrap();
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
        lowerer().lower(&tree("attack_time", body)).err().unwrap();
    }
    let body = json!({"targets":[target],"do":[{"buff":{"on":"enemy","power":-1}}]});
    lowerer().lower(&tree("lastword", body)).err().unwrap();
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

#[test]
fn required_head_cost_targets_keep_the_existing_trigger_scope() {
    let body = json!({
        "cost":{"pp":1},
        "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
        "do":[{"damage":{"to":"enemy","n":2}}]
    });
    for head in ["fanfare", "evolve_time", "super_evolve_time", "attack_time"] {
        lowerer().lower(&tree(head, body.clone())).unwrap();
    }
    lowerer()
        .lower(&tree("lastword", body.clone()))
        .err()
        .unwrap();
    let mut unpaid = body;
    unpaid.as_object_mut().unwrap().remove("cost");
    lowerer()
        .lower(&tree("super_evolve_time", unpaid))
        .err()
        .unwrap();
}
