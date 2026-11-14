//! Whole local matches on the public engine inputs; boundaries live in `boundaries.rs`.

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
    InputState, Outgoing, ReceiptResult, Seat, Submit, SubmitCommand, TemporaryAnswer, Update,
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

fn session(control: [SeatControl; 2]) -> LocalSession {
    let decks = from_fn(|_| PlayerDeck {
        construction: DeckConstruction::Class("ニュートラル".into()),
        main: [["BP10-122", "BP01-173"]; 20]
            .concat()
            .into_iter()
            .map(Into::into)
            .collect(),
        evolve: Vec::new(),
    });
    let game =
        Game::start_player_game(selection().clone(), decks, Viewer::P2, "battle-host").unwrap();
    LocalSession::new(game, control, HostLimits::default())
}

fn absorb(deliveries: &[Delivery], latest: &mut [Option<Update>; 2]) {
    for delivery in deliveries {
        if let Outgoing::Update(update) = &delivery.message {
            let slot = &mut latest[usize::from(delivery.seat == Seat::P2)];
            if let Some(previous) = slot.as_ref() {
                assert!(update.update_seq > previous.update_seq, "{update:?}");
            }
            *slot = Some(update.clone());
        }
    }
}

/// Delegate every question until the match ends; returns each seat's last update.
fn play_out(session: &mut LocalSession) -> [Option<Update>; 2] {
    let mut latest: [Option<Update>; 2] = [None, None];
    absorb(&session.start(), &mut latest);
    for request in 0..5_000 {
        let open = latest
            .iter()
            .flatten()
            .find_map(|update| match &update.input {
                InputState::Question { id, .. } => Some((update.viewer, id.clone())),
                InputState::Waiting { .. }
                | InputState::Finished { .. }
                | InputState::EngineFault { .. } => None,
            });
        let Some((seat, id)) = open else {
            return latest;
        };
        let request = format!("r{request}");
        let out = session.submit(
            seat,
            Submit {
                request_id: request.clone(),
                command: SubmitCommand::Answer {
                    question_id: id,
                    answer: TemporaryAnswer::Policy,
                },
            },
        );
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
        Some(InputState::Finished { winner: Some(_) })
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
        Some(InputState::Finished { winner: Some(_) })
    ));
}
