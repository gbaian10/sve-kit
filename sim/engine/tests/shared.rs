//! Runs the independent engine against the unmodified shared behavior suites.

#![cfg(feature = "runner")]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use std::path::PathBuf;

use sve_engine::adapter::{AiAdapter, AssistAdapter, ReplayAdapter};
use sve_engine::ai::Profile;
use sve_scenario_runner::ai::{AiEngine, AiOutcome, check_ai, load_positions};
use sve_scenario_runner::arch::{
    ArchOptions, AssistEngine, Outcome, ReplayEngine, check_assist, check_replay, load_fixtures,
};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[path = "support/generated_catalog.rs"]
mod generated_catalog;

use generated_catalog::historical_regression_catalog;

#[test]
fn replay_and_assist_match_every_shared_assertion() {
    let catalog = historical_regression_catalog();
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
    let catalog = historical_regression_catalog();
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
