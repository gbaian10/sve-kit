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
use sve_battle_contract::Command;
use sve_battle_host::load::{Inputs, load};
use sve_battle_host::wire::{
    BattleInput, BattleUpdate, HostMessage, ReceiptResult, RequestId, Seat, Submit,
};
use sve_battle_host::{Delivery, LocalSession, SeatControl};
use sve_engine::battle::BattleReferences;
use sve_engine::catalog::{CurrentSelectionLoader, EngineIdentityInput, SelectedEffects};
use sve_engine::game::Game;
use sve_engine::game::player::{DeckConstruction, PlayerDeck, Viewer};
use sve_engine::host::HostLimits;

/// Snapshot, keywords, selection, bound rules and references, exactly as a host passes them.
fn texts() -> &'static [String; 5] {
    static TEXTS: OnceLock<[String; 5]> = OnceLock::new();
    TEXTS.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let snapshot = read_to_string(var_os("SVE_TEST_SNAPSHOT").unwrap()).unwrap();
        let selection = read_to_string(var_os("SVE_TEST_CURRENT_SELECTION").unwrap()).unwrap();
        let keywords = read_to_string(root.join("authored/rules/keywords.yaml")).unwrap();
        let mut rules: Value = serde_saphyr::from_str(
            &read_to_string(root.join("authored/rules/engine/index.yaml")).unwrap(),
        )
        .unwrap();
        // The pin names the reviewed batch; generated test inputs keep the same rule capabilities.
        *rules.pointer_mut("/input/snapshot_sha256").unwrap() =
            Value::String(fixture::hex(snapshot.as_bytes()));
        let references = serde_json::to_string(&fixture::references(&snapshot)).unwrap();
        [snapshot, keywords, selection, rules.to_string(), references]
    })
}

fn inputs() -> Inputs<'static> {
    let [snapshot, keywords, selection, rules, references] = texts();
    Inputs {
        snapshot,
        keywords,
        selection,
        rules,
        references,
    }
}

fn selection() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let input = inputs();
        CurrentSelectionLoader::new()
            .unwrap()
            .select_with_rules(
                input.snapshot,
                input.keywords,
                &serde_json::from_str(input.selection).unwrap(),
                input.rules,
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
    let references = BattleReferences::new(
        &selection().prototypes,
        fixture::references(inputs().snapshot),
    )
    .unwrap();
    let game =
        Game::start_player_game(selection().clone(), decks, Viewer::P2, "battle-host").unwrap();
    LocalSession::new(game, references, control, HostLimits::default())
}

fn absorb(deliveries: &[Delivery], latest: &mut [Option<BattleUpdate>; 2]) {
    for delivery in deliveries {
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
        let answer = fixture::passive(&question);
        let out = session.submit(
            seat,
            &Submit {
                request_id: RequestId::new(request.clone()),
                command: Command::Answer {
                    question_id: question.id,
                    answer,
                },
            },
        );
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

fn setup() -> String {
    let main = [MAIN_DECK; 20].concat();
    let deck =
        json!({"construction":{"kind":"class","value":"ニュートラル"},"main":main,"evolve":[]});
    json!({"decks":[deck, deck],"first_chooser":"p2","seed":"battle-host-load",
        "control":["player","policy"],"max_submissions":10_000_u16})
    .to_string()
}

#[test]
fn loaded_policy_seat_plays_inside_the_host_against_a_player_channel() {
    let mut session = load(inputs(), &setup()).unwrap();
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
