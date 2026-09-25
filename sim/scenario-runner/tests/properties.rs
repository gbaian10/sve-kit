//! Property tests of the comparison rules: invariants that must hold for any input,
//! not just the shapes that happen to appear in the question set.

#![allow(
    clippy::default_numeric_fallback,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::absolute_paths,
    clippy::std_instead_of_alloc,
    reason = "test code: fixtures are literal JSON and a panic is a test failure"
)]

use proptest::prelude::*;
use serde_json::{Map, Value};
use sve_scenario_runner::compare::{
    assert_value, events_subsequence, exact, forbidden_hit, mentions, subset,
};

const KEYS: &[&str] = &["kind", "player", "object", "card", "n", "zone"];

fn leaf() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::from),
        (-5_i64..=20).prop_map(Value::from),
        "[a-z]{1,3}|破壊|引く".prop_map(Value::from),
    ]
}

fn json_value() -> impl Strategy<Value = Value> {
    leaf().prop_recursive(3, 24, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
            prop::collection::btree_map(prop::sample::select(KEYS), inner, 0..4).prop_map(|m| {
                Value::Object(m.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
            }),
        ]
    })
}

/// An event without `group`, so it takes part in plain subsequence matching.
fn event() -> impl Strategy<Value = Value> {
    (
        prop::sample::select(&["引く", "破壊", "ダメージ", "移動"][..]),
        prop::sample::select(&["P1", "P2"][..]),
        0_i64..4,
    )
        .prop_map(|(kind, player, n)| {
            let mut m = Map::new();
            m.insert("kind".into(), Value::from(kind));
            m.insert("player".into(), Value::from(player));
            m.insert("n".into(), Value::from(n));
            Value::Object(m)
        })
}

fn leaves(v: &Value, out: &mut Vec<Value>) {
    match v {
        Value::Array(a) => a.iter().for_each(|x| leaves(x, out)),
        Value::Object(m) => m.values().for_each(|x| leaves(x, out)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => out.push(v.clone()),
    }
}

proptest! {
    #[test]
    fn subset_and_exact_are_reflexive(v in json_value()) {
        prop_assert!(subset(&v, &v));
        prop_assert!(exact(&v, &v));
    }

    #[test]
    fn exact_implies_subset(a in json_value(), b in json_value()) {
        if exact(&a, &b) {
            prop_assert!(subset(&a, &b));
        }
    }

    #[test]
    fn list_order_does_not_matter_outside_zones(items in prop::collection::vec(json_value(), 0..5)) {
        let mut reversed = items.clone();
        reversed.reverse();
        prop_assert!(subset(&Value::Array(items), &Value::Array(reversed)));
    }

    #[test]
    fn dropping_an_expected_key_keeps_a_match(
        m in prop::collection::btree_map(prop::sample::select(KEYS), json_value(), 1..5),
        drop in 0_usize..5,
    ) {
        let full: Map<String, Value> = m.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
        let mut partial = full.clone();
        if let Some(key) = full.keys().nth(drop % full.len()) {
            partial.remove(key);
        }
        prop_assert!(subset(&Value::Object(partial), &Value::Object(full)));
    }

    #[test]
    fn a_subsequence_of_the_events_is_found(
        events in prop::collection::vec(event(), 0..8),
        keep in prop::collection::vec(any::<bool>(), 8),
    ) {
        let picked: Vec<Value> = events
            .iter()
            .zip(&keep)
            .filter(|(_, k)| **k)
            .map(|(e, _)| e.clone())
            .collect();
        prop_assert!(events_subsequence(&picked, &events));
    }

    #[test]
    fn an_occurring_event_is_a_forbidden_hit(events in prop::collection::vec(event(), 1..8), at in 0_usize..8) {
        let hit = events[at % events.len()].clone();
        prop_assert!(forbidden_hit(core::slice::from_ref(&hit), &events).is_some());
        prop_assert!(forbidden_hit(&[], &events).is_none());
    }

    #[test]
    fn every_leaf_is_mentioned(v in json_value()) {
        let mut found = Vec::new();
        leaves(&v, &mut found);
        for leaf in &found {
            prop_assert!(mentions(&v, leaf));
        }
    }

    #[test]
    fn a_zone_matches_itself(cards in prop::collection::vec(json_value(), 0..5)) {
        let zone = Value::Array(cards);
        prop_assert!(assert_value("P1.hand", &zone, &zone));
        prop_assert!(assert_value("P1.deck", &zone, &zone));
    }
}
