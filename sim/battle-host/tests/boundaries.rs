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
use sve_battle_contract::question::{Question, QuestionShape, QuestionUse};
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
/// With `start_amulets`, each seat first chooses a start amulet, so questions precede the
/// opening shuffle.
fn game(start_amulets: bool) -> (Game, BattleReferences) {
    let amulet = json!({"number":"t-start","faces":[{"name":"t-start","card_class":"ニュートラル",
        "card_type":"アミュレット","cost":"1","power":null,"hp":null,"traits":[],"text":null,
        "sections":[],"title":"synthetic"}]})
    .to_string();
    let snapshot = [card("t-small", "1", "1"), card("t-large", "3", "3"), amulet].join("\n");
    let registry = json!({"format":1_u8,"kind":"keyword_registry","keywords":{
        "start_amulet":{"ja":"スタートアミュレット","rule":"14.4.3",
            "expansion":{"op":"keyword","name":"start_amulet"}}
    }})
    .to_string();
    let start = if start_amulets {
        json!([{"kind":"static","line":1_i64,"body":{"op":"keyword","name":"start_amulet"}}])
    } else {
        json!([])
    };
    let effects = json!({"format":1_u8,"kind":"effect_set","cards":{
        "t-small":{"status":"complete","review":"synthetic","abilities":[]},
        "t-large":{"status":"complete","review":"synthetic","abilities":[]},
        "t-start":{"status":"complete","review":"synthetic","abilities":start}
    }})
    .to_string();
    let mut rules: Value = serde_json::from_str(&fixture::rules(&snapshot, "t-small")).unwrap();
    if let Some(title) = rules.pointer_mut("/titles/0") {
        title["title_code"] = json!("synthetic_start");
        title["capabilities"] = json!([{"kind":"opening_start_amulet"}]);
    }
    let catalog = Catalog::from_documents_with_rules(
        &snapshot,
        &registry,
        &[("t.yaml".into(), effects)],
        &rules.to_string(),
        &EngineIdentityInput::LegacyJp,
    )
    .unwrap();
    let references = BattleReferences::new(&catalog, fixture::references(&snapshot)).unwrap();
    let deck = |seat: &str| {
        (0..10_u32)
            .map(|index| {
                let number = match index {
                    0 if start_amulets => "t-start",
                    _ if index % 2 == 0 => "t-small",
                    _ => "t-large",
                };
                json!({"id":format!("{seat}/deck/{index}"),"card":number})
            })
            .collect::<Vec<_>>()
    };
    let player = |seat: &str| {
        if start_amulets {
            json!({"leader":{"class":"ニュートラル"},"construction":"title",
                "title_code":"synthetic_start","deck_list":deck(seat),"evolve_deck_list":[]})
        } else {
            json!({"leader":{"class":"ニュートラル"},"construction":"class",
                "deck_list":deck(seat),"evolve_deck_list":[]})
        }
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
    opened(false, control, max_submissions)
}

fn opened(start_amulets: bool, control: [SeatControl; 2], max_submissions: u64) -> LocalSession {
    let (game, references) = game(start_amulets);
    LocalSession::new(
        game,
        references,
        fixture::identity("", ""),
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

/// Every delivery and the final authority of a hot-seat run; `noise` adds rejected commands
/// (the other seat, an option the question never offered, a retired question) and baseline
/// queries before every valid command.
///
/// With `start_amulets` the run covers the opening only: the start amulet choices come before
/// the opening shuffle, and the mulligan questions after it show each shuffled hand, so
/// randomness a rejected command touched would change them. Otherwise the match runs to its end.
fn scripted(start_amulets: bool, noise: bool) -> (Vec<Delivery>, String) {
    let mut session = opened(start_amulets, HOT_SEAT, 1_000);
    let started = session.start();
    let mut transcript = started.clone();
    let mut seats = Seats::default();
    seats.absorb(&started);
    let mut previous: Option<Question> = None;
    for request in 0..400_u32 {
        let Some((seat, question)) = seats.question() else {
            break;
        };
        if start_amulets && question.usage == QuestionUse::MainAction {
            return (transcript, authority(&session));
        }
        if noise {
            let mut rejected_commands = vec![
                (other(seat), answer(&question, "noise")),
                (
                    seat,
                    answer_with(question.id.as_str(), fixture::wrong(&question), "noise"),
                ),
            ];
            rejected_commands.extend(previous.map(|retired| (seat, answer(&retired, "noise"))));
            for (from, command) in rejected_commands {
                assert_eq!(
                    session.submit(from, &command),
                    rejected(from, "noise"),
                    "noise is rejected before reaching the engine"
                );
            }
            for viewer in [Seat::P1, Seat::P2] {
                let _baseline = session.baseline(viewer);
            }
        }
        let out = session.submit(seat, &answer(&question, &format!("r{request}")));
        assert!(applied(&out), "{:?}", question.usage);
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
    let opening = scripted(true, false);
    assert!(
        opening.0.iter().any(|delivery| matches!(&delivery.message,
            HostMessage::Update(update) if matches!(&update.input,
                BattleInput::Question { question } if question.usage == QuestionUse::StartAmulet))),
        "the opening asks for start amulets before the shuffle"
    );
    assert_eq!(opening, scripted(true, true));
    assert_eq!(scripted(false, false), scripted(false, true));
}

#[test]
fn baselines_resend_state_without_advancing_anything() {
    let mut session = session(HOT_SEAT, 100);
    let started = session.start();
    let before = authority(&session);
    for seat in [Seat::P1, Seat::P2, Seat::P1] {
        let expected: Vec<_> = started
            .iter()
            .filter(|delivery| {
                delivery.seat == seat && matches!(delivery.message, HostMessage::Update(_))
            })
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

/// What the Worker posts: each message as JSON, checked by the strict contract decoder.
fn wire(deliveries: &[Delivery]) -> Vec<(Seat, Value)> {
    deliveries
        .iter()
        .map(|delivery| {
            let json = serde_json::to_string(&delivery.message).unwrap();
            let decoded: HostMessage = sve_battle_contract::decode(&json).unwrap();
            assert_eq!(
                decoded, delivery.message,
                "the contract decodes what the host sends"
            );
            (delivery.seat, serde_json::from_str(&json).unwrap())
        })
        .collect()
}

fn submitted(question: &Question, answer: Answer, request: &str) -> String {
    serde_json::to_string(&ClientMessage::Submit(answer_with(
        question.id.as_str(),
        answer,
        request,
    )))
    .unwrap()
}

#[test]
fn the_worker_entry_points_send_contract_messages() {
    let mut session = session(HOT_SEAT, 100);
    let started = session.start();
    let opening = wire(&started);
    let ready: Vec<_> = opening
        .iter()
        .filter(|(_, json)| json["kind"] == "ready")
        .map(|(seat, json)| (*seat, json["viewer"].clone()))
        .collect();
    assert_eq!(
        ready,
        [(Seat::P1, json!("p1")), (Seat::P2, json!("p2"))],
        "each player seat is told the game's identity once, before any update"
    );
    assert!(
        opening
            .iter()
            .position(|(_, json)| json["kind"] == "update")
            .is_some_and(|first| first >= ready.len())
    );
    let (_, baseline) = opening
        .iter()
        .find(|(seat, json)| *seat == Seat::P1 && json["kind"] == "update")
        .unwrap();
    assert_eq!(
        (
            &baseline["update_seq"],
            &baseline["input"]["question"]["id"]
        ),
        (&json!(1_u8), &json!("p1-q1"))
    );
    assert_eq!(
        baseline["events"],
        json!({"status":"available","batches":[],"animate":[]}),
        "a baseline replays no history"
    );
    assert!(
        session
            .start()
            .iter()
            .all(|delivery| !matches!(delivery.message, HostMessage::Ready(_))),
        "Ready is delivered once per game"
    );

    let mut seats = Seats::default();
    seats.absorb(&started);
    let (seat, question) = seats.question().unwrap();
    let name = if seat == Seat::P1 { "p1" } else { "p2" };
    let refused =
        wire(&session.submit_named(name, &submitted(&question, fixture::wrong(&question), "r0")));
    assert_eq!(
        refused,
        [(
            seat,
            json!({"kind":"receipt","request_id":"r0","result":"rejected",
            "rejection":"invalid_submission"})
        )]
    );
    let accepted = wire(&session.submit_named(
        name,
        &submitted(&question, fixture::passive(&question), "r1"),
    ));
    assert_eq!(
        accepted.first(),
        Some(&(
            seat,
            json!({"kind":"receipt","request_id":"r1","result":"applied",
            "rejection":null})
        ))
    );
    assert!(
        accepted
            .iter()
            .skip(1)
            .all(|(_, json)| json["kind"] == "update"),
        "the accepted answer advances the match"
    );
    assert_eq!(
        wire(&session.submit_named(name, "{")),
        [Seat::P1, Seat::P2].map(|stopped| (
            stopped,
            json!({"kind":"host_error","format":1_u8,"reason":"format"})
        ))
    );
}

#[test]
fn a_waiting_seat_is_never_sent_the_other_seats_question() {
    let mut session = session(HOT_SEAT, 100);
    let mut deliveries = session.start();
    let mut seats = Seats::default();
    seats.absorb(&deliveries);
    for request in 0..20_u32 {
        let (seat, question) = seats.question().unwrap();
        let options = serde_json::to_value(&question.shape).unwrap();
        let waiting: Vec<_> = wire(&deliveries)
            .into_iter()
            .filter(|(to, _)| *to == other(seat))
            .map(|(_, json)| json.to_string())
            .collect();
        for message in &waiting {
            assert!(
                !message.contains(&format!("\"{}\"", question.id.as_str()))
                    && !message.contains(&options.to_string()),
                "{message}"
            );
        }
        deliveries = session.submit(seat, &answer(&question, &format!("r{request}")));
        seats.absorb(&deliveries);
    }
}
