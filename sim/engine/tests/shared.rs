//! Runs the independent engine against the unmodified shared behavior suites.

#![cfg(feature = "runner")]

extern crate alloc;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::sync::Arc;
use std::env::var_os;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde_json::{Value, json};
use sve_engine::adapter::{Adapter, AiAdapter, AssistAdapter, ReplayAdapter};
use sve_engine::ai::Profile;
use sve_engine::catalog::Catalog;
use sve_scenario_runner::ai::{AiEngine, AiOutcome, check_ai, load_positions};
use sve_scenario_runner::arch::{
    ArchOptions, AssistEngine, Outcome, ReplayEngine, check_assist, check_replay, load_fixtures,
};
use sve_scenario_runner::{RunOptions, Verdict, load_dir, load_selection, run, score_g1};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[expect(
    clippy::expect_used,
    reason = "The externally supplied immutable fixture is a required test input."
)]
fn catalog() -> Arc<Catalog> {
    static CATALOG: OnceLock<Arc<Catalog>> = OnceLock::new();
    Arc::clone(CATALOG.get_or_init(|| {
        let snapshot = PathBuf::from(
            var_os("SVE_TEST_SNAPSHOT")
                .expect("set SVE_TEST_SNAPSHOT to the immutable cards.jsonl input"),
        );
        Arc::new(
            Catalog::load(&snapshot, &root().join("authored"))
                .expect("validated shared card snapshot"),
        )
    }))
}

#[test]
fn representative_rules_preserve_states_and_expose_legacy_pending_shape() {
    let questions = load_dir(&root().join("tests/rules-scenarios/questions")).unwrap();
    let selection =
        load_selection(&root().join("tests/rules-scenarios/g1-selection.yaml")).unwrap();
    let reports = score_g1(&mut Adapter::new(catalog()), &questions, &selection, 41).unwrap();
    assert_eq!(reports.len(), 41);
    let mut mismatched = 0_usize;
    for report in reports {
        match report.verdict {
            Verdict::Pass => {}
            Verdict::Fail { failures } => {
                mismatched = mismatched.saturating_add(1);
                for failure in failures {
                    assert_eq!(failure.what, "awaiting", "{failure:?}");
                    assert_eq!(failure.actual["by"], failure.expected["by"], "{failure:?}");
                    let legacy = failure.expected["choices"].as_array().unwrap();
                    assert!(
                        legacy.iter().all(|choice| choice
                            .as_object()
                            .unwrap()
                            .keys()
                            .all(|key| key == "do" || key == "pending")),
                        "{failure:?}"
                    );
                    let identities = |choices: &[Value]| {
                        choices
                            .iter()
                            .map(|choice| {
                                json!({"do":choice["do"],"pending":choice["pending"]}).to_string()
                            })
                            .collect::<BTreeSet<_>>()
                    };
                    assert_eq!(
                        identities(failure.actual["choices"].as_array().unwrap()),
                        identities(legacy),
                        "{failure:?}"
                    );
                }
            }
            other @ (Verdict::Unsupported { .. }
            | Verdict::AdapterError { .. }
            | Verdict::Ineligible { .. }) => panic!("unexpected G1 outcome: {other:?}"),
        }
    }
    assert_eq!(
        mismatched, 8,
        "The public G1 report must continue exposing every unresolved contract mismatch."
    );
}

#[test]
fn second_exam_passes_every_new_scenario_without_contract_exceptions() {
    let questions = load_dir(&root().join("tests/rules-scenarios/questions")).unwrap();
    let selection =
        load_selection(&root().join("docs/evaluation/seal-2/new-selection.yaml")).unwrap();
    let reports = run(
        &mut Adapter::new(catalog()),
        &questions,
        Some(&selection),
        RunOptions {
            require_verified: true,
        },
    )
    .unwrap();
    assert_eq!(reports.len(), 139);
    for report in reports {
        assert!(matches!(report.verdict, Verdict::Pass), "{report:?}");
    }
}

#[test]
fn replay_and_assist_match_every_shared_assertion() {
    let catalog = catalog();
    let fixtures = load_fixtures(&root().join("tests/architecture-fixtures")).unwrap();
    let mut replay =
        || -> Box<dyn ReplayEngine> { Box::new(ReplayAdapter::new(Arc::clone(&catalog))) };
    let mut assist =
        || -> Box<dyn AssistEngine> { Box::new(AssistAdapter::new(Arc::clone(&catalog))) };
    let options = ArchOptions::default();
    let mut reports = check_replay(&mut replay, &fixtures, &options);
    reports.extend(check_assist(&mut assist, &mut replay, &fixtures, &options));
    assert_eq!(reports.len(), 15);
    for report in reports {
        assert_eq!(report.outcome, Outcome::Pass, "{report:?}");
    }
}

#[test]
fn information_pairs_legality_and_profiles_match_shared_ai_contract() {
    let catalog = catalog();
    let mut profiles = BTreeMap::new();
    for id in ["general", "aggro", "control"] {
        profiles.insert(
            id.to_owned(),
            Profile::load(&root().join(format!("sim/engine/profiles/{id}.yaml"))).unwrap(),
        );
    }
    let mut factory = || -> Box<dyn AiEngine> {
        Box::new(AiAdapter::new(Arc::clone(&catalog), profiles.clone()))
    };
    let positions = load_positions(&root().join("tests/ai-positions")).unwrap();
    let reports = check_ai(&mut factory, &positions, &root().join("profiles.yaml"));
    assert_eq!(
        reports
            .iter()
            .filter(|r| r.outcome == AiOutcome::Pass)
            .count(),
        25
    );
    for report in reports {
        assert!(
            matches!(
                report.outcome,
                AiOutcome::Pass | AiOutcome::Diagnostic { .. }
            ),
            "{report:?}"
        );
    }
}
