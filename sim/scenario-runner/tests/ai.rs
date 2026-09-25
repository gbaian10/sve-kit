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
    assert!(notes(&reports, "ai-q-a", "Q3").contains("[]"));
    assert_eq!(
        reports.iter().filter(|r| r.id == "R4").count(),
        1,
        "one R4 report per pair"
    );
    let pair = notes(&reports, "ai-r-a", "R4");
    assert!(
        pair.contains("r-a: chose") && pair.contains("r-b: chose"),
        "{pair}"
    );
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
        (
            Mutation::AlwaysResolved,
            &["ai-h/H2", "ai-r-a/R2", "ai-r-b/R2"],
        ),
        (Mutation::GameEnd, &["ai-h/H2", "ai-q-a/Q1b", "ai-r-a/R2"]),
        (
            Mutation::WrongPoint,
            &["ai-h/H2", "ai-q-b/Q1b", "ai-r-b/R2"],
        ),
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

fn edge(
    edge: u64,
    parent: Option<u64>,
    sample: u64,
    by: &str,
    rule: &str,
    decision: &Value,
) -> Value {
    // Only P2's answer happens at a Quick point; P1 acts in the main phase.
    let at = if by == "P2" { "quick" } else { "main" };
    json!({"edge": edge, "parent": parent, "sample": sample, "by": by, "at": at,
           "rule": rule, "decision": decision})
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

/// Sample 0 assumes P2 holds エンジェルスナイプ and answers the attack with it;
/// sample 1 assumes a ファイター and ends the turn.
fn good_log() -> Vec<Value> {
    let mut root0 = edge(
        1,
        None,
        0,
        "P1",
        "7.3",
        &json!({"do": "attack", "attacker": "a1"}),
    );
    root0["sample_hand"] = json!([{"id": "s0-h", "card": "BP01-179"}]);
    let mut root1 = edge(3, None, 1, "P1", "7.3", &json!({"do": "end-phase"}));
    root1["sample_hand"] = json!([{"id": "s1-h", "card": "BP01-173"}]);
    vec![
        root0,
        edge(
            2,
            Some(1),
            0,
            "P2",
            "8.4.7",
            &json!({"do": "play", "card": "s0-h", "targets": {"1": ["a1"]}}),
        ),
        root1,
    ]
}

fn q(stem: &str) -> sve_scenario_runner::ai::AiPosition {
    positions().0.remove(stem).unwrap()
}

fn log_ok(log: &[Value], edges: u64, budget: u64, stem: &str) -> bool {
    check_search_log(log, &report(edges), budget, &q(stem)).is_empty()
}

#[test]
fn a_tree_log_with_a_sampled_quick_answer_passes() {
    // The same log passes in both variants: in q-b the real hand has no Quick card.
    assert!(log_ok(&good_log(), 3, 5000, "q-a"));
    assert!(log_ok(&good_log(), 3, 5000, "q-b"));
    assert!(log_ok(&good_log(), 3, 3, "q-b"));
    let mut named = good_log();
    named[0]["edge"] = json!("root");
    named[1]["parent"] = json!("root");
    assert!(log_ok(&named, 3, 5000, "q-b"));
}

#[test]
fn search_logs_that_prove_nothing_fail() {
    let fails = |log: &[Value]| !log_ok(log, 3, 5000, "q-b");
    let mut flat = good_log();
    flat[1]["parent"] = Value::Null;
    assert!(fails(&flat), "a flat log");
    let mut switched = good_log();
    switched[1]["sample"] = json!(1);
    assert!(fails(&switched), "the sample switches along the path");
    // Even when the other sample could also have played it.
    let mut both = switched.clone();
    both[2]["sample_hand"] = json!([{"id": "s0-h", "card": "BP01-179"}]);
    assert!(
        fails(&both),
        "the sample switches, both hands hold the card"
    );
    let mut unsampled = good_log();
    for entry in &mut unsampled {
        entry.as_object_mut().unwrap().remove("sample");
    }
    assert!(fails(&unsampled), "no sample field");
    assert!(
        !log_ok(&good_log(), 4, 5000, "q-b"),
        "count differs from the report"
    );
    assert!(!log_ok(&good_log(), 3, 2, "q-b"), "over budget");
    let mut at_main = good_log();
    at_main[1]["at"] = json!("main");
    assert!(fails(&at_main), "the Quick edge is not at a Quick point");
    let mut no_at = good_log();
    no_at[1].as_object_mut().unwrap().remove("at");
    assert!(fails(&no_at), "the Quick edge has no at");
    let mut end = good_log();
    end[1]["rule"] = json!("7.4.5");
    assert!(fails(&end), "only the end-phase Quick");
    let mut later = good_log();
    later.swap(0, 1);
    assert!(fails(&later), "parent after child");
    let mut twice = good_log();
    twice[2]["edge"] = json!(1);
    assert!(fails(&twice), "repeated edge");
    let mut unnamed = good_log();
    unnamed[2]["edge"] = Value::Null;
    assert!(fails(&unnamed), "edge without id");
    let mut rooted = good_log();
    rooted[0]["decision"] = json!({"do": "end-phase"});
    assert!(fails(&rooted), "root is not an attack");
}

#[test]
fn the_quick_answer_must_come_from_the_samples_hand() {
    let fails = |log: &[Value]| !log_ok(log, 3, 5000, "q-b");
    let mut not_quick = good_log();
    not_quick[0]["sample_hand"] = json!([{"id": "s0-h", "card": "BP01-173"}]);
    assert!(fails(&not_quick), "a ファイター is not a Quick card");
    let mut elsewhere = good_log();
    elsewhere[1]["decision"]["card"] = json!("h1");
    assert!(
        fails(&elsewhere),
        "the played card is not in the sample's hand"
    );
    let mut no_hand = good_log();
    no_hand[0].as_object_mut().unwrap().remove("sample_hand");
    assert!(fails(&no_hand), "a root without sample_hand");
    let mut two_hands = good_log();
    let mut extra = edge(4, None, 0, "P1", "7.3", &json!({"do": "end-phase"}));
    extra["sample_hand"] = json!([{"id": "x", "card": "BP01-173"}]);
    two_hands.push(extra);
    assert!(!log_ok(&two_hands, 4, 5000, "q-b"), "one sample, two hands");
    let mut same_hand = good_log();
    let mut again = edge(4, None, 0, "P1", "7.3", &json!({"do": "end-phase"}));
    again["sample_hand"] = same_hand[0]["sample_hand"].clone();
    same_hand.push(again);
    assert!(
        log_ok(&same_hand, 4, 5000, "q-b"),
        "roots of one sample agreeing"
    );
    let mut big = good_log();
    big[0]["sample_hand"] =
        json!([{"id": "s0-h", "card": "BP01-179"}, {"id": "z", "card": "BP01-173"}]);
    assert!(fails(&big), "hand size differs from P2's");
    let mut unlisted = good_log();
    unlisted[2]["sample_hand"] = json!([{"id": "s1-h", "card": "BP99-999"}]);
    assert!(fails(&unlisted), "a card outside the public deck list");
    // Without PP the Quick cannot be paid for.
    let mut poor = q("q-b");
    poor.setup["players"]["P2"]["pp"]["current"] = json!(0);
    assert!(!check_search_log(&good_log(), &report(3), 5000, &poor).is_empty());
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
    assert!(notes(&run(Mutation::TraceQuick, &good), "ai-q-a", "Q3").contains("BP01-179"));
    assert!(notes(&run(Mutation::TracePartial, &good), "ai-q-a", "Q3").contains("[]"));
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
