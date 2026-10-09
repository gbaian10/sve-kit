//! Runs the independent engine against the unmodified shared behavior suites.

#![cfg(feature = "runner")]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use std::fs::read_to_string;
use std::path::PathBuf;

use sve_engine::adapter::{Adapter, AiAdapter, AssistAdapter, ReplayAdapter};
use sve_engine::ai::Profile;
use sve_scenario_runner::ai::{AiEngine, AiOutcome, check_ai, load_positions};
use sve_scenario_runner::arch::{
    ArchOptions, AssistEngine, Outcome, ReplayEngine, check_assist, check_replay, load_fixtures,
};
use sve_scenario_runner::{
    RunOptions, Verdict, gate, load_dir, load_known_failures, load_selection, run, score_g1,
};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[path = "support/private_catalog.rs"]
mod private_catalog;

use private_catalog::catalog;

#[test]
fn representative_rules_pass_every_checkpoint() {
    let Some(catalog) = catalog() else {
        return;
    };
    let questions = load_dir(&root().join("tests/rules-scenarios/questions")).unwrap();
    let selection =
        load_selection(&root().join("tests/rules-scenarios/g1-selection.yaml")).unwrap();
    let reports = score_g1(&mut Adapter::new(catalog), &questions, &selection, 41).unwrap();
    assert_eq!(reports.len(), 41);
    for report in reports {
        assert!(matches!(report.verdict, Verdict::Pass), "{report:?}");
    }
}

#[test]
fn every_shared_scenario_passes_or_is_a_listed_known_failure() {
    let Some(catalog) = catalog() else {
        return;
    };
    let questions = load_dir(&root().join("tests/rules-scenarios/questions")).unwrap();
    let known = load_known_failures(&root().join("tests/engine/known-failures.yaml")).unwrap();
    let reports = run(
        &mut Adapter::new(catalog),
        &questions,
        None,
        RunOptions {
            require_verified: true,
        },
    )
    .unwrap();
    let result = gate(&reports, &known);
    assert!(result.ok(), "{:#?}", result.problems);
}

/// Load-time rejection (M1): every authored card either loads or is a reviewed rejection.
#[test]
fn authored_yaml_loads_or_is_a_listed_rejection() {
    let Some(catalog) = catalog() else {
        return;
    };
    let listed: serde_json::Value = serde_saphyr::from_str(
        &read_to_string(root().join("tests/engine/rejected-yaml.yaml")).unwrap(),
    )
    .unwrap();
    let mut expected = listed["rejected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry["card"].as_str().unwrap().to_owned(),
                entry["finding"].as_str().unwrap().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    let mut actual = catalog
        .rejections()
        .iter()
        .flat_map(|(card, findings)| {
            findings.iter().map(move |finding| {
                let message = finding.splitn(3, ':').nth(2).unwrap_or(finding).trim();
                (card.clone(), message.to_owned())
            })
        })
        .collect::<Vec<_>>();
    expected.sort();
    actual.sort();
    assert_eq!(actual, expected);
    // Every finding points at a line of its card, never at "line 0".
    for findings in catalog.rejections().values() {
        for finding in findings {
            let line = finding.split(':').nth(1).unwrap_or_default();
            assert!(line.parse::<usize>().is_ok_and(|n| n > 0), "{finding}");
        }
    }
}

#[test]
fn replay_and_assist_match_every_shared_assertion() {
    let Some(catalog) = catalog() else {
        return;
    };
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
    let Some(catalog) = catalog() else {
        return;
    };
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
