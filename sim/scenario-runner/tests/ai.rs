//! Validates the AI checks with a scripted engine: a correct one passes every hard
//! check, and each broken behaviour is caught by the check meant for it.

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::absolute_paths,
    clippy::std_instead_of_alloc,
    clippy::default_numeric_fallback,
    clippy::unwrap_used,
    clippy::float_arithmetic,
    clippy::panic,
    reason = "test code: scripted tables, and a panic is a test failure"
)]

mod ai_support;

use std::fs;
use std::path::{Path, PathBuf};

use ai_support::{FakeAi, MUTATIONS, Mutation, Recording, table};
use serde_json::{Value, json};
use sve_scenario_runner::ai::{
    AiCheckReport, AiEngine, AiOutcome, AiPositions, AiReport, check_ai, check_search_log,
    load_positions,
};

fn outcome<'rep>(reports: &'rep [AiCheckReport], position: &str, id: &str) -> &'rep AiOutcome {
    &reports
        .iter()
        .find(|r| r.position == position && r.id == id)
        .unwrap()
        .outcome
}

fn notes(reports: &[AiCheckReport], position: &str, id: &str) -> String {
    let AiOutcome::Diagnostic { notes } = outcome(reports, position, id) else {
        panic!("{position}/{id} is not a diagnostic");
    };
    notes.join(" ")
}

fn positions() -> AiPositions {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/ai-positions");
    load_positions(&dir).unwrap()
}

/// Writes a `profiles.yaml` (with its files) into a fresh temporary directory.
fn profiles(name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sve-ai-profiles-{}-{name}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for file in ["general.toml", "aggro.toml", "control.toml"] {
        fs::write(dir.join(file), "weights = {}\n").unwrap();
    }
    let path = dir.join("profiles.yaml");
    fs::write(&path, text).unwrap();
    path
}

const GOOD_PROFILES: &str = "\
- {id: general, subtype: general, period: 2026-09, file: general.toml}
- {id: aggro, subtype: face, period: 2026-09, file: aggro.toml}
- {id: control, subtype: board, period: 2026-09, file: control.toml}
";

fn run(mutation: Mutation, profile_file: &Path) -> Vec<AiCheckReport> {
    let all = positions();
    let oracle = table(&all);
    let mut factory =
        || -> Box<dyn AiEngine> { Box::new(FakeAi::new(mutation, std::rc::Rc::clone(&oracle))) };
    check_ai(&mut factory, &all, profile_file)
}

fn failed(reports: &[AiCheckReport]) -> Vec<String> {
    reports
        .iter()
        .filter(|r| matches!(r.outcome, AiOutcome::Fail { .. }))
        .map(|r| format!("{}/{}", r.position, r.id))
        .collect()
}

#[test]
fn a_correct_ai_passes_every_hard_check() {
    let reports = run(Mutation::None, &profiles("good", GOOD_PROFILES));
    let bad: Vec<&AiCheckReport> = reports
        .iter()
        .filter(|r| !matches!(r.outcome, AiOutcome::Pass | AiOutcome::Diagnostic { .. }))
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
    assert!(notes(&reports, "ai-q-a", "Q3").contains("false"));
    assert!(notes(&reports, "ai-r-b", "R4").contains("candidates differ: true"));
    let ids: Vec<&str> = reports.iter().map(|r| r.id.as_str()).collect();
    for id in [
        "P0", "H1", "H2", "H3", "Q0", "Q0b", "Q1", "Q1b", "Q2", "Q3", "R0", "R1", "R2", "R3", "R4",
        "P-legal", "P1", "P2",
    ] {
        assert!(ids.contains(&id), "{id} is not reported");
    }
}

/// Each mutation must fail exactly where it is meant to be caught.
#[test]
fn every_broken_ai_is_caught_by_its_check() {
    let expected: &[(Mutation, &[&str])] = &[
        (Mutation::MissingDistribution, &["ai-h/H1"]),
        (Mutation::ExtraAttack, &["ai-h/H1"]),
        (Mutation::StaleSearch, &["ai-h/H2"]),
        (
            Mutation::IllegalDecision,
            &["ai-h/H3", "ai-r-a/R2", "ai-p/P1"],
        ),
        (Mutation::PeekHand, &["ai-q-a/Q2", "ai-q-b/Q2"]),
        (Mutation::PeekDeck, &["ai-r-a/R1", "ai-r-b/R1"]),
        (Mutation::LeakReveal, &["ai-r-a/R3", "ai-r-b/R3"]),
        (Mutation::ProfileReversed, &["ai-p/P1"]),
        (Mutation::ProfileIgnored, &["ai-p/P1"]),
        (Mutation::ThinkMutates, &["ai-p/P2"]),
        (Mutation::NoDeckList, &["ai-q-a/Q0b", "ai-q-b/Q0b"]),
        (Mutation::LeakHand, &["ai-q-a/Q0", "ai-q-b/Q0"]),
    ];
    assert_eq!(expected.len(), MUTATIONS.len());
    let good = profiles("mut", GOOD_PROFILES);
    for (mutation, must_fail) in expected {
        let got = failed(&run(*mutation, &good));
        for id in *must_fail {
            assert!(
                got.iter().any(|g| g == id),
                "{mutation:?}: {id} passed; failed: {got:?}"
            );
        }
    }
}

#[test]
fn profiles_without_required_fields_fail_p0() {
    let missing = "\
- {id: general, subtype: general, period: 2026-09, file: general.toml}
- {id: aggro, subtype: face, file: aggro.toml}
- {id: control, period: 2026-09, file: nowhere.toml}
";
    let reports = run(Mutation::None, &profiles("missing", missing));
    let p0 = reports.iter().find(|r| r.id == "P0").unwrap();
    let AiOutcome::Fail { reasons } = &p0.outcome else {
        panic!("{p0:?}");
    };
    assert_eq!(reasons.len(), 3, "{reasons:?}");
    let absent = run(Mutation::None, Path::new("/nonexistent/profiles.yaml"));
    assert!(matches!(absent[0].outcome, AiOutcome::Fail { .. }));
    let broken = run(Mutation::None, &profiles("broken", "{not: a list}"));
    assert!(matches!(broken[0].outcome, AiOutcome::Fail { .. }));
    let short = run(Mutation::None, &profiles("short", "- {id: general}\n"));
    let AiOutcome::Fail {
        reasons: short_reasons,
    } = &short[0].outcome
    else {
        panic!("{:?}", short[0]);
    };
    assert!(
        short_reasons.iter().any(|r| r.contains("aggro is missing")),
        "{short_reasons:?}"
    );
}

#[test]
fn engine_errors_are_reported_apart_from_failures() {
    let reports = run(Mutation::Unsupported, &profiles("unsup", GOOD_PROFILES));
    assert!(
        reports
            .iter()
            .filter(|r| r.id != "P0")
            .all(|r| matches!(r.outcome, AiOutcome::Unsupported { .. })),
        "{reports:#?}"
    );
}

// --- Q4: the search-log audit ---

fn edge(edge: u64, parent: Option<u64>, sample: u64, by: &str, rule: &str, what: &str) -> Value {
    json!({"edge": edge, "parent": parent, "sample": sample, "by": by, "at": "quick",
           "rule": rule, "decision": {"do": what}})
}

fn report(edges: u64) -> AiReport {
    AiReport {
        decision: json!({"do": "end-phase"}),
        candidates: Vec::new(),
        trace: Vec::new(),
        edges,
        engine_steps: 0,
        millis: 0,
    }
}

fn good_log() -> Vec<Value> {
    vec![
        edge(1, None, 0, "P1", "7.3", "attack"),
        edge(2, Some(1), 0, "P2", "8.4.7", "play"),
        edge(3, None, 1, "P1", "7.3", "end-phase"),
    ]
}

#[test]
fn a_tree_log_with_a_quick_answer_passes() {
    assert!(check_search_log(&good_log(), &report(3), 5000).is_empty());
    // Ids may be strings too.
    let mut named = good_log();
    named[0]["edge"] = json!("root");
    named[1]["parent"] = json!("root");
    assert!(check_search_log(&named, &report(3), 5000).is_empty());
}

#[test]
fn search_logs_that_prove_nothing_fail() {
    // A flat log: the Quick play hangs off nothing.
    let mut flat = good_log();
    flat[1]["parent"] = Value::Null;
    assert!(!check_search_log(&flat, &report(3), 5000).is_empty());
    // The sample switches along the path.
    let mut switched = good_log();
    switched[1]["sample"] = json!(1);
    assert!(!check_search_log(&switched, &report(3), 5000).is_empty());
    // The count disagrees with the report, or exceeds the budget.
    assert!(!check_search_log(&good_log(), &report(4), 5000).is_empty());
    assert!(!check_search_log(&good_log(), &report(3), 2).is_empty());
    assert!(check_search_log(&good_log(), &report(3), 3).is_empty());
    // Only the end-phase Quick (7.4.5).
    let mut end = good_log();
    end[1]["rule"] = json!("7.4.5");
    assert!(!check_search_log(&end, &report(3), 5000).is_empty());
    // A parent that comes later, a repeated edge, an edge without id.
    let mut later = good_log();
    later.swap(0, 1);
    assert!(!check_search_log(&later, &report(3), 5000).is_empty());
    let mut twice = good_log();
    twice[2]["edge"] = json!(1);
    assert!(!check_search_log(&twice, &report(3), 5000).is_empty());
    let mut unnamed = good_log();
    unnamed[2]["edge"] = Value::Null;
    assert!(!check_search_log(&unnamed, &report(3), 5000).is_empty());
    // The root is not a P1 attack.
    let mut rooted = good_log();
    rooted[0]["decision"] = json!({"do": "end-phase"});
    assert!(!check_search_log(&rooted, &report(3), 5000).is_empty());
}

#[test]
fn equivalent_answers_and_budget_edges_are_accepted() {
    let good = profiles("equiv", GOOD_PROFILES);
    // Target and selection order do not matter.
    let reversed = run(Mutation::ReverseTargets, &good);
    assert!(failed(&reversed).is_empty(), "{:?}", failed(&reversed));
    // The honest fake uses exactly the budget; one edge more fails every think.
    let over = failed(&run(Mutation::OverBudget, &good));
    for id in ["ai-h/H3", "ai-q-a/Q2", "ai-r-a/R2", "ai-p/P1"] {
        assert!(over.iter().any(|g| g == id), "{id}: {over:?}");
    }
}

#[test]
fn rejected_decisions_fail_the_check_that_submits_them() {
    let got = failed(&run(
        Mutation::RejectDecisions,
        &profiles("reject", GOOD_PROFILES),
    ));
    for id in ["ai-h/H2", "ai-q-a/Q1b", "ai-r-a/R2"] {
        assert!(got.iter().any(|g| g == id), "{id}: {got:?}");
    }
}

#[test]
fn the_quick_diagnostic_needs_p2_quick_and_a_play() {
    let good = profiles("trace", GOOD_PROFILES);
    assert!(notes(&run(Mutation::TraceQuick, &good), "ai-q-a", "Q3").contains("true"));
    assert!(notes(&run(Mutation::TracePartial, &good), "ai-q-a", "Q3").contains("false"));
}

#[test]
fn branches_pass_their_own_random_after_the_parent() {
    let all = positions();
    let oracle = table(&all);
    let randoms = std::rc::Rc::new(core::cell::RefCell::new(Vec::new()));
    let mut factory = || -> Box<dyn AiEngine> {
        Box::new(Recording {
            inner: FakeAi::new(Mutation::None, std::rc::Rc::clone(&oracle)),
            randoms: std::rc::Rc::clone(&randoms),
        })
    };
    check_ai(&mut factory, &all, &profiles("random", GOOD_PROFILES));
    let seen = randoms.borrow();
    let took_b5 =
        json!([{"player": "P2", "zone": "deck", "result": ["b8", "b6", "b10", "b7", "b9"]}]);
    assert!(seen.iter().any(|r| r["shuffles"] == took_b5), "{seen:?}");
}

/// A hidden card whose number is also on a public card cannot be told apart by number.
#[test]
fn a_public_copy_masks_only_its_own_card_number() {
    let mut all = positions();
    let r_a = all.0.get_mut("r-a").unwrap();
    r_a.setup["players"]["P2"]["zones"]["field"] = json!([{"id": "pz", "card": "BP01-173"}]);
    let oracle = table(&all);
    let mut factory = || -> Box<dyn AiEngine> {
        Box::new(FakeAi::new(
            Mutation::LeakRevealNumbers,
            std::rc::Rc::clone(&oracle),
        ))
    };
    let reports = check_ai(&mut factory, &all, &profiles("public", GOOD_PROFILES));
    let AiOutcome::Fail { reasons } = outcome(&reports, "ai-r-a", "R3") else {
        panic!("R3 should fail");
    };
    let text = reasons.join(" ");
    assert!(!text.contains("hidden c1"), "{text}");
    assert!(text.contains("hidden c2"), "{text}");
}
