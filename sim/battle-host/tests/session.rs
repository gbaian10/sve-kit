//! Whole local matches on the public engine inputs; boundaries live in `boundaries.rs`.

#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "Missing generated inputs or an unexpected message invalidate these fixtures."
)]

#[path = "support/fixture.rs"]
mod fixture;

use core::array::from_fn;
use serde_json::{Value, json};
use std::{env::var_os, fs::read_to_string, path::PathBuf, sync::OnceLock};
use sve_battle_contract::question::{MainIntent, Question, QuestionShape, ShapeKind};
use sve_battle_contract::{Answer, ClientMessage, Command};
use sve_battle_host::wire::{
    BattleInput, BattleUpdate, HostMessage, ReceiptResult, RequestId, Seat, Submit,
};
use sve_battle_host::{Delivery, LocalSession, SeatControl};
use sve_engine::battle::BattleReferences;
use sve_engine::catalog::{CurrentSelectionLoader, EngineIdentityInput, SelectedEffects};
use sve_engine::game::Game;
use sve_engine::game::player::{DeckConstruction, PlayerDeck, Viewer};
use sve_engine::host::HostLimits;

/// The generated author-test inputs and their bound rules.
fn texts() -> &'static [String; 4] {
    static TEXTS: OnceLock<[String; 4]> = OnceLock::new();
    TEXTS.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let snapshot = read_to_string(var_os("SVE_TEST_SNAPSHOT").unwrap()).unwrap();
        let selection = read_to_string(var_os("SVE_TEST_CURRENT_SELECTION").unwrap()).unwrap();
        let keywords = read_to_string(root.join("authored/rules/keywords.yaml")).unwrap();
        let mut rules: Value = serde_saphyr::from_str(
            &read_to_string(root.join("authored/rules/engine/index.yaml")).unwrap(),
        )
        .unwrap();
        rules
            .pointer_mut("/input")
            .and_then(Value::as_object_mut)
            .unwrap()
            .insert(
                "snapshot_sha256".into(),
                Value::String(fixture::hex(snapshot.as_bytes())),
            );
        [snapshot, keywords, selection, rules.to_string()]
    })
}

fn selection() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let [snapshot, keywords, selection, rules] = texts();
        CurrentSelectionLoader::new()
            .unwrap()
            .select_with_rules(
                snapshot,
                keywords,
                &serde_json::from_str(selection).unwrap(),
                rules,
                &EngineIdentityInput::LegacyJp,
            )
            .unwrap()
    })
}

const MAIN_DECK: [&str; 2] = ["BP10-122", "BP01-173"];

fn session(control: [SeatControl; 2]) -> LocalSession {
    let decks = from_fn(|_| PlayerDeck {
        construction: DeckConstruction::Class("ニュートラル".into()),
        main: [MAIN_DECK; 20]
            .concat()
            .into_iter()
            .map(Into::into)
            .collect(),
        evolve: Vec::new(),
    });
    let references =
        BattleReferences::new(&selection().prototypes, fixture::references(&texts()[0])).unwrap();
    let game =
        Game::start_player_game(selection().clone(), decks, Viewer::P2, "battle-host").unwrap();
    LocalSession::new(
        game,
        references,
        fixture::identity("", ""),
        control,
        HostLimits::default(),
    )
}

/// The deliveries as the Worker posts them: every message must survive the strict contract
/// decoder unchanged.
fn absorb(deliveries: &[Delivery], latest: &mut [Option<BattleUpdate>; 2]) {
    for delivery in deliveries {
        let json = serde_json::to_string(&delivery.message).unwrap();
        let decoded: HostMessage = sve_battle_contract::decode(&json).unwrap();
        assert_eq!(
            decoded, delivery.message,
            "the contract decodes what the host sends"
        );
        if let HostMessage::Update(update) = &delivery.message {
            let [first, second] = latest;
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
}

/// Answer every question passively until the match ends; returns each seat's last update.
fn play_out(session: &mut LocalSession) -> [Option<BattleUpdate>; 2] {
    let mut latest: [Option<BattleUpdate>; 2] = [None, None];
    absorb(&session.start(), &mut latest);
    for request in 0..5_000_u32 {
        let open = latest
            .iter()
            .flatten()
            .find_map(|update| match &update.input {
                BattleInput::Question { question } => Some((update.viewer, question.clone())),
                BattleInput::Waiting { .. }
                | BattleInput::Finished { .. }
                | BattleInput::EngineFault { .. } => None,
            });
        let Some((seat, question)) = open else {
            return latest;
        };
        let request = format!("r{request}");
        let message = ClientMessage::Submit(Submit {
            request_id: RequestId::new(request.clone()),
            command: Command::Answer {
                answer: fixture::passive(&question),
                question_id: question.id,
            },
        });
        // The Worker's entry point: a seat name and the client message as JSON.
        let name = if seat == Seat::P1 { "p1" } else { "p2" };
        let out = session.submit_named(name, &serde_json::to_string(&message).unwrap());
        let first = out.first().unwrap();
        assert_eq!(first.seat, seat, "the receipt goes to the submitting seat");
        assert!(
            matches!(&first.message,
            HostMessage::Receipt(receipt) if receipt.request_id().as_str() == request
                && receipt.result() == ReceiptResult::Applied),
            "the receipt comes first: {first:?}"
        );
        assert!(
            out.iter()
                .skip(1)
                .all(|delivery| matches!(delivery.message, HostMessage::Update(_))),
            "only updates follow the receipt"
        );
        absorb(&out, &mut latest);
    }
    panic!("match did not finish");
}

#[test]
fn hot_seat_match_runs_to_a_result_with_receipts_first() {
    let mut session = session([SeatControl::Player, SeatControl::Player]);
    let latest = play_out(&mut session);
    assert!(latest.iter().all(|update| matches!(
        update.as_ref().map(|update| &update.input),
        Some(BattleInput::Finished {
            winner: Some(_),
            ..
        })
    )));
    assert_eq!(session.stopped(), None);
}

#[test]
fn policy_seat_plays_inside_the_host_against_a_player_channel() {
    let mut session = session([SeatControl::Player, SeatControl::Policy]);
    let [first, second] = play_out(&mut session);
    assert!(second.is_none(), "the policy seat has no channel");
    assert!(matches!(
        first.map(|update| update.input),
        Some(BattleInput::Finished {
            winner: Some(_),
            ..
        })
    ));
}

/// P1 holds BP07-102 (look at the top three cards, may reveal a Bishop follower into the hand)
/// on a mid-game board whose P1 deck is ten copies of `deck`.
fn private_look(deck: &str) -> LocalSession {
    let cards = |seat: &str, card: &str| {
        (0..10_u8)
            .map(|n| json!({"id":format!("{seat}-{n}"),"card":card}))
            .collect::<Vec<_>>()
    };
    let player = |hand: &str, pile: Vec<Value>| {
        json!({"leader":{"class":"ビショップ","life":20_u8},"pp":{"current":2_u8,"max":2_u8},
            "ep":0_u8,"sep":0_u8,"zones":{"hand":[{"id":hand,"card":"BP07-102"}],"deck":pile}})
    };
    let position = json!({
        "turn":{"active":"P1","first_player":"P1","elapsed_turns":{"P1":3_u8,"P2":2_u8},
            "phase":"main"},
        "players":{"P1":player("look", cards("p1", deck)),"P2":player("idle", cards("p2", "BP10-122"))}
    });
    let game = Game::from_selected_effects(
        selection().clone(),
        &position,
        &Value::Null,
        &Value::Null,
        "private-look",
    )
    .unwrap();
    let references =
        BattleReferences::new(&selection().prototypes, fixture::references(&texts()[0])).unwrap();
    LocalSession::new(
        game,
        references,
        fixture::identity("", ""),
        [SeatControl::Player, SeatControl::Player],
        HostLimits::default(),
    )
}

/// The intent playing P1's BP07-102.
fn play_look(update: &BattleUpdate, intents: &[MainIntent]) -> Answer {
    let intent = intents.iter().find_map(|intent| match intent {
        MainIntent::Play { id, source, .. }
            if update
                .observation
                .objects
                .get(source)
                .is_some_and(|object| object.card.printing_id == "test-BP07-102") =>
        {
            Some(id.clone())
        }
        MainIntent::Play { .. }
        | MainIntent::Activate { .. }
        | MainIntent::Evolve { .. }
        | MainIntent::Attack { .. }
        | MainIntent::EndPhase { .. } => None,
    });
    Answer::ActionWindow {
        intent: intent.unwrap(),
        target: None,
    }
}

/// P1 plays the looking amulet and gives the smallest answer to everything it is asked until its
/// main window returns. Returns every message P2 received and each stop P1 was asked at, with the
/// number of candidates of a pick.
fn look_and_decline(deck: &str) -> (Vec<String>, Vec<(ShapeKind, Option<usize>)>) {
    let mut session = private_look(deck);
    let mut waiting = Vec::new();
    let mut stops = Vec::new();
    let mut latest = None;
    let mut deliver = |out: Vec<Delivery>, slot: &mut Option<BattleUpdate>| {
        for delivery in out {
            if delivery.seat == Seat::P2 {
                waiting.push(serde_json::to_string(&delivery.message).unwrap());
                continue;
            }
            if let HostMessage::Update(update) = delivery.message {
                *slot = Some(*update);
            }
        }
    };
    deliver(session.start(), &mut latest);
    let mut played = false;
    for request in 0..10_u8 {
        let update = latest.clone().unwrap();
        let BattleInput::Question { question } = &update.input else {
            panic!("P1 always answers here: {:?}", update.input);
        };
        let answer = if let QuestionShape::ActionWindow { intents } = &question.shape {
            if played {
                return (waiting, stops);
            }
            played = true;
            play_look(&update, intents)
        } else {
            let candidates = if let QuestionShape::Select { selection } = &question.shape {
                Some(selection.candidates.len())
            } else {
                None
            };
            stops.push((question.shape.kind(), candidates));
            fixture::passive(question)
        };
        let message = ClientMessage::Submit(Submit {
            request_id: RequestId::new(format!("r{request}")),
            command: Command::Answer {
                answer,
                question_id: question.id.clone(),
            },
        });
        deliver(
            session.submit_named("p1", &serde_json::to_string(&message).unwrap()),
            &mut latest,
        );
    }
    panic!("the main window did not return");
}

#[test]
fn a_private_look_with_or_without_a_pick_sends_the_waiting_seat_the_same_messages() {
    // Bishop followers on top give P1 a pick it declines; neutral cards leave nothing to pick, and
    // the engine still asks the same empty pick rather than skipping a stop P2 could notice.
    let (with_pick, offered) = look_and_decline("BP22-102");
    let (without_pick, empty) = look_and_decline("BP10-122");
    assert_eq!(
        offered,
        [
            (ShapeKind::Choose, None),
            (ShapeKind::Select, Some(3)),
            (ShapeKind::Order, None)
        ]
    );
    assert_eq!(
        empty,
        [
            (ShapeKind::Choose, None),
            (ShapeKind::Select, Some(0)),
            (ShapeKind::Order, None)
        ]
    );
    assert_eq!(
        with_pick, without_pick,
        "P2's updates, update_seq and animate never depend on P1's private candidates"
    );
}

/// P1 on its seventh turn with BP09-004 on the field and BP09-005 in its evolution deck.
fn evolution() -> LocalSession {
    let player = |seat: &str, field: Value, evolve: Value| {
        json!({"leader":{"class":"エルフ","life":20_u8},"pp":{"current":7_u8,"max":7_u8},
            "ep":0_u8,"sep":1_u8,"zones":{"field":field,"evolve_deck":evolve,
            "deck":[{"id":format!("{seat}-deck"),"card":"BP10-122"}]}})
    };
    let position = json!({
        "turn":{"active":"P1","first_player":"P1","elapsed_turns":{"P1":7_u8,"P2":6_u8},
            "phase":"main"},
        "players":{
            "P1":player("p1", json!([{"id":"source","card":"BP09-004"}]),
                json!([{"id":"form","card":"BP09-005"}])),
            "P2":player("p2", json!([]), json!([]))
        }
    });
    let game = Game::from_selected_effects(
        selection().clone(),
        &position,
        &Value::Null,
        &Value::Null,
        "withdraw",
    )
    .unwrap();
    let references =
        BattleReferences::new(&selection().prototypes, fixture::references(&texts()[0])).unwrap();
    LocalSession::new(
        game,
        references,
        fixture::identity("", ""),
        [SeatControl::Player, SeatControl::Player],
        HostLimits::default(),
    )
}

fn p1_submit(session: &mut LocalSession, request: &str, command: Command) -> Vec<Delivery> {
    let message = ClientMessage::Submit(Submit {
        request_id: RequestId::new(request.into()),
        command,
    });
    session.submit_named("p1", &serde_json::to_string(&message).unwrap())
}

fn p1_question(out: &[Delivery]) -> Question {
    out.iter()
        .filter(|delivery| delivery.seat == Seat::P1)
        .find_map(|delivery| match &delivery.message {
            HostMessage::Update(update) => match &update.input {
                BattleInput::Question { question } => Some((**question).clone()),
                BattleInput::Waiting { .. }
                | BattleInput::Finished { .. }
                | BattleInput::EngineFault { .. } => None,
            },
            HostMessage::Ready(_) | HostMessage::Receipt(_) | HostMessage::HostError(_) => None,
        })
        .unwrap()
}

#[test]
fn a_withdrawn_evolution_reverts_on_the_wire_and_sends_the_waiting_seat_nothing() {
    let mut session = evolution();
    let window = p1_question(&session.start());
    assert!(!window.can_withdraw);
    let QuestionShape::ActionWindow { intents } = &window.shape else {
        panic!("main window: {window:?}");
    };
    let evolve = intents
        .iter()
        .find_map(|intent| match intent {
            MainIntent::Evolve { id, .. } => Some(id.clone()),
            MainIntent::Play { .. }
            | MainIntent::Activate { .. }
            | MainIntent::Attack { .. }
            | MainIntent::EndPhase { .. } => None,
        })
        .unwrap();
    let declared = p1_submit(
        &mut session,
        "declare",
        Command::Answer {
            question_id: window.id.clone(),
            answer: Answer::ActionWindow {
                intent: evolve,
                target: None,
            },
        },
    );
    let asked = p1_question(&declared);
    assert!(asked.can_withdraw, "an evolution stage can be withdrawn");
    let withdrawn = p1_submit(
        &mut session,
        "withdraw",
        Command::Withdraw {
            question_id: asked.id.clone(),
        },
    );
    assert!(matches!(
        &withdrawn[0].message,
        HostMessage::Receipt(receipt) if receipt.result() == ReceiptResult::Reverted
    ));
    let again = p1_question(&withdrawn);
    assert!(matches!(again.shape, QuestionShape::ActionWindow { .. }));
    assert_ne!(again.id, window.id, "the restored window is a new question");
    assert!(
        declared
            .iter()
            .chain(&withdrawn)
            .all(|delivery| delivery.seat == Seat::P1),
        "the waiting seat receives nothing from a declaration and its withdrawal"
    );
    let stale = p1_submit(
        &mut session,
        "stale",
        Command::Withdraw {
            question_id: asked.id,
        },
    );
    assert!(matches!(
        &*stale,
        [Delivery { message: HostMessage::Receipt(receipt), .. }]
            if receipt.result() == ReceiptResult::Rejected
    ));
}
