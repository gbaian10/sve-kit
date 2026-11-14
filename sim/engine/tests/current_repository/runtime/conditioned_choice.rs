use super::*;
use std::fs::read_to_string;
use std::path::PathBuf;
use sve_engine::adapter::TypedAdapter;
use sve_scenario_runner::{AwaitingMode, RunOptions, Verdict, parse_question, run};

#[test]
fn nerusha_whole_card_passes_the_combo_zero_two_and_three_shared_exam() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let question = parse_question(
        &read_to_string(root.join("tests/rules-scenarios/questions/card-BP04-007.yaml")).unwrap(),
    )
    .unwrap();
    let reports = run(
        &mut TypedAdapter::new(selection_with_rules().clone()),
        &[question],
        None,
        RunOptions {
            require_verified: true,
            awaiting: AwaitingMode::FirstStage,
        },
    )
    .unwrap();
    assert_eq!(reports.len(), 3);
    for report in reports {
        assert!(
            matches!(report.verdict, Verdict::Pass),
            "{}: {}",
            report.scenario,
            serde_json::to_value(&report.verdict).unwrap()
        );
    }
}
