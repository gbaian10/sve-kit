//! Validates the architecture checks with scripted engines: a correct one passes every
//! assertion, and each broken skeleton is caught by the assertion meant for it.

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::absolute_paths,
    clippy::std_instead_of_alloc,
    clippy::default_numeric_fallback,
    clippy::too_many_lines,
    clippy::integer_division_remainder_used,
    clippy::unwrap_used,
    reason = "test code: scripted tables, and a panic is a test failure"
)]

mod arch_support;

use std::path::PathBuf;

use arch_support::{
    ASSIST_MUTATIONS, AssistMutation, FakeAssist, FakeReplay, REPLAY_MUTATIONS, ReplayMutation,
};
use serde_json::Value;
use sve_scenario_runner::arch::{
    ArchFixtures, ArchOptions, AssistEngine, BranchKind, CheckReport, Layer, Layered, NodeId,
    Observation, Outcome, Realign, ReplayEngine, check_assist, check_replay, load_fixtures,
};
use sve_scenario_runner::{EngineError, Fixture, Step, View};

fn fixtures() -> ArchFixtures {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/architecture-fixtures");
    load_fixtures(&dir).unwrap()
}

/// The fake puts its node id at `node`; approved before running, as the design requires.
fn options() -> ArchOptions {
    ArchOptions {
        node_id_paths: vec!["node".to_owned(), "knowledge.carried.*.from".to_owned()],
        skip_paths: Vec::new(),
    }
}

fn replay(mutation: ReplayMutation) -> Vec<CheckReport> {
    let fx = fixtures();
    let mut factory = || -> Box<dyn ReplayEngine> { Box::new(FakeReplay::new(mutation)) };
    check_replay(&mut factory, &fx, &options())
}

fn assist(mutation: AssistMutation) -> Vec<CheckReport> {
    let fx = fixtures();
    let mut factory = || -> Box<dyn AssistEngine> { Box::new(FakeAssist::new(mutation)) };
    check_assist(&mut factory, &fx, &options())
}

fn outcome<'rep>(reports: &'rep [CheckReport], id: &str) -> &'rep Outcome {
    &reports.iter().find(|r| r.id == id).unwrap().outcome
}

#[test]
fn a_correct_replay_skeleton_passes_every_check() {
    let reports = replay(ReplayMutation::None);
    assert_eq!(reports.len(), 8);
    for r in &reports {
        assert_eq!(r.outcome, Outcome::Pass, "{}", r.id);
    }
}

#[test]
fn a_correct_assist_skeleton_passes_every_check() {
    let reports = assist(AssistMutation::None);
    assert_eq!(reports.len(), 7);
    for r in &reports {
        assert_eq!(r.outcome, Outcome::Pass, "{}", r.id);
    }
}

#[test]
fn every_broken_replay_skeleton_is_caught() {
    for (mutation, ids) in REPLAY_MUTATIONS {
        let reports = replay(*mutation);
        for id in *ids {
            assert!(
                matches!(outcome(&reports, id), Outcome::Fail { .. }),
                "{mutation:?} should fail {id}: {:?}",
                outcome(&reports, id)
            );
        }
    }
}

#[test]
fn every_broken_assist_skeleton_is_caught() {
    for (mutation, ids) in ASSIST_MUTATIONS {
        let reports = assist(*mutation);
        for id in *ids {
            assert!(
                matches!(outcome(&reports, id), Outcome::Fail { .. }),
                "{mutation:?} should fail {id}: {:?}",
                outcome(&reports, id)
            );
        }
    }
}

#[test]
fn an_unapproved_transport_field_fails_the_paired_checks_until_approved() {
    let fx = fixtures();
    let mut factory =
        || -> Box<dyn ReplayEngine> { Box::new(FakeReplay::new(ReplayMutation::TransportCounter)) };
    let reports = check_replay(&mut factory, &fx, &options());
    assert!(matches!(outcome(&reports, "R5"), Outcome::Fail { .. }));
    let mut approved = options();
    approved.skip_paths.push("transport".to_owned());
    let approved_reports = check_replay(&mut factory, &fx, &approved);
    assert_eq!(outcome(&approved_reports, "R5"), &Outcome::Pass);
}

#[test]
fn missing_fixture_files_are_errors() {
    load_fixtures(&PathBuf::from("/nonexistent/arch")).unwrap_err();
}

/// Refuses everything with one error, to check how errors are reported.
struct Refusing(EngineError);

impl ReplayEngine for Refusing {
    fn start(&mut self, _: &Fixture, _: &str, _: &str) -> Result<NodeId, EngineError> {
        Err(self.0.clone())
    }
    fn decide(&mut self, _: &NodeId, _: &Value) -> Result<(NodeId, Step), EngineError> {
        Err(self.0.clone())
    }
    fn branch(&mut self, _: &NodeId, _: BranchKind) -> Result<NodeId, EngineError> {
        Err(self.0.clone())
    }
    fn save(&self, _: &NodeId) -> Result<Vec<u8>, EngineError> {
        Err(self.0.clone())
    }
    fn restore(&mut self, _: &[u8]) -> Result<NodeId, EngineError> {
        Err(self.0.clone())
    }
    fn export(&self, _: &NodeId, _: View) -> Result<Value, EngineError> {
        Err(self.0.clone())
    }
    fn digest(&self, _: &NodeId) -> Result<String, EngineError> {
        Err(self.0.clone())
    }
    fn observe(&self, _: &NodeId, _: View) -> Result<Observation, EngineError> {
        Err(self.0.clone())
    }
    fn query(&self, _: &NodeId, _: View, _: &str) -> Result<Option<Value>, EngineError> {
        Err(self.0.clone())
    }
    fn decisions(&self, _: &NodeId) -> Result<Vec<Value>, EngineError> {
        Err(self.0.clone())
    }
    fn events(&self, _: &NodeId) -> Result<Vec<Value>, EngineError> {
        Err(self.0.clone())
    }
    fn admin_log(&self) -> Result<Vec<Value>, EngineError> {
        Err(self.0.clone())
    }
}

impl AssistEngine for Refusing {
    fn start(&mut self, _: &Fixture, _: &str, _: &str) -> Result<(), EngineError> {
        Err(self.0.clone())
    }
    fn act(&mut self, _: &Value) -> Result<Layered<Step>, EngineError> {
        Err(self.0.clone())
    }
    fn divergences(&self) -> Result<Vec<Value>, EngineError> {
        Err(self.0.clone())
    }
    fn auto_advance(&self) -> Result<bool, EngineError> {
        Err(self.0.clone())
    }
    fn advance(&mut self) -> Result<Option<Layered<Step>>, EngineError> {
        Err(self.0.clone())
    }
    fn realign(&mut self, _: &str, _: Realign) -> Result<(), EngineError> {
        Err(self.0.clone())
    }
    fn digest(&self, _: Layer) -> Result<String, EngineError> {
        Err(self.0.clone())
    }
    fn observe(&self, _: Layer, _: View) -> Result<Observation, EngineError> {
        Err(self.0.clone())
    }
    fn query(&self, _: Layer, _: View, _: &str) -> Result<Option<Value>, EngineError> {
        Err(self.0.clone())
    }
}

#[test]
fn engine_errors_are_reported_apart_from_failures() {
    let fx = fixtures();
    for (error, want) in [
        (
            EngineError::Unsupported("no".into()),
            Outcome::Unsupported {
                reason: "no".into(),
            },
        ),
        (
            EngineError::Adapter("broken".into()),
            Outcome::AdapterError {
                reason: "broken".into(),
            },
        ),
    ] {
        let mut replay_factory = || -> Box<dyn ReplayEngine> { Box::new(Refusing(error.clone())) };
        let mut assist_factory = || -> Box<dyn AssistEngine> { Box::new(Refusing(error.clone())) };
        let reports = check_replay(&mut replay_factory, &fx, &options());
        let reports = reports
            .iter()
            .chain(check_assist(&mut assist_factory, &fx, &options()).iter())
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(reports.len(), 15);
        for r in &reports {
            assert_eq!(r.outcome, want, "{}", r.id);
        }
    }
}
