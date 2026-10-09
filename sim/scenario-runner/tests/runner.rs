//! Validates the runner against the real question set with fake engines.

#![allow(
    clippy::default_numeric_fallback,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::float_arithmetic,
    clippy::absolute_paths,
    clippy::std_instead_of_alloc,
    reason = "test code: fixtures are literal JSON and a panic is a test failure"
)]

mod support;

use std::path::PathBuf;

use support::{ALL, Mutant, Mutation, Oracle};
use sve_scenario_runner::model::Expected;
use sve_scenario_runner::{RunOptions, Verdict, load_dir, load_selection, run, score_g1, summary};

fn questions_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/rules-scenarios/questions")
}

#[test]
fn oracle_passes_every_scenario() {
    let questions = load_dir(&questions_dir()).unwrap();
    let mut engine = Oracle::new(&questions);
    let reports = run(&mut engine, &questions, None, RunOptions::default()).unwrap();
    let failed: Vec<_> = reports
        .iter()
        .filter(|r| !matches!(r.verdict, Verdict::Pass))
        .map(|r| format!("{} / {}: {:?}", r.question, r.scenario, r.verdict))
        .collect();
    assert_eq!(reports.len(), 708, "{:?}", summary(&reports));
    assert!(
        failed.is_empty(),
        "{} failed, first: {:#?}",
        failed.len(),
        &failed[..failed.len().min(10)]
    );
}

#[test]
fn every_mutation_is_caught_where_it_applies() {
    let questions = load_dir(&questions_dir()).unwrap();
    let mut applied = std::collections::BTreeMap::<String, usize>::new();
    for &mutation in ALL {
        let mut engine = Mutant::new(Oracle::new(&questions), mutation);
        let reports = run(&mut engine, &questions, None, RunOptions::default()).unwrap();
        for (report, applies) in reports.iter().zip(applicability(&questions, mutation)) {
            if !applies {
                continue;
            }
            *applied.entry(format!("{mutation:?}")).or_default() += 1;
            assert!(
                matches!(report.verdict, Verdict::Fail { .. }),
                "{mutation:?} not caught in {} / {}",
                report.question,
                report.scenario
            );
        }
    }
    // Every mutation must have been exercised somewhere in the question set.
    for &mutation in ALL {
        assert!(
            applied.contains_key(&format!("{mutation:?}")),
            "{mutation:?} never applied"
        );
    }
    println!("{applied:?}");
}

fn applicability(questions: &[sve_scenario_runner::Question], mutation: Mutation) -> Vec<bool> {
    let mut out = Vec::new();
    for q in questions {
        for s in &q.scenarios {
            let max = s
                .expected
                .iter()
                .filter_map(|e| e.decision_index().ok())
                .max()
                .unwrap_or(0);
            out.push((1..=max).any(|n| {
                let entries: Vec<&Expected> = s
                    .expected
                    .iter()
                    .filter(|e| e.decision_index().ok() == Some(n))
                    .collect();
                Mutant::applies(mutation, &entries)
            }));
        }
    }
    out
}

#[test]
fn g1_entry_scores_the_41_scenarios() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/rules-scenarios");
    let questions = load_dir(&root.join("questions")).unwrap();
    let selection = load_selection(&root.join("g1-selection.yaml")).unwrap();
    let mut engine = Oracle::new(&questions);
    let reports = score_g1(&mut engine, &questions, &selection, 41).unwrap();
    assert_eq!(reports.len(), 41);
    assert!(
        reports.iter().all(|r| matches!(r.verdict, Verdict::Pass)),
        "{:?}",
        summary(&reports)
    );
}
