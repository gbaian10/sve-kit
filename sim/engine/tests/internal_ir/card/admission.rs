//! Whole-card runtime guards for synthetic lowering fixtures.

use super::*;
use alloc::collections::BTreeMap;
use std::env::var_os;
use std::fs::read_to_string;
use sve_engine::EngineFailure;

fn runtime_support(tree: &Value, snapshot: &str) -> BTreeMap<String, Value> {
    let printings = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .map(|card| (card["number"].as_str().unwrap().to_owned(), card))
        .collect::<BTreeMap<_, _>>();
    let costs = printings
        .iter()
        .filter_map(|(key, card)| {
            card["faces"][0]["cost"]
                .as_str()
                .and_then(|cost| cost.parse().ok())
                .map(|cost| (key.clone(), Nat(cost)))
        })
        .collect();
    let lowerer = lowerer();
    tree["cards"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, card)| {
            let selected = json!({
                "format":tree["format"], "kind":tree["kind"], "scope":tree["scope"],
                "cards":{key:card}
            });
            let runtime = match lowerer.lower_with_printed_costs(&selected, &costs) {
                Err(failure) => {
                    json!({"status":"rejected", "code":failure.code, "path":failure.path})
                }
                Ok(_) if !printings.contains_key(key) => {
                    json!({"status":"rejected", "reason":"snapshot_printing_missing"})
                }
                Ok(cards) => {
                    let mut game = fanfare::fragment_position(1, key, 10, json!([]));
                    match game.load_internal_card(key, &cards[key]) {
                        Ok(()) => json!({"status":"supported"}),
                        Err(failure) => json!({"status":"rejected", "reason":failure.to_string()}),
                    }
                }
            };
            (key.clone(), runtime)
        })
        .collect()
}

#[test]
fn expanded_fixtures_obey_whole_card_runtime_guards() {
    let snapshot = read_to_string(var_os("SVE_TEST_SNAPSHOT").unwrap()).unwrap();
    let mut tree = fanfare::expanded::spell_tree().clone();
    tree["cards"]["BP16-U06"] = tree["cards"]["BP01-041"].clone();
    let reports = runtime_support(&tree, &snapshot);
    assert_eq!(reports["BP03-002"]["status"], "supported");
    assert_eq!(reports["SD03-005"]["status"], "supported");
    assert_eq!(reports["BP01-041"]["status"], "supported");
    assert_eq!(reports["BP16-U06"]["status"], "rejected");
    assert_eq!(
        reports["BP16-U06"]["reason"],
        "unsupported: internal D1 does not support amulet with printed combat statistics"
    );
}

#[test]
fn unknown_printed_cost_rejects_even_an_empty_card_but_zero_is_known() {
    let initial = setup(1, json!([]));
    let mut game = Game::new(
        catalog_with_cost(&Value::Null),
        &initial,
        &Value::Null,
        &Value::Null,
        "missing-cost",
    )
    .unwrap();
    let before = exact_authority(&game);
    assert!(
        matches!(game.load_internal_card("unit-follower", &card(vec![])), Err(EngineFailure::Unsupported(reason)) if reason.contains("missing printed card cost"))
    );
    assert_eq!(exact_authority(&game), before);
    let mut dash = Game::new(
        catalog_with_cost(&json!("-")),
        &initial,
        &Value::Null,
        &Value::Null,
        "dash-cost",
    )
    .unwrap();
    assert!(matches!(
        dash.load_internal_card("unit-follower", &card(vec![])),
        Err(EngineFailure::Unsupported(_))
    ));
    let mut zero = Game::new(
        catalog_with_cost(&json!("0")),
        &initial,
        &Value::Null,
        &Value::Null,
        "zero-cost",
    )
    .unwrap();
    zero.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
}
