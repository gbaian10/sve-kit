//! Channel boundaries on the smallest synthetic match: two vanilla cards, no effects.

#![expect(
    clippy::unwrap_used,
    reason = "A malformed synthetic fixture or an unexpected message invalidates these tests."
)]

#[path = "support/fixture.rs"]
mod fixture;

use serde_json::{Value, json};
extern crate alloc;

use alloc::sync::Arc;
use sve_battle_contract::question::{Question, QuestionShape};
use sve_battle_contract::{Answer, ClientMessage, Command, OptionId};
use sve_battle_host::wire::{
    BattleInput, BattleUpdate, HostError, HostErrorReason, HostMessage, QuestionId, Receipt,
    ReceiptResult, RequestId, Seat, Submit,
};
use sve_battle_host::{Delivery, LocalSession, SeatControl};
use sve_engine::battle::BattleReferences;
use sve_engine::catalog::{Catalog, EngineIdentityInput};
use sve_engine::game::Game;
use sve_engine::host::HostLimits;

fn card(number: &str, power: &str, hp: &str) -> String {
    json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル",
        "card_type":"フォロワー","cost":"1","power":power,"hp":hp,"traits":[],"text":null,
        "sections":[],"title":"synthetic"}]})
    .to_string()
}

/// A pregame opening; two different cards make a drifted shuffle visible later.
fn game() -> (Game, BattleReferences) {
    let snapshot = [card("t-small", "1", "1"), card("t-large", "3", "3")].join("\n");
    let registry = json!({"format":1_u8,"kind":"keyword_registry","keywords":{}}).to_string();
    let effects = json!({"format":1_u8,"kind":"effect_set","cards":{
        "t-small":{"status":"complete","review":"synthetic","abilities":[]},
        "t-large":{"status":"complete","review":"synthetic","abilities":[]}
    }})
    .to_string();
    let catalog = Catalog::from_documents_with_rules(
        &snapshot,
        &registry,
        &[("t.yaml".into(), effects)],
        &fixture::rules(&snapshot, "t-small"),
        &EngineIdentityInput::LegacyJp,
    )
    .unwrap();
    let references = BattleReferences::new(&catalog, fixture::references(&snapshot)).unwrap();
    let deck = |seat: &str| {
        (0..10_u32)
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
    (game, references)
}

fn session(control: [SeatControl; 2], max_submissions: u64) -> LocalSession {
    let (game, references) = game();
    LocalSession::new(
        game,
        references,
        control,
        HostLimits {
            max_submissions,
            max_retries: 1,
        },
    )
}

const HOT_SEAT: [SeatControl; 2] = [SeatControl::Player, SeatControl::Player];

fn answer(question: &Question, request: &str) -> Submit {
    answer_with(question.id.as_str(), fixture::passive(question), request)
}

fn answer_with(id: &str, answer: Answer, request: &str) -> Submit {
    Submit {
        request_id: RequestId::new(request.into()),
        command: Command::Answer {
            question_id: QuestionId::new(id.into()),
            answer,
        },
    }
}

const fn concede(request: String) -> Submit {
    Submit {
        request_id: RequestId::new(request),
        command: Command::Concede,
    }
}

/// Authority state as Debug prints it. The RNG prints no internal position, so unchanged
/// randomness is shown by `rejected_commands_and_queries_do_not_advance_rules_or_randomness`.
fn authority(session: &LocalSession) -> String {
    format!("{:?}", session.game())
}

fn rejected(seat: Seat, request: &str) -> Vec<Delivery> {
    vec![Delivery {
        seat,
        message: HostMessage::Receipt(Receipt::new(
            RequestId::new(request.into()),
            ReceiptResult::Rejected,
        )),
    }]
}

const fn stopped(seat: Seat, reason: HostErrorReason) -> Delivery {
    Delivery {
        seat,
        message: HostMessage::HostError(HostError::new(reason)),
    }
}

/// Latest update per seat, checking that each seat's sequence only grows.
#[derive(Default)]
struct Seats([Option<BattleUpdate>; 2]);

impl Seats {
    fn absorb(&mut self, deliveries: &[Delivery]) {
        for delivery in deliveries {
            let HostMessage::Update(update) = &delivery.message else {
                continue;
            };
            let [first, second] = &mut self.0;
            let slot = if delivery.seat == Seat::P1 {
                first
            } else {
                second
            };
            if let Some(previous) = slot.as_ref() {
                assert!(update.update_seq > previous.update_seq, "{update:?}");
            }
            *slot = Some((**update).clone());
        }
    }

    fn question(&self) -> Option<(Seat, Question)> {
        self.0
            .iter()
            .flatten()
            .find_map(|update| match &update.input {
                BattleInput::Question { question } => Some((update.viewer, (**question).clone())),
                BattleInput::Waiting { .. }
                | BattleInput::Finished { .. }
                | BattleInput::EngineFault { .. } => None,
            })
    }
}

fn other(seat: Seat) -> Seat {
    if seat == Seat::P1 { Seat::P2 } else { Seat::P1 }
}

fn applied(out: &[Delivery]) -> bool {
    matches!(out.first().map(|delivery| &delivery.message),
        Some(HostMessage::Receipt(receipt)) if receipt.result() == ReceiptResult::Applied)
}

#[test]
fn a_stale_id_from_the_other_seat_never_names_its_current_question() {
    let mut session = session(HOT_SEAT, 100);
    let mut seats = Seats::default();
    seats.absorb(&session.start());
    let (first, first_question) = seats.question().unwrap();
    // Answer until the other seat holds its own first question.
    let mut current = (first, first_question.clone());
    for request in 0..10_u32 {
        let out = session.submit(current.0, &answer(&current.1, &format!("r{request}")));
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
        session.submit(holder, &answer(&first_question, "replayed")),
        rejected(holder, "replayed")
    );
    assert_eq!(
        session.submit(first, &answer(&held, "foreign")),
        rejected(first, "foreign")
    );
    assert_eq!(
        session.submit(
            holder,
            &answer_with("p1-q999", fixture::passive(&held), "unknown")
        ),
        rejected(holder, "unknown")
    );
    assert_eq!(authority(&session), before);
    assert_ne!(held.id, first_question.id, "seat-scoped ids must differ");
    assert!(applied(&session.submit(holder, &answer(&held, "valid"))));
    let after = authority(&session);
    assert_eq!(
        session.submit(holder, &answer(&held, "repeat")),
        rejected(holder, "repeat")
    );
    assert_eq!(authority(&session), after);
}

#[test]
fn answers_outside_the_current_question_never_reach_the_engine() {
    let mut session = session(HOT_SEAT, 100);
    let mut seats = Seats::default();
    seats.absorb(&session.start());
    let (seat, question) = seats.question().unwrap();
    let QuestionShape::Choose { options, .. } = &question.shape else {
        panic!("the opening asks who goes first")
    };
    let id = question.id.as_str();
    let every: Vec<OptionId> = options.iter().map(|option| option.id.clone()).collect();
    let before = authority(&session);
    for (label, wrong) in [
        ("shape", Answer::Confirm { accept: true }),
        (
            "unknown option",
            Answer::Choose {
                chosen: vec![OptionId::new("not-offered".into())],
            },
        ),
        ("over bounds", Answer::Choose { chosen: every }),
        ("under bounds", Answer::Choose { chosen: vec![] }),
    ] {
        assert_eq!(
            session.submit(seat, &answer_with(id, wrong, label)),
            rejected(seat, label),
            "{label}"
        );
    }
    assert_eq!(authority(&session), before);
    assert!(applied(&session.submit(seat, &answer(&question, "valid"))));
}

/// Every delivery and the final authority of a hot-seat run; `noise` adds rejected
/// commands and baseline queries before every valid command.
fn scripted(noise: bool) -> (Vec<Delivery>, String) {
    let mut session = session(HOT_SEAT, 1_000);
    let started = session.start();
    let mut transcript = started.clone();
    let mut seats = Seats::default();
    seats.absorb(&started);
    let mut previous: Option<Question> = None;
    for request in 0..400_u32 {
        let Some((seat, question)) = seats.question() else {
            break;
        };
        if noise {
            let mut stale = vec![(other(seat), question.clone())];
            stale.extend(previous.map(|retired| (seat, retired)));
            for (from, held) in stale {
                assert_eq!(
                    session.submit(from, &answer(&held, "noise")),
                    rejected(from, "noise"),
                    "noise is rejected before reaching the engine"
                );
            }
            for viewer in [Seat::P1, Seat::P2] {
                let _baseline = session.baseline(viewer);
            }
        }
        let out = session.submit(seat, &answer(&question, &format!("r{request}")));
        seats.absorb(&out);
        transcript.extend(out);
        previous = Some(question);
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
    let (deciding, _question) = seats.question().unwrap();
    let waiting = other(deciding);
    let out = session.submit(waiting, &concede("give-up".into()));
    assert!(applied(&out));
    seats.absorb(&out);
    for update in seats.0.iter().flatten() {
        assert!(
            matches!(update.input, BattleInput::Finished { winner, .. } if winner == Some(deciding)),
            "{:?}",
            update.input
        );
    }
    assert_eq!(
        session.submit(waiting, &concede("again".into())),
        rejected(waiting, "again")
    );
}

#[test]
fn a_policy_seat_has_no_channel() {
    let mut session = session([SeatControl::Player, SeatControl::Policy], 100);
    let started = session.start();
    assert!(
        matches!(started.as_slice(), [first, ..] if first.seat == Seat::P1),
        "the player seat gets a baseline"
    );
    assert!(started.iter().all(|delivery| delivery.seat == Seat::P1));
    assert_eq!(
        session.submit(
            Seat::P2,
            &answer_with("p2-q1", Answer::Quick { option: None }, "no-channel")
        ),
        rejected(Seat::P2, "no-channel")
    );
    assert_eq!(
        session.submit(Seat::P2, &concede("no-channel".into())),
        rejected(Seat::P2, "no-channel")
    );
}

#[test]
fn an_exhausted_policy_budget_stops_interaction() {
    let mut policies = session([SeatControl::Policy, SeatControl::Policy], 2);
    assert_eq!(
        policies.start(),
        Vec::<Delivery>::new(),
        "no player channel"
    );
    assert_eq!(policies.stopped(), Some(HostErrorReason::Budget));

    // Player answers do not count; the policy seat's second answer exceeds the budget.
    let mut session = session([SeatControl::Player, SeatControl::Policy], 1);
    let mut seats = Seats::default();
    seats.absorb(&session.start());
    for request in 0..20_u32 {
        let (seat, question) = seats.question().unwrap();
        let out = session.submit(seat, &answer(&question, &format!("r{request}")));
        if session.stopped().is_some() {
            assert!(applied(&out), "the player's own answer was accepted");
            assert_eq!(
                out.last(),
                Some(&stopped(Seat::P1, HostErrorReason::Budget))
            );
            return;
        }
        seats.absorb(&out);
    }
    panic!("the policy budget was never exhausted");
}

#[test]
fn malformed_messages_stop_interaction_and_nothing_restarts_it() {
    let mut session = session(HOT_SEAT, 100);
    let _started = session.start();
    let both = vec![
        stopped(Seat::P1, HostErrorReason::Format),
        stopped(Seat::P2, HostErrorReason::Format),
    ];
    let out = session.submit_json(
        Seat::P1,
        r#"{"kind":"submit","request_id":"x","command":{"kind":"guess"}}"#,
    );
    assert_eq!(out, both);
    let before = authority(&session);
    let valid = r#"{"kind":"submit","request_id":"y","command":{"kind":"concede"}}"#;
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
fn an_unknown_seat_name_stops_interaction_like_any_malformed_message() {
    let both = vec![
        stopped(Seat::P1, HostErrorReason::Format),
        stopped(Seat::P2, HostErrorReason::Format),
    ];
    for unknown in ["submit", "baseline"] {
        let mut session = session(HOT_SEAT, 100);
        let mut seats = Seats::default();
        seats.absorb(&session.start());
        let (seat, question) = seats.question().unwrap();
        let name = if seat == Seat::P1 { "p1" } else { "p2" };
        let valid =
            serde_json::to_string(&ClientMessage::Submit(answer(&question, "valid"))).unwrap();
        let before = authority(&session);
        let out = if unknown == "submit" {
            session.submit_named("p3", &valid)
        } else {
            session.baseline_named("p3")
        };
        assert_eq!(out, both, "{unknown}");
        assert_eq!(
            session.submit_named(name, &valid),
            vec![stopped(seat, HostErrorReason::Format)]
        );
        assert_eq!(
            session.baseline_named(name),
            vec![stopped(seat, HostErrorReason::Format)]
        );
        assert_eq!(session.baseline_named("p3"), both);
        assert_eq!(authority(&session), before);
    }
}

#[test]
fn an_unsafe_integer_stops_with_range_before_reaching_the_engine() {
    let mut session = session(HOT_SEAT, 100);
    let _started = session.start();
    let before = authority(&session);
    let out = session.submit_json(
        Seat::P1,
        r#"{"kind":"submit","request_id":9007199254740993,"command":{"kind":"concede"}}"#,
    );
    assert_eq!(
        out,
        vec![
            stopped(Seat::P1, HostErrorReason::Range),
            stopped(Seat::P2, HostErrorReason::Range),
        ]
    );
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
fn every_message_is_a_contract_host_message() {
    let mut session = session(HOT_SEAT, 100);
    let started = session.start();
    let json = serde_json::to_value(&started[0].message).unwrap();
    assert_eq!(
        (
            &json["kind"],
            &json["format"],
            &json["viewer"],
            &json["update_seq"]
        ),
        (&json!("update"), &json!(1_u8), &json!("p1"), &json!(1_u8))
    );
    assert_eq!(json["input"]["question"]["id"], json!("p1-q1"));
    assert_eq!(
        json["events"],
        json!({"status":"available","batches":[],"animate":[]}),
        "a baseline replays no history"
    );
    let stop = serde_json::to_value(&stopped(Seat::P1, HostErrorReason::Range).message).unwrap();
    assert_eq!(
        stop,
        json!({"kind":"host_error","format":1_u8,"reason":"range"})
    );
    let receipt = serde_json::to_value(&rejected(Seat::P1, "r")[0].message).unwrap();
    assert_eq!(
        receipt,
        json!({"kind":"receipt","request_id":"r","result":"rejected",
            "rejection":"invalid_submission"})
    );
    // The strict contract decoder accepts exactly what the host sends.
    for message in [json, stop, receipt] {
        let decoded: HostMessage = sve_battle_contract::decode(&message.to_string()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), message);
    }
}
