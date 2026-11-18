//! Recovery and architecture checks through the typed authority record.

#![cfg(feature = "runner")]

use core::array::from_fn;
use serde_json::{Value, json};
use std::path::PathBuf;
use sve_engine::adapter::TypedReplayAdapter;
use sve_engine::game::player::observation::ObservedZone;
use sve_engine::game::player::{
    Command, DeckConstruction, Input, OpeningAnswer, OpeningQuestion, PlayerDeck, QuestionShape,
    SubmissionKind, Viewer,
};
use sve_engine::game::replay::{Replay, ReplayNode};
use sve_scenario_runner::arch::{
    ArchOptions, BranchKind, CheckReport, Outcome, ReplayEngine, check_replay, load_fixtures,
};
use sve_scenario_runner::{EngineError, Fixture, View};

#[path = "support/repository.rs"]
mod repository;
#[path = "support/rule_fixtures.rs"]
mod rule_fixtures;

#[test]
fn architecture_replay_checks_use_the_typed_entry() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixtures = load_fixtures(&root.join("tests/architecture-fixtures")).unwrap();
    let selected = repository::selection_with_rules();
    let mut factory =
        || -> Box<dyn ReplayEngine> { Box::new(TypedReplayAdapter::new(selected.clone())) };
    let reports = check_replay(&mut factory, &fixtures, &ArchOptions::default());
    let failed: Vec<_> = reports
        .iter()
        .filter(|report| !matches!(report.id, "R2b" | "R5") && report.outcome != Outcome::Pass)
        .collect();
    assert_eq!(failed, Vec::<&CheckReport>::new());

    // R2b still requires memory of an undisclosed nonmatching card.
    let pending = reports.iter().find(|report| report.id == "R2b").unwrap();
    let Outcome::Fail { reasons } = &pending.outcome else {
        panic!("unexpected R2b result: {:?}", pending.outcome);
    };
    let missing =
        "replay branch: P1 does not carry a5 from a node of the source line where it knew it";
    assert!(reasons.iter().any(|reason| reason == missing));
    assert!(
        reasons.iter().all(|reason| {
            reason == missing
                || (reason.starts_with("replay branch N") && reason.ends_with("P1 forgot a5"))
        }),
        "{reasons:?}"
    );

    // Drawing on the full source lines gives P1 different knowledge to carry into each fork.
    let r5 = reports.iter().find(|report| report.id == "R5").unwrap();
    let Outcome::Fail {
        reasons: r5_reasons,
    } = &r5.outcome
    else {
        panic!("unexpected R5 result: {:?}", r5.outcome);
    };
    let expected: Vec<_> = (5_u8..=12)
        .map(|node| format!("M' shuffles branch N{node} P1: exports differ"))
        .collect();
    assert_eq!(r5_reasons, &expected);
}

#[test]
fn strict_replay_rebuilds_a_shuffled_opening_and_continues_the_pending_mulligan() {
    let decks = from_fn(|_| PlayerDeck {
        construction: DeckConstruction::Class("ニュートラル".into()),
        main: ["BP01-P25", "BP10-122"]
            .into_iter()
            .cycle()
            .take(12)
            .map(str::to_owned)
            .collect(),
        evolve: vec![],
    });
    let mut replay = Replay::start(
        repository::selection_with_rules().clone(),
        decks,
        Viewer::P2,
        "replay-opening".into(),
    )
    .unwrap();
    let Input::Question(first) = replay.summary(ReplayNode::ROOT, Viewer::P2).unwrap().input else {
        panic!("expected first-player choice");
    };
    let command = Command::Opening {
        token: first.token,
        answer: OpeningAnswer::FirstPlayer(Viewer::P1),
    };
    let (dealt, _) = replay
        .submit(ReplayNode::ROOT, Viewer::P2, command)
        .unwrap();
    let own = replay.summary(dealt, Viewer::P1).unwrap();
    let Input::Question(mulligan) = own.input else {
        panic!("expected mulligan");
    };
    let mut recovered = replay.recover(dealt).unwrap();
    assert_eq!(recovered.observe(Viewer::P1).unwrap(), own.observation);
    let other = replay.summary(dealt, Viewer::P2).unwrap();
    assert!(matches!(other.input, Input::Waiting(waiting) if waiting.actor == Viewer::P1));
    assert_eq!(recovered.observe(Viewer::P2).unwrap(), other.observation);
    let hidden = other
        .observation
        .players
        .first()
        .unwrap()
        .zones
        .iter()
        .find(|zone| zone.kind == ObservedZone::Hand)
        .unwrap();
    assert_eq!(hidden.count, 4);
    assert_eq!(hidden.cards.len(), 0);
    let QuestionShape::Opening(OpeningQuestion::Mulligan(cards)) = mulligan.shape else {
        panic!("expected mulligan cards");
    };
    let redraw = Command::Opening {
        token: mulligan.token,
        answer: OpeningAnswer::Mulligan(cards.into_iter().rev().map(|card| card.handle).collect()),
    };
    let (next, uninterrupted) = replay.submit(dealt, Viewer::P1, redraw.clone()).unwrap();
    assert_eq!(uninterrupted.kind, SubmissionKind::Applied);
    assert_eq!(recovered.submit_player(Viewer::P1, redraw), uninterrupted);
    assert_eq!(
        recovered.observe(Viewer::P1).unwrap(),
        replay.summary(next, Viewer::P1).unwrap().observation
    );
}

fn position(zones: &Value, random: Value) -> Fixture {
    Fixture {
        question: "typed-replay".into(),
        scenario: "resume".into(),
        setup: json!({
            "turn": {"active":"P1","first_player":"P1","phase":"main","elapsed_turns":{"P1":3,"P2":2}},
            "players": {
                "P1": {"leader":{"class":"ニュートラル","life":20},
                       "pp":{"current":10,"max":10},"zones":zones},
                "P2": {"leader":{"class":"ニュートラル","life":20},
                       "pp":{"current":0,"max":10},
                       "zones":{"hand":[{"id":"private-opponent","card":"BP01-173"}]}}
            }
        }),
        card_facts: Value::Null,
        random,
    }
}

fn adapter() -> TypedReplayAdapter {
    TypedReplayAdapter::new(repository::selection_with_rules().clone())
}

#[test]
fn save_replays_the_rule_bound_stack_selection_and_preserves_branch_privacy() {
    let fixture = position(
        &json!({"field":[
            {"id":"source","card":"BP01-075","state":{"counters":{"stack_counter":3_u8}}},
            {"id":"recipient","card":"BP03-T02","state":{"acted":true,"counters":{"stack_counter":2_u8}}}
        ]}),
        Value::Null,
    );
    let mut first = adapter();
    let root = first.start(&fixture, "bound-stack", "stack").unwrap();
    let input = first.observe(&root, View::P1).unwrap().awaiting.unwrap();
    let intent = input["choices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|choice| choice["do"] == "activate" && choice["ability"]["source"] == "source")
        .unwrap();
    let (pending, step) = first
        .decide(
            &root,
            &json!({"by":"P1","at":"main","do":"activate","ability":intent["ability"]}),
        )
        .unwrap();
    assert_eq!(step.outcome, "paused");
    let mut restored = adapter();
    assert_eq!(
        restored.restore(&first.save(&pending).unwrap()).unwrap(),
        pending
    );
    assert_eq!(
        restored.observe(&pending, View::P1).unwrap(),
        first.observe(&pending, View::P1).unwrap()
    );
    let answer = json!({"by":"P1","at":"resolve","do":"resolve-choice","select":["recipient"]});
    let (_resolved, live) = first.decide(&pending, &answer).unwrap();
    let (resolved, recovered) = restored.decide(&pending, &answer).unwrap();
    assert_eq!(recovered.events, live.events);
    assert_eq!(
        restored
            .query(
                &resolved,
                View::Omniscient,
                "P1.field.recipient.counters.スタックカウンター"
            )
            .unwrap(),
        Some(json!(5_u8))
    );
    let branch = restored.branch(&root, BranchKind::Undo).unwrap();
    assert_eq!(
        restored.restore(&restored.save(&branch).unwrap()).unwrap(),
        branch
    );
    assert!(
        !restored
            .export(&branch, View::P1)
            .unwrap()
            .to_string()
            .contains("private-opponent")
    );
}

#[test]
fn scripted_random_inputs_resume_in_order() {
    let fixture = position(
        &json!({
            "hand":[{"id":"spell","card":"BP21-080"}],
            "deck":[{"id":"drawn","card":"BP01-171"}]
        }),
        json!({"dice":[2_u8,5_u8]}),
    );
    let mut game = adapter();
    let root = game.start(&fixture, "scripted", "script").unwrap();
    let prefix = game.save(&root).unwrap();
    let mut restored = adapter();
    let start = restored.restore(&prefix).unwrap();
    let (_next, step) = restored
        .decide(
            &start,
            &json!({"by":"P1","at":"main","do":"play","card":"spell"}),
        )
        .unwrap();
    let rolls: Vec<_> = step
        .events
        .iter()
        .filter(|event| event["kind"] == "サイコロ")
        .map(|event| event["result"].clone())
        .collect();
    assert_eq!(rolls, [json!(2_u8), json!(5_u8)]);
}

#[test]
fn incomplete_random_script_recovers_the_fault_and_rolls_back_the_draw() {
    let fixture = position(
        &json!({
            "hand":[{"id":"spell","card":"BP21-080"}],
            "deck":[{"id":"drawn","card":"BP01-171"}]
        }),
        json!({"dice":[2_u8]}),
    );
    let mut game = adapter();
    let root = game.start(&fixture, "scripted-fault", "script").unwrap();
    let (faulted, step) = game
        .decide(
            &root,
            &json!({"by":"P1","at":"main","do":"play","card":"spell"}),
        )
        .unwrap();
    assert_eq!(step.outcome, "engine-fault");
    let mut restored = adapter();
    assert_eq!(
        restored.restore(&game.save(&faulted).unwrap()).unwrap(),
        faulted
    );
    assert_eq!(
        restored
            .query(&faulted, View::Omniscient, "P1.deck")
            .unwrap(),
        Some(json!(["drawn"]))
    );
    restored
        .decide(&faulted, &json!({"by":"P1","at":"main","do":"end-phase"}))
        .unwrap_err();
}

#[test]
fn private_save_requires_the_random_algorithm_version() {
    let mut game = adapter();
    let root = game
        .start(&position(&json!({}), Value::Null), "version", "version")
        .unwrap();
    let mut archive: Value = serde_json::from_slice(&game.save(&root).unwrap()).unwrap();
    archive.as_object_mut().unwrap().remove("random_algorithm");
    adapter()
        .restore(&serde_json::to_vec(&archive).unwrap())
        .unwrap_err();
}

#[test]
fn private_restore_rejects_a_tampered_command_receipt() {
    let mut game = adapter();
    let root = game
        .start(&position(&json!({}), Value::Null), "tampered", "tampered")
        .unwrap();
    let (next, _) = game
        .decide(&root, &json!({"by":"P1","at":"main","do":"end-phase"}))
        .unwrap();
    let mut archive: Value = serde_json::from_slice(&game.save(&next).unwrap()).unwrap();
    assert_eq!(archive["checkpoints"][1]["receipts"][0], "Applied");
    archive["checkpoints"][1]["receipts"][0] = json!("Cancelled");
    let failure = game
        .restore(&serde_json::to_vec(&archive).unwrap())
        .unwrap_err();
    assert!(matches!(failure, EngineError::Adapter(message)
        if message.contains("recorded replay checkpoint did not reproduce its result")));
}
