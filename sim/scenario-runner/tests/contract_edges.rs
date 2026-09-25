//! Hand-written checks of contract edge cases, independent of the question set.
//!
//! Each test builds a tiny scenario and a scripted engine, so what counts as correct
//! comes from the contract text, not from any question's `expected`.

#![allow(
    clippy::default_numeric_fallback,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::float_arithmetic,
    clippy::absolute_paths,
    clippy::std_instead_of_alloc,
    reason = "test code: fixtures are literal JSON and a panic is a test failure"
)]

use serde_json::{Value, json};
use sve_scenario_runner::compare::{
    assert_value, awaiting, events_exact, events_subsequence, exact, forbidden_hit, subset,
};
use sve_scenario_runner::model::parse_question;
use sve_scenario_runner::runner::{Failure, ScenarioReport, summary};
use sve_scenario_runner::{
    Engine, EngineError, Fixture, RunOptions, Selection, Step, Verdict, View, load_dir,
    parse_selection, run, score_g1,
};

fn arr(v: Value) -> Vec<Value> {
    match v {
        Value::Array(a) => a,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Object(_) => {
            Vec::new()
        }
    }
}

// --- events (contract 6.4) ---

#[test]
fn group_members_match_in_any_order() {
    let expected = arr(json!([
        {"kind": "破壊", "object": "a", "group": "g"},
        {"kind": "破壊", "object": "b", "group": "g"}
    ]));
    let actual = arr(json!([
        {"kind": "破壊", "object": "b", "group": 1},
        {"kind": "破壊", "object": "a", "group": 1}
    ]));
    assert!(events_subsequence(&expected, &actual));
}

#[test]
fn group_split_by_another_event_is_not_simultaneous() {
    let expected = arr(json!([
        {"kind": "破壊", "object": "a", "group": "g"},
        {"kind": "破壊", "object": "b", "group": "g"}
    ]));
    let actual = arr(json!([
        {"kind": "破壊", "object": "a", "group": 1},
        {"kind": "引く", "player": "P1"},
        {"kind": "破壊", "object": "b", "group": 1}
    ]));
    assert!(!events_subsequence(&expected, &actual));
}

#[test]
fn group_members_in_different_engine_groups_fail() {
    let expected = arr(json!([
        {"kind": "破壊", "object": "a", "group": "g"},
        {"kind": "破壊", "object": "b", "group": "g"}
    ]));
    let actual = arr(json!([
        {"kind": "破壊", "object": "a", "group": 1},
        {"kind": "破壊", "object": "b", "group": 2}
    ]));
    assert!(!events_subsequence(&expected, &actual));
}

#[test]
fn subsequence_allows_gaps_but_not_reordering() {
    let expected = arr(json!([{"kind": "プレイ"}, {"kind": "解決"}]));
    let gaps = arr(json!([{"kind": "プレイ"}, {"kind": "引く"}, {"kind": "解決"}]));
    let swapped = arr(json!([{"kind": "解決"}, {"kind": "プレイ"}]));
    assert!(events_subsequence(&expected, &gaps));
    assert!(!events_subsequence(&expected, &swapped));
}

#[test]
fn repeated_identical_events_are_counted() {
    let expected = arr(json!([
        {"kind": "ダメージ", "amount": 1}, {"kind": "ダメージ", "amount": 1}
    ]));
    let once = arr(json!([{"kind": "ダメージ", "amount": 1}]));
    assert!(!events_subsequence(&expected, &once));
}

#[test]
fn events_exact_rejects_an_extra_event_of_a_listed_kind() {
    let kinds = vec!["プレイ".to_owned()];
    let expected = arr(json!([{"kind": "プレイ", "object": "a"}]));
    let extra = arr(json!([
        {"kind": "プレイ", "object": "a"}, {"kind": "引く"}, {"kind": "プレイ", "object": "b"}
    ]));
    let other_kinds_only = arr(json!([{"kind": "引く"}, {"kind": "プレイ", "object": "a"}]));
    assert!(!events_exact(&kinds, &expected, &extra));
    assert!(events_exact(&kinds, &expected, &other_kinds_only));
}

#[test]
fn forbidden_pattern_matches_on_written_fields() {
    let forbidden = arr(json!([{"kind": "ダメージ", "target": "b1"}]));
    let hit = arr(json!([{"kind": "ダメージ", "target": "b1", "amount": 0}]));
    let miss = arr(json!([{"kind": "ダメージ", "target": "b2", "amount": 3}]));
    assert!(forbidden_hit(&forbidden, &hit).is_some());
    assert!(forbidden_hit(&forbidden, &miss).is_none());
}

// --- zones and fillers (contract 2.2, 2.3) ---

#[test]
fn deck_is_ordered_other_zones_are_multisets() {
    assert!(!assert_value(
        "P1.deck",
        &json!(["a", "b"]),
        &json!(["b", "a"])
    ));
    assert!(assert_value(
        "P1.hand",
        &json!(["a", "b"]),
        &json!(["b", "a"])
    ));
}

#[test]
fn filler_counts_and_positions() {
    let expected = json!(["a", {"filler": 2}, "b"]);
    assert!(assert_value(
        "P1.deck",
        &expected,
        &json!(["a", {"filler": 1}, {"filler": 1}, "b"])
    ));
    assert!(!assert_value(
        "P1.deck",
        &expected,
        &json!([{"filler": 2}, "a", "b"])
    ));
    assert!(!assert_value(
        "P1.deck",
        &expected,
        &json!(["a", {"filler": 1}, "b"])
    ));
}

#[test]
fn filler_carrying_an_id_is_not_a_placeholder() {
    assert!(!assert_value(
        "P1.hand",
        &json!([{"filler": 1}]),
        &json!([{"filler": 1, "id": "d"}])
    ));
}

#[test]
fn empty_zone_must_be_empty() {
    assert!(!assert_value("P2.field", &json!([]), &json!(["b1"])));
}

#[test]
fn object_subtree_compares_written_keys_only() {
    assert!(assert_value(
        "P2.field.b1",
        &json!({"hp": 1}),
        &json!({"hp": 1, "power": 3})
    ));
    assert!(!assert_value(
        "P2.field.b1",
        &json!({"hp": 1}),
        &json!({"hp": 2})
    ));
}

// --- awaiting (contract 6.5, 10.14) ---

#[test]
fn opponent_view_may_not_receive_choices() {
    let want = json!({"by": "P1"});
    assert!(awaiting(View::P2, &want, Some(&json!({"by": "P1"}))));
    assert!(!awaiting(
        View::P2,
        &want,
        Some(&json!({"by": "P1", "choices": [{"do": "pass"}]}))
    ));
}

#[test]
fn choices_must_be_the_exact_option_set() {
    let want = json!({"by": "P1", "choices": [{"do": "pass"}, {"do": "end-phase"}]});
    let same = json!({"by": "P1", "choices": [{"do": "end-phase"}, {"do": "pass"}]});
    let missing = json!({"by": "P1", "choices": [{"do": "pass"}]});
    let extra_field =
        json!({"by": "P1", "choices": [{"do": "pass", "card": "q"}, {"do": "end-phase"}]});
    assert!(awaiting(View::Omniscient, &want, Some(&same)));
    assert!(!awaiting(View::Omniscient, &want, Some(&missing)));
    assert!(!awaiting(View::P1, &want, Some(&extra_field)));
}

// --- runner behaviour, with a scripted engine ---

/// Replies from a fixed script: one step per decision, plus fixed query answers.
struct Script {
    steps: Vec<Result<Step, EngineError>>,
    answers: Vec<(String, Value)>,
    projection: Value,
    awaiting: Option<Value>,
    next: usize,
}

impl Script {
    fn new(steps: Vec<Result<Step, EngineError>>) -> Self {
        Self {
            steps,
            answers: Vec::new(),
            projection: json!({}),
            awaiting: None,
            next: 0,
        }
    }
}

impl Engine for Script {
    fn load(&mut self, _fixture: &Fixture) -> Result<(), EngineError> {
        self.next = 0;
        Ok(())
    }

    fn decide(&mut self, _decision: &Value) -> Result<Step, EngineError> {
        let step = self.steps[self.next].clone();
        self.next += 1;
        step
    }

    fn query(&self, _view: View, path: &str) -> Result<Option<Value>, EngineError> {
        Ok(self
            .answers
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, v)| v.clone()))
    }

    fn awaiting(&self, _view: View) -> Result<Option<Value>, EngineError> {
        Ok(self.awaiting.clone())
    }

    fn projection(&self, _view: View) -> Result<Value, EngineError> {
        Ok(self.projection.clone())
    }
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "scripts mix steps with engine errors"
)]
fn step(outcome: &str, events: Value) -> Result<Step, EngineError> {
    Ok(Step {
        outcome: outcome.to_owned(),
        events: arr(events),
    })
}

const TWO_DECISIONS: &str = "
id: t-1
status: verified
scenarios:
  - name: s
    setup:
      players:
        P1:
          zones:
            hand: [{id: d, card: BP01-042}]
    decisions:
      - {n: 1, by: P1, at: main, do: pass}
      - {n: 2, by: P1, at: main, do: pass}
    expected:
      - at: after-decision-2
        view: P2
        outcome: resolved
        events:
          - {kind: 引く, player: P1}
          - {kind: 捨てる, player: P1}
        assert:
          knowledge: {does_not_know: [d]}
";

#[test]
fn event_window_spans_decisions_without_a_checkpoint() {
    let q = parse_question(TWO_DECISIONS).unwrap();
    let mut engine = Script::new(vec![
        step("resolved", json!([{"kind": "引く", "player": "P1"}])),
        step("resolved", json!([{"kind": "捨てる", "player": "P1"}])),
    ]);
    engine
        .answers
        .push(("knowledge".into(), json!({"identifiable": []})));
    let reports = run(&mut engine, &[q], None, RunOptions::default()).unwrap();
    assert!(
        matches!(reports[0].verdict, Verdict::Pass),
        "{:?}",
        reports[0].verdict
    );
}

#[test]
fn hidden_card_number_in_projection_is_a_leak() {
    let q = parse_question(TWO_DECISIONS).unwrap();
    let mut engine = Script::new(vec![
        step("resolved", json!([{"kind": "引く", "player": "P1"}])),
        step("resolved", json!([{"kind": "捨てる", "player": "P1"}])),
    ]);
    engine
        .answers
        .push(("knowledge".into(), json!({"identifiable": []})));
    engine.projection = json!({"opponent_hand": [{"card": "BP01-042"}]});
    let reports = run(&mut engine, &[q], None, RunOptions::default()).unwrap();
    let Verdict::Fail { failures } = &reports[0].verdict else {
        panic!("expected a leak, got {:?}", reports[0].verdict);
    };
    assert!(failures.iter().any(|f| f.what == "projection leak"));
}

#[test]
fn hidden_id_in_opponent_awaiting_is_a_leak() {
    let q = parse_question(TWO_DECISIONS).unwrap();
    let mut engine = Script::new(vec![
        step("resolved", json!([{"kind": "引く", "player": "P1"}])),
        step("resolved", json!([{"kind": "捨てる", "player": "P1"}])),
    ]);
    engine
        .answers
        .push(("knowledge".into(), json!({"identifiable": []})));
    engine.awaiting = Some(json!({"by": "P1", "choices": [{"select": ["d"]}]}));
    let reports = run(&mut engine, &[q], None, RunOptions::default()).unwrap();
    assert!(matches!(reports[0].verdict, Verdict::Fail { .. }));
}

#[test]
fn unsupported_and_adapter_errors_are_reported_apart() {
    let q = parse_question(TWO_DECISIONS).unwrap();
    let mut unsupported = Script::new(vec![Err(EngineError::Unsupported("x".into()))]);
    let mut broken = Script::new(vec![Err(EngineError::Adapter("y".into()))]);
    let a = run(
        &mut unsupported,
        core::slice::from_ref(&q),
        None,
        RunOptions::default(),
    )
    .unwrap();
    let b = run(&mut broken, &[q], None, RunOptions::default()).unwrap();
    assert!(matches!(a[0].verdict, Verdict::Unsupported { .. }));
    assert!(matches!(b[0].verdict, Verdict::AdapterError { .. }));
}

#[test]
fn checkpoint_beyond_the_decisions_is_a_question_error() {
    let text = TWO_DECISIONS.replace("after-decision-2", "after-decision-3");
    let q = parse_question(&text).unwrap();
    let mut engine = Script::new(vec![]);
    run(&mut engine, &[q], None, RunOptions::default()).unwrap_err();
}

#[test]
fn unknown_selection_entries_are_errors_not_skips() {
    let q = parse_question(TWO_DECISIONS).unwrap();
    for bad in [
        "{t-2: all}",
        "{t-1: [nope]}",
        "{t-1: [s, s]}",
        "{t-1: every}",
    ] {
        let selection: Selection = parse_selection(bad).unwrap();
        let mut engine = Script::new(vec![]);
        assert!(
            run(
                &mut engine,
                core::slice::from_ref(&q),
                Some(&selection),
                RunOptions::default()
            )
            .is_err(),
            "{bad} should be rejected"
        );
    }
}

#[test]
fn unverified_questions_are_not_scored_when_required() {
    let text = TWO_DECISIONS.replace("status: verified", "status: draft");
    let q = parse_question(&text).unwrap();
    let mut engine = Script::new(vec![]);
    let options = RunOptions {
        require_verified: true,
    };
    let reports = run(&mut engine, &[q], None, options).unwrap();
    assert!(matches!(reports[0].verdict, Verdict::Ineligible { .. }));
}

#[test]
fn unconfirmed_card_facts_are_not_scored_when_required() {
    let text = TWO_DECISIONS.replace(
        "    decisions:",
        "    card_facts: {CP03-016: {trigger_icon: draw, verified: false}}\n    decisions:",
    );
    let q = parse_question(&text).unwrap();
    let mut engine = Script::new(vec![]);
    let options = RunOptions {
        require_verified: true,
    };
    let reports = run(&mut engine, &[q], None, options).unwrap();
    assert!(matches!(reports[0].verdict, Verdict::Ineligible { .. }));
}

#[test]
fn inherit_merges_maps_and_replaces_lists() {
    let text = "
id: t-2
scenarios:
  - name: base
    setup: {turn: {active: P1, phase: main}, players: {P1: {zones: {hand: [{id: a, card: X}]}}}}
  - name: child
    inherit: base
    setup: {turn: {phase: end}, players: {P1: {zones: {hand: [{id: b, card: Y}]}}}}
";
    // `Y` stays a string: YAML 1.1 booleans are off.
    let q = parse_question(text).unwrap();
    let fixtures = sve_scenario_runner::inherit::expand(&q.id, &q.scenarios).unwrap();
    let child = &fixtures[1].setup;
    assert_eq!(child["turn"], json!({"active": "P1", "phase": "end"}));
    assert_eq!(
        child["players"]["P1"]["zones"]["hand"],
        json!([{"id": "b", "card": "Y"}])
    );
}

// --- known limit: a public copy of the same card masks a card-number leak ---

/// Reports P2's view as the public field plus, if `leaky`, the card numbers of
/// P1's hand.
struct HandEcho {
    leaky: bool,
    setup: Value,
}

impl HandEcho {
    fn cards(&self, player: &str, zone: &str) -> Vec<Value> {
        arr(self.setup["players"][player]["zones"][zone].clone())
            .iter()
            .map(|o| o["card"].clone())
            .collect()
    }
}

impl Engine for HandEcho {
    fn load(&mut self, fixture: &Fixture) -> Result<(), EngineError> {
        self.setup = fixture.setup.clone();
        Ok(())
    }

    fn decide(&mut self, _decision: &Value) -> Result<Step, EngineError> {
        step("resolved", json!([]))
    }

    fn query(&self, _view: View, path: &str) -> Result<Option<Value>, EngineError> {
        Ok((path == "knowledge").then(|| json!({"identifiable": ["p"]})))
    }

    fn awaiting(&self, _view: View) -> Result<Option<Value>, EngineError> {
        Ok(None)
    }

    fn projection(&self, _view: View) -> Result<Value, EngineError> {
        let mut out = json!({"field": self.cards("P2", "field")});
        if self.leaky {
            out["opponent_hand_card"] = self.cards("P1", "hand").into();
        }
        Ok(out)
    }
}

const SAME_CARD_PUBLIC: &str = "
id: t-3
status: verified
scenarios:
  - name: s
    setup:
      players:
        P1: {zones: {hand: [{id: d, card: BP01-042}]}}
        P2: {zones: {field: [{id: p, card: BP01-042}]}}
    decisions:
      - {n: 1, by: P1, at: main, do: pass}
    expected:
      - at: after-decision-1
        view: P2
        outcome: resolved
        assert:
          knowledge: {knows: [p], does_not_know: [d]}
";

/// Documents the limit rather than hiding it: the runner cannot tell a public copy
/// of a card from a leaked hidden copy in a payload of arbitrary shape. This case is
/// left to the adapter review (paired positions on the real player payload).
#[test]
fn known_limit_public_copy_masks_a_card_number_leak() {
    let q = parse_question(SAME_CARD_PUBLIC).unwrap();
    let mut leaky = HandEcho {
        leaky: true,
        setup: Value::Null,
    };
    let reports = run(&mut leaky, &[q], None, RunOptions::default()).unwrap();
    assert!(matches!(reports[0].verdict, Verdict::Pass));
}

// --- groups the question does not mention ---

#[test]
fn a_broken_group_fails_even_when_not_asserted() {
    let q = parse_question(TWO_DECISIONS).unwrap();
    let mut engine = Script::new(vec![
        step(
            "resolved",
            json!([{"kind": "破壊", "group": 7}, {"kind": "引く", "player": "P1"}, {"kind": "破壊", "group": 7}]),
        ),
        step("resolved", json!([{"kind": "捨てる", "player": "P1"}])),
    ]);
    engine
        .answers
        .push(("knowledge".into(), json!({"identifiable": []})));
    let reports = run(&mut engine, &[q], None, RunOptions::default()).unwrap();
    let Verdict::Fail { failures } = &reports[0].verdict else {
        panic!("expected a failure, got {:?}", reports[0].verdict);
    };
    assert!(failures.iter().any(|f| f.what == "group not contiguous"));
}

// --- the G1 entry fails closed ---

#[test]
fn g1_rejects_empty_all_miscounted_and_ineligible() {
    let q = parse_question(TWO_DECISIONS).unwrap();
    let draft =
        parse_question(&TWO_DECISIONS.replace("status: verified", "status: draft")).unwrap();
    let mut engine = Script::new(vec![]);
    let cases: [(&str, &sve_scenario_runner::Question, usize); 4] = [
        ("{}", &q, 0),
        ("{t-1: all}", &q, 1),
        ("{t-1: [s]}", &q, 2),
        ("{t-1: [s]}", &draft, 1),
    ];
    for (text, question, count) in cases {
        let selection = parse_selection(text).unwrap();
        let result = score_g1(
            &mut engine,
            core::slice::from_ref(question),
            &selection,
            count,
        );
        assert!(result.is_err(), "{text} with {count} should be rejected");
    }
}

#[test]
fn the_g1_selection_names_exactly_41_scenarios() {
    let root =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/rules-scenarios");
    let questions = load_dir(&root.join("questions")).unwrap();
    let selection = sve_scenario_runner::load_selection(&root.join("g1-selection.yaml")).unwrap();
    assert!(
        selection
            .0
            .values()
            .all(|p| matches!(p, sve_scenario_runner::runner::Pick::Named(_)))
    );
    assert_eq!(selection.validate(&questions).unwrap(), 41);
}

// --- value comparison (contract 2.3) ---

#[test]
fn numbers_compare_by_value_at_any_depth() {
    assert!(subset(&json!(1), &json!(1.0)));
    for (e, a) in [
        (json!(1), json!(1.0)),
        (json!([1]), json!([1.0])),
        (json!({"n": 1}), json!({"n": 1.0})),
    ] {
        assert!(exact(&e, &a), "{e} vs {a}");
    }
}

#[test]
fn lists_outside_zones_are_multisets_of_partial_items() {
    assert!(subset(&json!([1, 2]), &json!([2, 1])));
    assert!(subset(&json!([{"a": 1}]), &json!([{"a": 1, "b": 2}])));
    // Each actual item may be used only once.
    assert!(!subset(
        &json!([{"a": 1}, {"a": 1}]),
        &json!([{"a": 1}, {"b": 2}])
    ));
}

#[test]
fn exact_rejects_extra_keys() {
    assert!(!exact(&json!({"a": 1}), &json!({"a": 1, "b": 2})));
}

#[test]
fn placeholders_apply_only_to_zone_paths() {
    let expected = json!([{"filler": 2}]);
    let actual = json!([{"filler": 1}, {"filler": 1}]);
    assert!(assert_value("P1.hand", &expected, &actual));
    assert!(!assert_value("P1.notes", &expected, &actual));
}

#[test]
fn a_placeholder_does_not_match_a_null_card() {
    assert!(!assert_value(
        "P1.hand",
        &json!([{"filler": 1}]),
        &json!([null])
    ));
}

// --- awaiting (contract 6.5) ---

#[test]
fn own_view_compares_choices() {
    let want = json!({"by": "P1", "choices": [{"do": "pass"}]});
    assert!(awaiting(View::P1, &want, Some(&want)));
    assert!(awaiting(
        View::P2,
        &json!({"by": "P2", "choices": []}),
        Some(&json!({"by": "P2", "choices": []}))
    ));
}

#[test]
fn unwritten_choices_are_not_compared() {
    assert!(awaiting(
        View::Omniscient,
        &json!({"by": "P1"}),
        Some(&json!({"by": "P1", "choices": [{"do": "pass"}]}))
    ));
}

#[test]
fn views_parse_from_the_contract_spelling() {
    assert_eq!(View::parse("omniscient"), Some(View::Omniscient));
    assert_eq!(View::parse("P1"), Some(View::P1));
    assert_eq!(View::parse("P2"), Some(View::P2));
    assert_eq!(View::parse("p1"), None);
}

// --- inherit (contract 3) ---

#[test]
fn an_inherit_cycle_through_another_scenario_is_an_error() {
    let text = "
id: t-3
scenarios:
  - {name: a, inherit: b}
  - {name: b, inherit: a}
";
    let q = parse_question(text).unwrap();
    sve_scenario_runner::inherit::expand(&q.id, &q.scenarios).unwrap_err();
}

#[test]
fn a_scenario_without_setup_gets_empty_maps() {
    let q = parse_question("id: t-4\nscenarios:\n  - {name: a}\n").unwrap();
    let fixtures = sve_scenario_runner::inherit::expand(&q.id, &q.scenarios).unwrap();
    assert_eq!(fixtures[0].setup, json!({}));
    assert_eq!(fixtures[0].card_facts, json!({}));
}

// --- selections ---

#[test]
fn a_valid_selection_counts_its_scenarios() {
    let q = parse_question(TWO_DECISIONS).unwrap();
    for (text, count) in [("{t-1: all}", 1), ("{t-1: [s]}", 1), ("{}", 0)] {
        let selection = parse_selection(text).unwrap();
        assert_eq!(
            selection.validate(core::slice::from_ref(&q)).unwrap(),
            count,
            "{text}"
        );
    }
    let unknown = parse_selection("{t-1: [nope]}").unwrap();
    unknown.validate(core::slice::from_ref(&q)).unwrap_err();
}

// --- events_exact at a checkpoint ---

#[test]
fn events_exact_fails_the_scenario_on_an_extra_event() {
    let text = TWO_DECISIONS.replace(
        "        assert:",
        "        events_exact: [引く]\n        assert:",
    );
    let q = parse_question(&text).unwrap();
    let mut engine = Script::new(vec![
        step(
            "resolved",
            json!([{"kind": "引く", "player": "P1"}, {"kind": "引く", "player": "P1"}]),
        ),
        step("resolved", json!([{"kind": "捨てる", "player": "P1"}])),
    ]);
    engine
        .answers
        .push(("knowledge".into(), json!({"identifiable": []})));
    let reports = run(&mut engine, &[q], None, RunOptions::default()).unwrap();
    let Verdict::Fail { failures } = &reports[0].verdict else {
        panic!("{:?}", reports[0].verdict);
    };
    assert!(failures.iter().any(|f| f.what.starts_with("events_exact")));
}

// --- summary ---

#[test]
fn summary_counts_each_verdict() {
    let report = |verdict| ScenarioReport {
        question: "q".into(),
        scenario: "s".into(),
        verdict,
    };
    let failure = Failure {
        at: "after-decision-1".into(),
        view: "P1".into(),
        what: "outcome".into(),
        expected: json!("resolved"),
        actual: json!("illegal"),
    };
    let reports = [
        report(Verdict::Pass),
        report(Verdict::Pass),
        report(Verdict::Fail {
            failures: vec![failure],
        }),
        report(Verdict::Unsupported {
            reason: String::new(),
        }),
        report(Verdict::AdapterError {
            reason: String::new(),
        }),
        report(Verdict::Ineligible {
            reason: String::new(),
        }),
    ];
    let counts: Vec<(&str, usize)> = summary(&reports).into_iter().collect();
    assert_eq!(
        counts,
        [
            ("adapter-error", 1),
            ("fail", 1),
            ("ineligible", 1),
            ("pass", 2),
            ("unsupported", 1),
        ]
    );
}
