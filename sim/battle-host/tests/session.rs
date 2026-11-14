//! Local session channels over a real public-input match.

#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "Missing generated inputs or an unexpected message invalidate these fixtures."
)]

use core::array::from_fn;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{env::var_os, fs::read_to_string, path::PathBuf, sync::OnceLock};
use sve_battle_host::wire::{
    HostError, HostErrorReason, InputState, Outgoing, Receipt, ReceiptResult, Rejection, Seat,
    Submit, SubmitCommand, TemporaryAnswer, Update,
};
use sve_battle_host::{Delivery, LocalSession, SeatControl};
use sve_engine::catalog::{CurrentSelectionLoader, EngineIdentityInput, SelectedEffects};
use sve_engine::game::Game;
use sve_engine::game::player::{DeckConstruction, PlayerDeck, Viewer};
use sve_engine::host::HostLimits;

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn selection() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let snapshot = read_to_string(var_os("SVE_TEST_SNAPSHOT").unwrap()).unwrap();
        let input: Value = serde_json::from_str(
            &read_to_string(var_os("SVE_TEST_CURRENT_SELECTION").unwrap()).unwrap(),
        )
        .unwrap();
        let keywords = read_to_string(root.join("authored/rules/keywords.yaml")).unwrap();
        let mut rules: Value = serde_saphyr::from_str(
            &read_to_string(root.join("authored/rules/engine/index.yaml")).unwrap(),
        )
        .unwrap();
        // The pin names the reviewed batch; generated test inputs keep the same rule capabilities.
        *rules.pointer_mut("/input/snapshot_sha256").unwrap() =
            Value::String(hex(snapshot.as_bytes()));
        CurrentSelectionLoader::new()
            .unwrap()
            .select_with_rules(
                &snapshot,
                &keywords,
                &input,
                &rules.to_string(),
                &EngineIdentityInput::LegacyJp,
            )
            .unwrap()
    })
}

fn game(seed: &str) -> Game {
    let decks = from_fn(|_| PlayerDeck {
        construction: DeckConstruction::Class("ニュートラル".into()),
        // Two different printings so a shuffle that drifted would change later observations.
        main: [["BP10-122", "BP01-173"]; 20]
            .concat()
            .into_iter()
            .map(Into::into)
            .collect(),
        evolve: Vec::new(),
    });
    Game::start_player_game(selection().clone(), decks, Viewer::P2, seed).unwrap()
}

fn session(control: [SeatControl; 2], max_submissions: u64) -> (LocalSession, Vec<Delivery>) {
    let mut session = LocalSession::new(
        game("battle-host"),
        control,
        HostLimits {
            max_submissions,
            max_retries: 1,
        },
    );
    let started = session.start();
    (session, started)
}

fn hot_seat() -> (LocalSession, Vec<Delivery>) {
    session([SeatControl::Player, SeatControl::Player], 10_000)
}

fn answer(id: &str, request: &str) -> Submit {
    Submit {
        request_id: request.into(),
        command: SubmitCommand::Answer {
            question_id: id.into(),
            answer: TemporaryAnswer::Policy,
        },
    }
}

fn updates(deliveries: &[Delivery], seat: Seat) -> Vec<&Update> {
    deliveries
        .iter()
        .filter(|delivery| delivery.seat == seat)
        .filter_map(|delivery| match &delivery.message {
            Outgoing::Update(update) => Some(update),
            Outgoing::Receipt(_) | Outgoing::HostError(_) => None,
        })
        .collect()
}

/// The seat and id of the only open question in the latest updates.
fn open_question(latest: &[Option<Update>; 2]) -> Option<(Seat, String)> {
    latest
        .iter()
        .flatten()
        .find_map(|update| match &update.input {
            InputState::Question { id, .. } => Some((update.viewer, id.clone())),
            InputState::Waiting { .. }
            | InputState::Finished { .. }
            | InputState::EngineFault { .. } => None,
        })
}

fn absorb(latest: &mut [Option<Update>; 2], deliveries: &[Delivery]) {
    for (index, seat) in [Seat::P1, Seat::P2].into_iter().enumerate() {
        for update in updates(deliveries, seat) {
            if let Some(previous) = &latest[index] {
                assert!(update.update_seq > previous.update_seq, "{update:?}");
            }
            latest[index] = Some(update.clone());
        }
    }
}

/// Everything either seat could observe; a rejected command must leave it unchanged.
fn authority(session: &LocalSession) -> Vec<String> {
    [Viewer::P1, Viewer::P2]
        .into_iter()
        .flat_map(|viewer| {
            [
                format!("{:?}", session.game().input(viewer)),
                format!("{:?}", session.game().observe(viewer).unwrap()),
            ]
        })
        .collect()
}

fn rejected(request: &str) -> Receipt {
    Receipt {
        request_id: request.into(),
        result: ReceiptResult::Rejected,
        rejection: Some(Rejection::InvalidSubmission),
    }
}

#[test]
fn hot_seat_match_runs_to_a_result_with_receipts_first_and_per_seat_sequences() {
    let (mut session, started) = hot_seat();
    let mut latest = [None, None];
    absorb(&mut latest, &started);
    assert!(latest.iter().flatten().all(|update| update.baseline));
    let mut commands = 0_u32;
    while let Some((seat, id)) = open_question(&latest) {
        commands += 1;
        assert!(commands < 5_000, "match did not finish");
        let request = format!("r{commands}");
        let out = session.submit(seat, answer(&id, &request));
        let first = out.first().unwrap();
        assert_eq!(first.seat, seat);
        assert!(matches!(&first.message,
            Outgoing::Receipt(receipt) if receipt.request_id == request
                && receipt.result == ReceiptResult::Applied));
        assert!(
            out.iter()
                .skip(1)
                .all(|delivery| matches!(delivery.message, Outgoing::Update(_)))
        );
        absorb(&mut latest, &out);
    }
    for update in latest.iter().flatten() {
        assert!(matches!(
            update.input,
            InputState::Finished { winner: Some(_) }
        ));
    }
    assert_eq!(session.stopped(), None);
}

#[test]
fn stale_wrong_seat_unknown_and_repeated_answers_are_invalid_and_change_nothing() {
    let (mut session, started) = hot_seat();
    let mut latest = [None, None];
    absorb(&mut latest, &started);
    let (seat, id) = open_question(&latest).unwrap();
    let other = if seat == Seat::P1 { Seat::P2 } else { Seat::P1 };
    let before = authority(&session);
    for (from, question, request) in [
        (other, id.as_str(), "wrong-seat"),
        (seat, "q999", "unknown"),
    ] {
        let out = session.submit(from, answer(question, request));
        assert_eq!(
            out,
            vec![Delivery {
                seat: from,
                message: Outgoing::Receipt(rejected(request)),
            }]
        );
    }
    assert_eq!(authority(&session), before);
    let applied = session.submit(seat, answer(&id, "first"));
    absorb(&mut latest, &applied);
    let after = authority(&session);
    for request in ["repeat", "repeat-again"] {
        let out = session.submit(seat, answer(&id, request));
        assert_eq!(
            out,
            vec![Delivery {
                seat,
                message: Outgoing::Receipt(rejected(request)),
            }]
        );
    }
    assert_eq!(authority(&session), after);
    // The next real question still works after the rejections.
    let (next_seat, next) = open_question(&latest).unwrap();
    assert_ne!(next, id);
    let out = session.submit(next_seat, answer(&next, "next"));
    assert!(matches!(&out[0].message,
        Outgoing::Receipt(receipt) if receipt.result == ReceiptResult::Applied));
}

#[test]
fn baselines_are_queries_that_keep_rules_randomness_and_sequence() {
    let (mut session, started) = hot_seat();
    let before = authority(&session);
    for seat in [Seat::P1, Seat::P2, Seat::P1] {
        let again = session.baseline(seat);
        let original = updates(&started, seat);
        assert_eq!(updates(&again, seat), original);
    }
    assert_eq!(authority(&session), before);
}

/// Valid deliveries and final state of a hot-seat run; `noise` adds rejected commands
/// and baseline queries before every valid command.
fn scripted(noise: bool) -> (Vec<Delivery>, Vec<String>) {
    let (mut session, started) = hot_seat();
    let mut transcript = started.clone();
    let mut latest = [None, None];
    absorb(&mut latest, &started);
    let mut previous = String::new();
    for request in 0..60 {
        let Some((seat, id)) = open_question(&latest) else {
            break;
        };
        if noise {
            let other = if seat == Seat::P1 { Seat::P2 } else { Seat::P1 };
            for (from, question) in [(other, id.as_str()), (seat, previous.as_str())] {
                let out = session.submit(from, answer(question, "noise"));
                assert_eq!(
                    out,
                    vec![Delivery {
                        seat: from,
                        message: Outgoing::Receipt(rejected("noise")),
                    }]
                );
            }
            let _baseline = session.baseline(Seat::P1);
            let _baseline = session.baseline(Seat::P2);
        }
        let out = session.submit(seat, answer(&id, &format!("r{request}")));
        absorb(&mut latest, &out);
        transcript.extend(out);
        previous = id;
    }
    (transcript, authority(&session))
}

#[test]
fn rejected_commands_and_queries_do_not_advance_rules_or_randomness() {
    let plain = scripted(false);
    assert_eq!(plain, scripted(false));
    assert_eq!(plain, scripted(true));
}

#[test]
fn concede_while_waiting_finishes_for_both_seats() {
    let (mut session, started) = hot_seat();
    let mut latest = [None, None];
    absorb(&mut latest, &started);
    let (seat, _id) = open_question(&latest).unwrap();
    let waiting = if seat == Seat::P1 { Seat::P2 } else { Seat::P1 };
    let out = session.submit(
        waiting,
        Submit {
            request_id: "give-up".into(),
            command: SubmitCommand::Concede,
        },
    );
    assert!(matches!(&out[0].message,
        Outgoing::Receipt(receipt) if receipt.result == ReceiptResult::Applied));
    absorb(&mut latest, &out);
    for update in latest.iter().flatten() {
        assert_eq!(update.input, InputState::Finished { winner: Some(seat) });
    }
    let again = session.submit(
        waiting,
        Submit {
            request_id: "again".into(),
            command: SubmitCommand::Concede,
        },
    );
    assert_eq!(
        again,
        vec![Delivery {
            seat: waiting,
            message: Outgoing::Receipt(rejected("again")),
        }]
    );
}

#[test]
fn policy_seat_answers_inside_the_host_and_has_no_channel() {
    let (mut session, started) = session([SeatControl::Player, SeatControl::Policy], 10_000);
    assert!(started.iter().all(|delivery| delivery.seat == Seat::P1));
    let mut latest = [None, None];
    absorb(&mut latest, &started);
    let out = session.submit(Seat::P2, answer("q1", "not-a-channel"));
    assert_eq!(
        out,
        vec![Delivery {
            seat: Seat::P2,
            message: Outgoing::Receipt(rejected("not-a-channel")),
        }]
    );
    let mut commands = 0_u32;
    while let Some((seat, id)) = open_question(&latest) {
        commands += 1;
        assert!(commands < 5_000, "match did not finish");
        assert_eq!(seat, Seat::P1);
        let out = session.submit(seat, answer(&id, "r"));
        assert!(out.iter().all(|delivery| delivery.seat == Seat::P1));
        absorb(&mut latest, &out);
    }
    assert!(matches!(
        latest[0].as_ref().unwrap().input,
        InputState::Finished { winner: Some(_) }
    ));
}

#[test]
fn exhausted_budget_and_malformed_messages_stop_interaction() {
    let (session, started) = session([SeatControl::Policy, SeatControl::Policy], 3);
    assert!(started.is_empty());
    assert_eq!(session.stopped(), Some(HostErrorReason::Budget));

    let (mut session, _started) = hot_seat();
    let out = session.submit_json(Seat::P1, r#"{"request_id":"x","command":{"kind":"guess"}}"#);
    let stopped = |seat| Delivery {
        seat,
        message: Outgoing::HostError(HostError {
            format: 0,
            reason: HostErrorReason::Format,
        }),
    };
    assert_eq!(out, vec![stopped(Seat::P1), stopped(Seat::P2)]);
    let valid = r#"{"request_id":"y","command":{"kind":"concede"}}"#;
    assert_eq!(
        session.submit_json(Seat::P2, valid),
        vec![stopped(Seat::P2)]
    );
    assert_eq!(session.submit_json(Seat::P2, "{"), vec![stopped(Seat::P2)]);
    assert_eq!(session.baseline(Seat::P1), vec![stopped(Seat::P1)]);
}

#[test]
fn wire_messages_use_the_temporary_format_and_snake_case_tags() {
    let (_session, started) = hot_seat();
    let json = serde_json::to_value(&started[0].message).unwrap();
    assert_eq!(json["kind"], "update");
    assert_eq!(json["format"], 0);
    assert_eq!(json["baseline"], true);
    assert!(json["viewer"] == "p1" || json["viewer"] == "p2");
    let receipt = serde_json::to_value(Outgoing::Receipt(rejected("r"))).unwrap();
    assert_eq!(
        receipt,
        serde_json::json!({"kind":"receipt","request_id":"r","result":"rejected",
            "rejection":"invalid_submission"})
    );
}
