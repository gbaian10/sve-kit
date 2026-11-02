//! Field-state filters and earlier-target exclusions reject every position that would drop them.
use super::*;
use sve_engine::game::internal::InputDomain;

fn tree(steps: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["activate"] = json!({"cost":{"pp":1},"do":steps});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    tree
}

fn choose(from: &Value) -> Value {
    json!([{"choose":{"bind":"picked","from":from,"n":1}},{"destroy":"picked"}])
}

#[test]
fn state_filters_reject_non_field_leader_and_observed_sources() {
    let field = json!({"who":"opp","zone":"field","kind":"follower","acted":true});
    lowerer().lower(&tree(&choose(&field))).unwrap();
    for from in [
        json!({"who":"self","zone":"cemetery","kind":"follower","acted":true}),
        json!({"who":"self","zone":"ex","kind":"follower","evolved":false}),
        json!({"who":"opp","zone":"field","kind":"follower","leader_or":true,"entered_this_turn":true}),
    ] {
        assert!(
            lowerer().lower(&tree(&choose(&from))).is_err(),
            "{from} has no field-only object to read"
        );
    }
    let observed = json!([
        {"look":{"top":{"who":"self","n":2},"bind":"seen"}},
        {"choose":{"bind":"picked","from":{"in":"seen","evolved":false},"n":1,"may":true}}
    ]);
    assert!(
        lowerer().lower(&tree(&observed)).is_err(),
        "a looked card is not on the field"
    );
}

#[test]
fn zone_exclusions_need_an_earlier_public_target_and_no_extremum() {
    let target = json!({"choose":{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}});
    let sweep = |except: Value| json!({"damage":{"n":1,"to":{"who":"opp","zone":"field","kind":"follower","except":except}}});
    lowerer()
        .lower(&tree(&json!([target, sweep(json!({"in":"enemy"}))])))
        .unwrap();
    assert!(
        lowerer()
            .lower(&tree(&json!([target, sweep(json!({"in":"missing"}))])))
            .is_err(),
        "an unknown binding is not an earlier target"
    );
    let observed = json!([
        {"look":{"top":{"who":"self","n":2},"bind":"seen"}},
        {"choose":{"bind":"picked","from":{"in":"seen"},"n":1,"may":true}},
        sweep(json!({"in":"picked"}))
    ]);
    assert!(
        lowerer().lower(&tree(&observed)).is_err(),
        "a private observed choice cannot shape a public sweep"
    );
    for extra in [json!({"acted":true}), json!({"except":{"in":"enemy"}})] {
        let mut lowest = json!({"who":"opp","zone":"field","kind":"follower","where":{"cmp":{
            "left":{"cost_of":"item"},"op":"eq","right":{"bound":"lowest"}
        }}});
        for (key, value) in extra.as_object().unwrap() {
            lowest[key] = value.clone();
        }
        let mut program = tree(&json!([target, {"destroy":{"ref":lowest}}]));
        program["cards"]["unit-follower"]["abilities"][0]["activate"]["define"] = json!({"lowest":{"agg":{"of":{"who":"opp","zone":"field","kind":"follower"},"attr":"cost","fn":"min"}}});
        assert!(
            lowerer().lower(&program).is_err(),
            "{extra} would be dropped from the extremum population"
        );
    }
}

#[test]
fn evolved_filter_matches_only_evolved_followers_never_amulets() {
    let programs = lowerer()
        .lower(&tree(&choose(
            &json!({"who":"opp","zone":"field","evolved":true}),
        )))
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"base","card":"unit-follower"},
        {"id":"charm","card":"unit-amulet"},
        {"id":"evolved","card":"unit-follower","state":{"evolved":true}},
        {"id":"super","card":"unit-follower","state":{"evolved":true,"super_evolved":true}}
    ]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "evolved-filter",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("evolved candidates")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("public choice")
    };
    assert_eq!(
        domain.members.len(),
        2_usize,
        "evolved and super-evolved followers qualify; the base follower and amulet do not"
    );
    assert!(
        domain
            .members
            .iter()
            .all(|member| member.card.as_deref() == Some("unit-follower")),
        "an amulet is never an evolved follower"
    );
}
