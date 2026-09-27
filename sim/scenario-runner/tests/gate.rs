//! The strict gate fails on every deviation from the reviewed list.

#![allow(
    clippy::indexing_slicing,
    clippy::std_instead_of_alloc,
    reason = "test code: a panic is a test failure"
)]

use sve_scenario_runner::gate::{GateProblem, KnownFailure, KnownFailures, gate, status};
use sve_scenario_runner::runner::{Failure, ScenarioReport, Verdict};

fn report(question: &str, scenario: &str, verdict: Verdict) -> ScenarioReport {
    ScenarioReport {
        question: question.into(),
        scenario: scenario.into(),
        verdict,
    }
}

fn fail() -> Verdict {
    Verdict::Fail {
        failures: vec![Failure {
            at: "after-decision-1".into(),
            view: "judge".into(),
            what: "outcome".into(),
            expected: "resolved".into(),
            actual: "paused".into(),
        }],
    }
}

fn listed(question: &str, scenario: &str, status: &str) -> KnownFailure {
    KnownFailure {
        question: question.into(),
        scenario: scenario.into(),
        status: status.into(),
        error: "KE-TEST".into(),
    }
}

const fn known(expected_scenarios: usize, known: Vec<KnownFailure>) -> KnownFailures {
    KnownFailures {
        expected_scenarios,
        known,
    }
}

#[test]
fn all_pass_with_empty_list_is_green() {
    let run = [
        report("q", "a", Verdict::Pass),
        report("q", "b", Verdict::Pass),
    ];
    let result = gate(&run, &known(2, Vec::new()));
    assert!(result.ok(), "{result:?}");
    assert_eq!(result.passed, 2);
}

#[test]
fn one_unlisted_failure_fails_the_gate() {
    let run = [report("q", "a", Verdict::Pass), report("q", "b", fail())];
    let result = gate(&run, &known(2, Vec::new()));
    assert!(!result.ok());
    assert_eq!(
        result.problems,
        vec![GateProblem::Unexpected {
            question: "q".into(),
            scenario: "b".into(),
            status: "fail".into(),
        }]
    );
}

#[test]
fn every_non_pass_verdict_counts_as_a_failure() {
    for verdict in [
        fail(),
        Verdict::Unsupported { reason: "x".into() },
        Verdict::AdapterError { reason: "x".into() },
        Verdict::Ineligible { reason: "x".into() },
    ] {
        let kind = status(&verdict);
        let result = gate(&[report("q", "a", verdict)], &known(1, Vec::new()));
        assert!(!result.ok(), "{kind} must not pass the gate");
    }
}

#[test]
fn listed_failure_with_the_same_status_is_tolerated() {
    let run = [report("q", "a", fail()), report("q", "b", Verdict::Pass)];
    let result = gate(&run, &known(2, vec![listed("q", "a", "fail")]));
    assert!(result.ok(), "{result:?}");
    assert_eq!((result.passed, result.known), (1, 1));
}

#[test]
fn listed_scenario_that_now_passes_must_be_removed() {
    let run = [report("q", "a", Verdict::Pass)];
    let result = gate(&run, &known(1, vec![listed("q", "a", "fail")]));
    assert_eq!(
        result.problems,
        vec![GateProblem::NowPasses {
            question: "q".into(),
            scenario: "a".into(),
        }]
    );
}

#[test]
fn a_changed_failure_kind_is_reported() {
    let run = [report(
        "q",
        "a",
        Verdict::AdapterError { reason: "x".into() },
    )];
    let result = gate(&run, &known(1, vec![listed("q", "a", "fail")]));
    assert!(matches!(
        result.problems.as_slice(),
        [GateProblem::StatusChanged { listed, actual, .. }] if listed == "fail" && actual == "adapter-error"
    ));
}

#[test]
fn stale_duplicate_and_count_problems_are_reported() {
    let run = [report("q", "a", fail())];
    let list = vec![
        listed("q", "a", "fail"),
        listed("q", "a", "fail"),
        listed("q", "gone", "fail"),
    ];
    let result = gate(&run, &known(2, list));
    assert!(result.problems.contains(&GateProblem::Duplicate {
        question: "q".into(),
        scenario: "a".into(),
    }));
    assert!(result.problems.contains(&GateProblem::Stale {
        question: "q".into(),
        scenario: "gone".into(),
    }));
    assert!(result.problems.contains(&GateProblem::Count {
        expected: 2,
        actual: 1,
    }));
}
