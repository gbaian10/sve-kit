//! The shared rule exam on the typed player entry, against a reviewed results list.

#![cfg(feature = "runner")]

use std::path::PathBuf;
use sve_engine::adapter::TypedAdapter;
use sve_scenario_runner::inherit::expand;
use sve_scenario_runner::{
    AwaitingMode, Engine as _, EngineError, RunOptions, gate, load_dir, load_known_failures, run,
};

#[path = "support/repository.rs"]
mod repository;
#[path = "support/rule_fixtures.rs"]
mod rule_fixtures;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn every_shared_scenario_passes_on_the_typed_entry_or_is_listed() {
    let questions = load_dir(&root().join("tests/rules-scenarios/questions")).unwrap();
    let known = load_known_failures(&root().join("tests/engine/typed-known-results.yaml")).unwrap();
    // Every entry pins its cause, not only its verdict kind.
    let unpinned: Vec<_> = known
        .known
        .iter()
        .filter(|entry| entry.reason.is_none())
        .map(|entry| (&entry.question, &entry.scenario))
        .collect();
    assert!(
        unpinned.is_empty(),
        "entries without a reason: {unpinned:?}"
    );
    let reports = run(
        &mut TypedAdapter::new(repository::selection_with_rules().clone()),
        &questions,
        None,
        RunOptions {
            require_verified: true,
            awaiting: AwaitingMode::FirstStage,
        },
    )
    .unwrap();
    let result = gate(&reports, &known);
    assert!(
        result.ok(),
        "update tests/engine/typed-known-results.yaml: {:#?}",
        result.problems
    );
}

#[test]
fn mystic_acquisition_shared_scenarios_match_their_reviewed_results() {
    let mut questions = load_dir(&root().join("tests/rules-scenarios/questions")).unwrap();
    questions.retain(|question| {
        matches!(
            question.id.as_str(),
            "card-BP01-056" | "rule-10.7-01" | "rule-10.7.5-01"
        )
    });
    for question in &mut questions {
        if question.id == "rule-10.7-01" {
            question
                .scenarios
                .retain(|scenario| scenario.name.contains("神秘の獲得"));
        }
    }
    let reports = run(
        &mut TypedAdapter::new(repository::selection_with_rules().clone()),
        &questions,
        None,
        RunOptions {
            require_verified: true,
            awaiting: AwaitingMode::FirstStage,
        },
    )
    .unwrap();
    assert_eq!(reports.len(), 7);
    let mut known =
        load_known_failures(&root().join("tests/engine/typed-known-results.yaml")).unwrap();
    known.known.retain(|entry| {
        questions.iter().any(|question| {
            question.id == entry.question
                && question
                    .scenarios
                    .iter()
                    .any(|scenario| scenario.name == entry.scenario)
        })
    });
    known.expected_scenarios = 7;
    let result = gate(&reports, &known);
    assert!(result.ok(), "{:#?}", result.problems);
}

#[test]
fn a_reverted_play_leaves_no_referee_events_behind() {
    let questions = load_dir(&root().join("tests/rules-scenarios/questions")).unwrap();
    let question = questions
        .iter()
        .find(|question| question.id == "card-BP10-053")
        .unwrap();
    let name = question
        .scenarios
        .iter()
        .position(|scenario| scenario.name.starts_with("B "))
        .unwrap();
    let fixture = &expand(&question.id, &question.scenarios).unwrap()[name];
    let decisions = &question.scenarios[name].decisions;
    let selected = repository::selection_with_rules();
    let mut retried = TypedAdapter::new(selected.clone());
    retried.load(fixture).unwrap();
    let refused = retried.decide(&decisions[0]).unwrap();
    assert_eq!(refused.outcome, "cannot-play");
    assert!(refused.events.is_empty(), "{:?}", refused.events);
    let after_refusal = retried.decide(&decisions[1]).unwrap();
    let mut direct = TypedAdapter::new(selected.clone());
    direct.load(fixture).unwrap();
    let first_try = direct.decide(&decisions[1]).unwrap();
    assert_ne!(first_try.events.len(), 0);
    assert_eq!(after_refusal.outcome, first_try.outcome);
    // Event and execution identities keep counting past a rollback; contents must match.
    let contents = |events: &[serde_json::Value]| {
        events
            .iter()
            .map(|event| {
                let mut event = event.clone();
                event.as_object_mut().map(|fields| {
                    fields.remove("id");
                    fields.remove("cause")
                });
                event
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(contents(&after_refusal.events), contents(&first_try.events));
}

#[test]
fn a_named_pending_event_is_never_accepted_unchecked() {
    let questions = load_dir(&root().join("tests/rules-scenarios/questions")).unwrap();
    let question = questions
        .iter()
        .find(|question| question.id == "card-BP19-080")
        .unwrap();
    let fixture = &expand(&question.id, &question.scenarios).unwrap()[0];
    let mut adapter = TypedAdapter::new(repository::selection_with_rules().clone());
    adapter.load(fixture).unwrap();
    // Only c1 enters, so a1 line 1 has a single pending occurrence: the one for c1.
    let mut enter = question.scenarios[0].decisions[0].clone();
    enter["targets"]["1"] = serde_json::json!(["c1"]);
    adapter.decide(&enter).unwrap();
    for entered in ["c1", "c2"] {
        let named = serde_json::json!({"by": "P1", "at": "check-timing", "do": "choose-pending",
            "pending": {"ability": {"source": "a1", "line": 1_u8}, "event": {"entered_field": entered}}});
        assert!(
            matches!(
                adapter.clone().decide(&named),
                Err(EngineError::Unsupported(_))
            ),
            "{entered}"
        );
    }
    let unnamed = serde_json::json!({"by": "P1", "at": "check-timing", "do": "choose-pending",
        "pending": {"ability": {"source": "a1", "line": 1_u8}}});
    adapter.decide(&unnamed).unwrap();
}

#[test]
fn otherworld_ruler_shared_scenarios_match_their_reviewed_results() {
    let mut questions = load_dir(&root().join("tests/rules-scenarios/questions")).unwrap();
    questions.retain(|question| question.id == "card-BP08-SL19");
    let reports = run(
        &mut TypedAdapter::new(repository::selection_with_rules().clone()),
        &questions,
        None,
        RunOptions {
            require_verified: true,
            awaiting: AwaitingMode::FirstStage,
        },
    )
    .unwrap();
    assert_eq!(reports.len(), 3);
    let mut known =
        load_known_failures(&root().join("tests/engine/typed-known-results.yaml")).unwrap();
    known
        .known
        .retain(|entry| entry.question == "card-BP08-SL19");
    known.expected_scenarios = 3;
    let result = gate(&reports, &known);
    assert!(
        result.ok(),
        "update declaration exam results: {:#?}",
        result.problems
    );
}

#[test]
fn hidden_memory_shared_scenarios_match_their_reviewed_results() {
    let mut questions = load_dir(&root().join("tests/rules-scenarios/questions")).unwrap();
    questions.retain(|question| question.id == "card-BP17-030");
    assert_eq!(questions.len(), 1);
    let mut known =
        load_known_failures(&root().join("tests/engine/typed-known-results.yaml")).unwrap();
    let reports = run(
        &mut TypedAdapter::new(repository::selection_with_rules().clone()),
        &questions,
        None,
        RunOptions {
            require_verified: true,
            awaiting: AwaitingMode::FirstStage,
        },
    )
    .unwrap();
    known
        .known
        .retain(|entry| entry.question == "card-BP17-030");
    known.expected_scenarios = 6;
    let result = gate(&reports, &known);
    assert!(result.ok(), "{:#?}\nreports: {reports:#?}", result.problems);
}

#[test]
fn tempest_dragon_shared_scenarios_match_their_reviewed_results() {
    let mut questions = load_dir(&root().join("tests/rules-scenarios/questions")).unwrap();
    questions.retain(|question| question.id == "card-BP21-P38");
    let reports = run(
        &mut TypedAdapter::new(repository::selection_with_rules().clone()),
        &questions,
        None,
        RunOptions {
            require_verified: true,
            awaiting: AwaitingMode::FirstStage,
        },
    )
    .unwrap();
    assert_eq!(reports.len(), 2);
    let mut known =
        load_known_failures(&root().join("tests/engine/typed-known-results.yaml")).unwrap();
    known
        .known
        .retain(|entry| entry.question == "card-BP21-P38");
    known.expected_scenarios = 2;
    let result = gate(&reports, &known);
    assert!(
        result.ok(),
        "update EX class exam results: {:#?}",
        result.problems
    );
}
