//! Channel boundaries on the smallest synthetic match: two vanilla cards, no effects.

#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "A malformed synthetic fixture or an unexpected message invalidates these tests."
)]

use serde_json::{Value, json};
use std::sync::Arc;
use sve_battle_host::wire::{
    HostError, HostErrorReason, InputState, Outgoing, Receipt, ReceiptResult, Rejection, Seat,
    Submit, SubmitCommand, TemporaryAnswer, Update,
};
use sve_battle_host::{Delivery, LocalSession, SeatControl};
use sve_engine::catalog::Catalog;
use sve_engine::game::Game;
use sve_engine::host::HostLimits;

fn card(number: &str, power: &str, hp: &str) -> String {
    json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル",
        "card_type":"フォロワー","cost":"1","power":power,"hp":hp,"traits":[],"text":null,
        "sections":[]}]})
    .to_string()
}

/// A pregame opening; two different cards make a drifted shuffle visible later.
fn game() -> Game {
    let snapshot = [card("t-small", "1", "1"), card("t-large", "3", "3")].join("\n");
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    let effects = json!({"format":1,"kind":"effect_set","cards":{
        "t-small":{"status":"complete","review":"synthetic","abilities":[]},
        "t-large":{"status":"complete","review":"synthetic","abilities":[]}
    }})
    .to_string();
    let catalog =
        Catalog::from_documents(&snapshot, &registry, &[("t.yaml".into(), effects)]).unwrap();
    let deck = |seat: &str| {
        (0..10)
            .map(|index| {
                let number = if index % 2 == 0 { "t-small" } else { "t-large" };
                json!({"id":format!("{seat}/deck/{index}"),"card":number})
            })
            .collect::<Vec<_>>()
    };
    let player = |seat: &str| {
        json!({"leader":{"class":"ニュートラル"},"construction":"class",
            "deck_list":deck(seat),"evolve_deck_list":[]})
    };
    let setup = json!({"pregame":true,"players":{"P1":player("P1"),"P2":player("P2")}});
    let mut game = Game::new(
        Arc::new(catalog),
        &setup,
        &Value::Null,
        &json!({"first_chooser":"P1"}),
        "battle-host-synthetic",
    )
    .unwrap();
    game.advance_player_input();
    game
}

fn session(control: [SeatControl; 2], max_submissions: u64) -> LocalSession {
    LocalSession::new(
        game(),
        control,
        HostLimits {
            max_submissions,
            max_retries: 1,
        },
    )
}

const HOT_SEAT: [SeatControl; 2] = [SeatControl::Player, SeatControl::Player];

fn answer(id: &str, request: &str) -> Submit {
    Submit {
        request_id: request.into(),
        command: SubmitCommand::Answer {
            question_id: id.into(),
            answer: TemporaryAnswer::Policy,
        },
    }
}

const fn concede(request: String) -> Submit {
    Submit {
        request_id: request,
        command: SubmitCommand::Concede,
    }
}

/// Exact authority state, including randomness.
fn authority(session: &LocalSession) -> String {
    format!("{:?}", session.game())
}

fn rejected(seat: Seat, request: &str) -> Vec<Delivery> {
    vec![Delivery {
        seat,
        message: Outgoing::Receipt(Receipt {
            request_id: request.into(),
            result: ReceiptResult::Rejected,
            rejection: Some(Rejection::InvalidSubmission),
        }),
    }]
}

fn stopped(seat: Seat, reason: HostErrorReason) -> Delivery {
    Delivery {
        seat,
        message: Outgoing::HostError(HostError { format: 0, reason }),
    }
}

/// Latest update per seat, checking that each seat's sequence only grows.
#[derive(Default)]
struct Seats([Option<Update>; 2]);

impl Seats {
    fn absorb(&mut self, deliveries: &[Delivery]) {
        for delivery in deliveries {
            let Outgoing::Update(update) = &delivery.message else {
                continue;
            };
            let slot = &mut self.0[usize::from(delivery.seat == Seat::P2)];
            if let Some(previous) = slot.as_ref() {
                assert!(update.update_seq > previous.update_seq, "{update:?}");
            }
            *slot = Some(update.clone());
        }
    }

    fn question(&self) -> Option<(Seat, String)> {
        self.0
            .iter()
            .flatten()
            .find_map(|update| match &update.input {
                InputState::Question { id, .. } => Some((update.viewer, id.clone())),
                InputState::Waiting { .. }
                | InputState::Finished { .. }
                | InputState::EngineFault { .. } => None,
            })
    }
}

fn other(seat: Seat) -> Seat {
    if seat == Seat::P1 { Seat::P2 } else { Seat::P1 }
}

#[test]
fn a_stale_id_from_the_other_seat_never_names_its_current_question() {
    let mut session = session(HOT_SEAT, 100);
    let mut seats = Seats::default();
    seats.absorb(&session.start());
    let (first, first_id) = seats.question().unwrap();
    // Answer until the other seat holds its own first question.
    let mut current = (first, first_id.clone());
    for request in 0..10 {
        let out = session.submit(current.0, answer(&current.1, &format!("r{request}")));
        seats.absorb(&out);
        current = seats.question().unwrap();
        if current.0 != first {
            break;
        }
    }
    let (holder, held) = current;
    assert_eq!(holder, other(first));
    let before = authority(&session);
    assert_eq!(
        session.submit(holder, answer(&first_id, "replayed")),
        rejected(holder, "replayed")
    );
    assert_eq!(
        session.submit(first, answer(&held, "foreign")),
        rejected(first, "foreign")
    );
    assert_eq!(
        session.submit(holder, answer("p1-q999", "unknown")),
        rejected(holder, "unknown")
    );
    assert_eq!(authority(&session), before);
    assert_ne!(held, first_id, "seat-scoped ids must differ");
    let out = session.submit(holder, answer(&held, "valid"));
    assert!(matches!(&out[0].message,
        Outgoing::Receipt(receipt) if receipt.result == ReceiptResult::Applied));
    let after = authority(&session);
    assert_eq!(
        session.submit(holder, answer(&held, "repeat")),
        rejected(holder, "repeat")
    );
    assert_eq!(authority(&session), after);
}

/// Every delivery and the final authority of a hot-seat run; `noise` adds rejected
/// commands and baseline queries before every valid command.
fn scripted(noise: bool) -> (Vec<Delivery>, String) {
    let mut session = session(HOT_SEAT, 1_000);
    let started = session.start();
    let mut transcript = started.clone();
    let mut seats = Seats::default();
    seats.absorb(&started);
    let mut previous = String::new();
    for request in 0..200 {
        let Some((seat, id)) = seats.question() else {
            break;
        };
        if noise {
            for (from, question) in [(other(seat), id.as_str()), (seat, previous.as_str())] {
                assert_eq!(
                    session.submit(from, answer(question, "noise")),
                    rejected(from, "noise")
                );
            }
            for seat in [Seat::P1, Seat::P2] {
                let _baseline = session.baseline(seat);
            }
        }
        let out = session.submit(seat, answer(&id, &format!("r{request}")));
        seats.absorb(&out);
        transcript.extend(out);
        previous = id;
    }
    assert!(
        seats.question().is_none(),
        "the synthetic match must finish"
    );
    (transcript, authority(&session))
}

#[test]
fn rejected_commands_and_queries_do_not_advance_rules_or_randomness() {
    let plain = scripted(false);
    assert_eq!(plain, scripted(true));
}

#[test]
fn baselines_resend_state_without_advancing_anything() {
    let mut session = session(HOT_SEAT, 100);
    let started = session.start();
    let before = authority(&session);
    for seat in [Seat::P1, Seat::P2, Seat::P1] {
        let expected: Vec<_> = started
            .iter()
            .filter(|delivery| delivery.seat == seat)
            .collect();
        let again = session.baseline(seat);
        assert_eq!(again.iter().collect::<Vec<_>>(), expected);
    }
    assert_eq!(authority(&session), before);
}

#[test]
fn concede_while_waiting_finishes_for_both_seats_and_cannot_repeat() {
    let mut session = session(HOT_SEAT, 100);
    let mut seats = Seats::default();
    seats.absorb(&session.start());
    let (deciding, _id) = seats.question().unwrap();
    let waiting = other(deciding);
    let out = session.submit(waiting, concede("give-up".into()));
    assert!(matches!(&out[0].message,
        Outgoing::Receipt(receipt) if receipt.result == ReceiptResult::Applied));
    seats.absorb(&out);
    for update in seats.0.iter().flatten() {
        assert_eq!(
            update.input,
            InputState::Finished {
                winner: Some(deciding)
            }
        );
    }
    assert_eq!(
        session.submit(waiting, concede("again".into())),
        rejected(waiting, "again")
    );
}

#[test]
fn a_policy_seat_has_no_channel() {
    let mut session = session([SeatControl::Player, SeatControl::Policy], 100);
    let started = session.start();
    assert!(!started.is_empty());
    assert!(started.iter().all(|delivery| delivery.seat == Seat::P1));
    assert_eq!(
        session.submit(Seat::P2, answer("p2-q1", "no-channel")),
        rejected(Seat::P2, "no-channel")
    );
}

#[test]
fn an_exhausted_budget_stops_interaction() {
    let mut policies = session([SeatControl::Policy, SeatControl::Policy], 2);
    assert!(policies.start().is_empty());
    assert_eq!(policies.stopped(), Some(HostErrorReason::Budget));

    let mut session = session([SeatControl::Player, SeatControl::Policy], 1);
    let mut seats = Seats::default();
    seats.absorb(&session.start());
    let (seat, id) = seats.question().unwrap();
    let out = session.submit(seat, answer(&id, "within"));
    assert!(matches!(&out[0].message,
        Outgoing::Receipt(receipt) if receipt.result == ReceiptResult::Applied));
    seats.absorb(&out);
    let (seat, id) = seats.question().unwrap();
    let before = authority(&session);
    assert_eq!(
        session.submit(seat, answer(&id, "over")),
        vec![stopped(Seat::P1, HostErrorReason::Budget)]
    );
    assert_eq!(authority(&session), before);
}

#[test]
fn malformed_messages_stop_interaction_and_nothing_restarts_it() {
    let mut session = session(HOT_SEAT, 100);
    let _started = session.start();
    let both = vec![
        stopped(Seat::P1, HostErrorReason::Format),
        stopped(Seat::P2, HostErrorReason::Format),
    ];
    let out = session.submit_json(Seat::P1, r#"{"request_id":"x","command":{"kind":"guess"}}"#);
    assert_eq!(out, both);
    let before = authority(&session);
    let valid = r#"{"request_id":"y","command":{"kind":"concede"}}"#;
    assert_eq!(
        session.submit_json(Seat::P2, valid),
        vec![stopped(Seat::P2, HostErrorReason::Format)]
    );
    assert_eq!(
        session.submit_json(Seat::P2, "{"),
        vec![stopped(Seat::P2, HostErrorReason::Format)]
    );
    assert_eq!(
        session.baseline(Seat::P1),
        vec![stopped(Seat::P1, HostErrorReason::Format)]
    );
    assert_eq!(session.start(), both);
    assert_eq!(authority(&session), before);
}

#[test]
fn a_host_stopped_before_start_never_lets_the_policy_answer() {
    // P1 chooses first and is a policy seat, so a start that ignored the stop would submit.
    let mut session = session([SeatControl::Policy, SeatControl::Player], 100);
    let before = authority(&session);
    assert_eq!(
        session.submit_json(Seat::P2, "not json"),
        vec![stopped(Seat::P2, HostErrorReason::Format)]
    );
    assert_eq!(
        session.start(),
        vec![stopped(Seat::P2, HostErrorReason::Format)]
    );
    assert_eq!(authority(&session), before);
}

#[test]
fn wire_messages_use_the_temporary_format_and_snake_case_tags() {
    let mut session = session(HOT_SEAT, 100);
    let started = session.start();
    let json = serde_json::to_value(&started[0].message).unwrap();
    assert_eq!(
        json,
        json!({"kind":"update","format":0,"viewer":"p1","update_seq":1,"baseline":true,
            "input":{"kind":"question","id":"p1-q1","purpose":"rule_choice","shape":"opening"}})
    );
    let receipt = serde_json::to_value(&rejected(Seat::P1, "r")[0].message).unwrap();
    assert_eq!(
        receipt,
        json!({"kind":"receipt","request_id":"r","result":"rejected",
            "rejection":"invalid_submission"})
    );
}
