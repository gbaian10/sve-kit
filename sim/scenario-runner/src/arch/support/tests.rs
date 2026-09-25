//! Unit tests of the comparison helpers.

#![expect(
    clippy::default_numeric_fallback,
    reason = "test fixtures are literal JSON"
)]

use serde_json::json;

use super::*;
use crate::arch::Observation;
use crate::model::from_yaml;

fn ids() -> Ids {
    let mut ids = Ids::default();
    ids.node(&NodeId("n-a".into()));
    ids.node(&NodeId("n-b".into()));
    ids.node(&NodeId("n-a".into()));
    ids.events(&[
        json!({"id": "e-x"}),
        json!({"id": "e-y"}),
        json!({"id": "e-x"}),
        json!({"no": 1}),
    ]);
    ids
}

#[test]
fn ids_label_by_first_appearance() {
    let ids = ids();
    assert!(ids.knows_node("n-b"));
    assert!(!ids.knows_node("n-c"));
    assert_eq!(ids.node_label("n-a"), "#N1");
    assert_eq!(ids.node_label("n-b"), "#N2");
    // Unknown ids keep their value, so two different unknown ids never compare equal.
    assert_eq!(ids.node_label("n-c"), "n-c");
    assert_eq!(ids.node_label("n-d"), "n-d");
    assert_eq!(ids.event_label("e-z"), "e-z");
    assert_eq!(ids.event_label("e-x"), "#E1");
    assert_eq!(ids.event_label("e-y"), "#E2");
}

#[test]
fn normalize_touches_only_known_id_fields() {
    let options = ArchOptions {
        node_id_paths: vec!["at".into(), "list.*.from".into()],
        skip_paths: vec!["transport".into(), "drop.*".into(), "keep.*".into()],
    };
    let raw = json!({
        "at": "n-b",
        "list": [{"from": "n-a", "object": "n-a"}],
        "transport": 7,
        "drop": {"a": 1, "b": 2},
        "keep": [1, 2],
        "events": [{"kind": "k", "id": "e-y", "cause": {"event": "e-x", "decision": "n-a"}}],
        "object": "e-x",
    });
    let got = normalize(&raw, &ids(), &options);
    assert_eq!(
        got,
        json!({
            "at": "#N2",
            "list": [{"from": "#N1", "object": "n-a"}],
            "drop": {},
            "keep": [],
            "events": [{"kind": "k", "id": "#E2", "cause": {"event": "#E1", "decision": "#N1"}}],
            "object": "e-x",
        })
    );
}

#[test]
fn remove_path_by_key_and_index() {
    let mut v = json!({"a": {"b": 1, "c": 2}, "l": [{"x": 1}, {"x": 2}]});
    remove_path(&mut v, &["a", "b"]);
    remove_path(&mut v, &["l", "1", "x"]);
    remove_path(&mut v, &["missing", "x"]);
    remove_path(&mut v, &[]);
    assert_eq!(v, json!({"a": {"c": 2}, "l": [{"x": 1}, {}]}));
}

#[test]
fn hidden_at_respects_view_range_and_carried() {
    let rules = vec![
        Hidden {
            view: "P2".into(),
            objects: vec!["a".into(), "b".into()],
            from: 2,
            to: 4,
        },
        Hidden {
            view: "P2".into(),
            objects: vec!["b".into(), "c".into()],
            from: 4,
            to: 4,
        },
        Hidden {
            view: "P1".into(),
            objects: vec!["z".into()],
            from: 0,
            to: 9,
        },
    ];
    assert!(hidden_at(&rules, View::P2, 1, &[]).is_empty());
    assert_eq!(hidden_at(&rules, View::P2, 2, &[]), ["a", "b"]);
    assert_eq!(hidden_at(&rules, View::P2, 4, &["a".into()]), ["b", "c"]);
    assert!(hidden_at(&rules, View::P2, 5, &[]).is_empty());
    assert_eq!(hidden_at(&rules, View::P1, 5, &[]), ["z"]);
}

#[test]
fn leaks_find_ids_unmasked_card_numbers_and_the_seed() {
    let cards: BTreeMap<String, String> = [("h1", "C-1"), ("h2", "C-2"), ("p", "C-2")]
        .into_iter()
        .map(|(a, b)| (a.to_owned(), b.to_owned()))
        .collect();
    let hidden = vec!["h1".to_owned(), "h2".to_owned()];
    assert!(leaks(&json!({"x": "C-2 public copy"}), &hidden, &cards, "abcd").is_empty());
    assert_eq!(
        leaks(&json!(["h1"]), &hidden, &cards, "abcd"),
        ["hidden object h1"]
    );
    assert_eq!(
        leaks(&json!("card C-1"), &hidden, &cards, "abcd"),
        ["card number C-1 of hidden object h1"]
    );
    assert_eq!(
        leaks(&json!("seed ABCD"), &hidden, &cards, "abcd"),
        ["the seed"]
    );
    assert_eq!(
        leaks(&json!("seed abcd"), &hidden, &cards, "abcd"),
        ["the seed"]
    );
}

#[test]
fn known_reads_ids_and_carried_objects() {
    let (ids, carried) = known(
        &json!({"identifiable": ["a", {"object": "b"}], "carried": [{"object": "c", "from": "n"}]}),
    );
    assert_eq!(ids, ["a", "b"]);
    assert_eq!(carried, ["c"]);
    assert_eq!(known(&json!({})), (Vec::new(), Vec::new()));
}

#[test]
fn awaits_needs_player_and_option() {
    let a = json!({"by": "P1", "choices": [{"do": "pass"}]});
    assert!(awaits(Some(&a), "P1", "pass"));
    assert!(!awaits(Some(&a), "P2", "pass"));
    assert!(!awaits(Some(&a), "P1", "attack"));
    assert!(!awaits(None, "P1", "pass"));
}

#[test]
fn list_and_ordering_helpers() {
    assert!(list_has(Some(&json!(["a", {"id": "b"}])), &json!("b")));
    assert!(list_has(Some(&json!(["a"])), &json!("a")));
    assert!(!list_has(Some(&json!(["a"])), &json!("b")));
    assert!(!list_has(None, &json!("a")));
    assert_eq!(
        ids_of(Some(&json!(["a", {"id": "b"}]))),
        Some(vec!["a".into(), "b".into()])
    );
    assert_eq!(ids_of(Some(&json!([1]))), None);
    assert!(is_ordering(
        &["b".into(), "a".into()],
        &["a".into(), "b".into()]
    ));
    assert!(!is_ordering(
        &["a".into(), "a".into()],
        &["a".into(), "b".into()]
    ));
}

#[test]
fn has_match_searches_nested_values() {
    let v = json!({"x": [{"kind": "k", "n": 1}]});
    assert!(has_match(&v, &json!({"kind": "k"})));
    assert!(!has_match(&v, &json!({"kind": "j"})));
    assert!(!has_match(&json!(3), &json!({"kind": "k"})));
}

#[test]
fn observation_value_keeps_all_three_parts() {
    let o = Observation {
        projection: json!({"p": 1}),
        awaiting: None,
        knowledge: json!({"k": 2}),
    };
    assert_eq!(
        observation_value(&o),
        json!({"projection": {"p": 1}, "awaiting": null, "knowledge": {"k": 2}})
    );
}

fn fixed(yaml: &str) -> Fixed {
    from_yaml(yaml).unwrap()
}

fn run(
    fixed: &Fixed,
    state: &Value,
    awaiting: &Value,
    deck: &mut Option<Vec<String>>,
) -> Vec<String> {
    let query = |path: &str| Ok(state.get(path).cloned());
    let wait = || Ok(Some(awaiting.clone()));
    let probe = Probe {
        query: &query,
        awaiting: &wait,
    };
    check_fixed(&probe, fixed, deck, "t").unwrap()
}

#[test]
fn check_fixed_reports_each_kind_of_mismatch() {
    let state =
        json!({"life": 17, "hand": ["b1"], "deck": ["a5", "a4"], "hand2": ["a5"], "deck2": ["a4"]});
    let awaiting = json!({"by": "P1", "choices": [{"do": "pass"}]});
    let mut deck = None;
    let good = fixed(
        "{node: 1, assert: {life: 17}, awaiting_exact: {by: P1, choices: [{do: pass}]}, \
         awaiting_do: {by: P1, do: pass}, contains: {hand: b1}, record_deck: {path: deck, of: [a4, a5]}}",
    );
    assert!(run(&good, &state, &awaiting, &mut deck).is_empty());
    assert_eq!(deck, Some(vec!["a5".into(), "a4".into()]));
    let drew = fixed("{node: 2, drew_recorded: {hand: hand2, deck: deck2}}");
    assert!(run(&drew, &state, &awaiting, &mut deck).is_empty());
    let bad = fixed(
        "{node: 1, assert: {life: 18}, awaiting_exact: {by: P2}, awaiting_do: {by: P1, do: attack}, \
         contains: {hand: b9}, record_deck: {path: deck, of: [a4, a3]}}",
    );
    let mut other = None;
    assert_eq!(run(&bad, &state, &awaiting, &mut other).len(), 5);
    assert_eq!(other, None);
    let wrong_draw = fixed("{node: 2, drew_recorded: {hand: hand, deck: deck}}");
    assert_eq!(run(&wrong_draw, &state, &awaiting, &mut deck).len(), 2);
    assert_eq!(run(&wrong_draw, &state, &awaiting, &mut None).len(), 1);
}
