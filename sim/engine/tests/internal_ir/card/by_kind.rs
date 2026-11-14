use super::*;
use alloc::collections::BTreeMap;

/// A spell choosing `target` and running `steps`, lowered through the author pipeline.
fn spell(target: &Value, steps: &Value) -> Result<WellFormedCardProgram, lowering::LoweringError> {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["spell"] = json!({"targets":[target],"do":steps});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-spell":{"meta":meta,"abilities":[entry]}});
    lowerer()
        .lower_with_printed_costs(&tree, &BTreeMap::from([("unit-spell".to_owned(), Nat(1))]))
        .map(|mut cards| cards.remove("unit-spell").unwrap())
}

fn leader_or(count: &str) -> Value {
    json!({"bind":"t","from":{"who":"self","zone":"field","kind":"follower","leader_or":true},count:1})
}

fn by_kind(name: Option<&str>) -> Value {
    let item = name.unwrap_or("item");
    let mut step = json!({"ref":"t","leader":[{"life":{"who":item,"delta":2}}],
        "follower":[{"buff":{"on":item,"hp":2}}]});
    if let Some(name) = name {
        step["as"] = json!(name);
    }
    json!({"by_kind":step})
}

#[test]
fn by_kind_narrows_a_single_leader_or_follower_target_under_item_or_its_own_name() {
    for name in [None, Some("chosen")] {
        let program = spell(&leader_or("n"), &json!([by_kind(name)])).unwrap();
        assert!(
            program.card().abilities[0]
                .node
                .body
                .as_ref()
                .unwrap()
                .steps
                .iter()
                .any(|step| matches!(
                    step.node,
                    Node::Control {
                        control: Control::Match { .. }
                    }
                )),
            "the branches lower to one recipient match"
        );
    }
}

#[test]
fn by_kind_needs_exactly_one_chosen_recipient_and_keeps_its_names_local() {
    // Controls: each shape lowers without the by_kind step.
    spell(
        &json!({"bind":"t","from":{"who":"self","zone":"field","kind":"follower"},"n":1}),
        &json!([{"buff":{"on":"t","hp":2}}]),
    )
    .unwrap();
    spell(&leader_or("upto"), &json!([{"damage":{"to":"t","n":1}}])).unwrap();
    // An ordinary object target has no leader branch to narrow.
    spell(
        &json!({"bind":"t","from":{"who":"self","zone":"field","kind":"follower"},"n":1}),
        &json!([by_kind(None)]),
    )
    .unwrap_err();
    // An optional choice may hold no recipient, so it has no single kind.
    spell(&leader_or("upto"), &json!([by_kind(None)])).unwrap_err();
    // The narrowed name ends with its branch.
    spell(
        &leader_or("n"),
        &json!([by_kind(None), {"life":{"who":"item","delta":1}}]),
    )
    .unwrap_err();
    // A leader branch cannot treat the leader as a follower.
    spell(
        &leader_or("n"),
        &json!([{"by_kind":{"ref":"t","leader":[{"buff":{"on":"item","hp":2}}],"follower":[]}}]),
    )
    .unwrap_err();
}
