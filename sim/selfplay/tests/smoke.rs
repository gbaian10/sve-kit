//! A few fixed seeds on the public card inputs: real cards through the whole wire path.
//!
//! This is a smoke test, not the search. Large runs are a local job; see the crate README.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Missing or malformed generated inputs invalidate this test."
)]

extern crate alloc;

#[path = "../../battle-host/tests/support/fixture.rs"]
mod fixture;

#[path = "../../engine/tests/support/rule_fixtures.rs"]
mod rule_fixtures;

use alloc::collections::BTreeSet;
use serde_json::Value;
use std::env::var_os;
use std::fs::read_to_string;
use std::path::PathBuf;
use sve_battle_contract::Seat;
use sve_battle_host::{LocalSession, SeatControl};
use sve_engine::battle::BattleReferences;
use sve_engine::catalog::CurrentSelectionLoader;
use sve_engine::game::Game;
use sve_engine::game::player::{DeckConstruction, PlayerDeck, StrictStartError, Viewer};
use sve_engine::host::HostLimits;
use sve_selfplay::arena::{Arena, Setup};
use sve_selfplay::play::{End, Limits, play, replay};
use sve_selfplay::pool::{Basis, Deck, PoolCard, Pools};

/// The generated author-test inputs and their bound rules, as the host tests load them.
fn texts() -> [String; 4] {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let snapshot = read_to_string(var_os("SVE_TEST_SNAPSHOT").unwrap()).unwrap();
    let selection = read_to_string(var_os("SVE_TEST_CURRENT_SELECTION").unwrap()).unwrap();
    let keywords = read_to_string(root.join("authored/rules/keyword-registry.yaml")).unwrap();
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
}

/// Class pools over every card with an accepted author, one printing per card.
fn cards(snapshot: &str, selection: &Value) -> Vec<PoolCard> {
    let authored: BTreeSet<&str> = selection["authors"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|author| author.pointer("/input/card_no")?.as_str())
        .collect();
    snapshot
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|record| {
            let number = record["number"].as_str()?.to_owned();
            let face = &record["faces"][0];
            let kind = face["card_type"].as_str()?;
            let deck_card = ["フォロワー", "スペル", "アミュレット"]
                .iter()
                .any(|base| kind.contains(base))
                && !kind.contains("トークン");
            (deck_card && authored.contains(number.as_str())).then(|| PoolCard {
                reference: number.clone(),
                name: face["name"].as_str().unwrap_or(&number).to_owned(),
                class: face["card_class"].as_str().map(str::to_owned),
                // Class construction only: title decks open with their own procedures.
                title: None,
                evolve: kind.contains("エボルヴ") || kind.contains("アドバンス"),
                start_amulet: false,
                number,
            })
        })
        .collect()
}

fn arena() -> Arena {
    let [snapshot, keywords, selection, rule_source] = texts();
    let (rules, identity) = rule_fixtures::resolved(&snapshot, &rule_source).unwrap();
    let selection: Value = serde_json::from_str(&selection).unwrap();
    let pool = cards(&snapshot, &selection);
    let printings = pool
        .iter()
        .map(|card| (card.reference.clone(), format!("test-{}", card.number)))
        .collect();
    let numbers = snapshot
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|record| record["number"].as_str().map(str::to_owned))
        .map(|number| (format!("test-{number}"), number))
        .collect();
    let loader = CurrentSelectionLoader::new().unwrap();
    Arena {
        open: Box::new(move |setup: &Setup| {
            let deck = |deck: &Deck| PlayerDeck {
                construction: match &deck.construction {
                    Basis::Class(class) => DeckConstruction::Class(class.clone()),
                    Basis::Title(title) => DeckConstruction::Title(title.clone()),
                },
                main: deck.main.clone(),
                evolve: deck.evolve.clone(),
            };
            let decks = [deck(&setup.decks[0]), deck(&setup.decks[1])];
            let selected = Game::select_player_decks(
                &loader,
                &snapshot,
                &keywords,
                &selection,
                Some((&rules, &identity)),
                &decks,
            )
            .map_err(|error| error.to_string())?;
            let references =
                BattleReferences::new(&selected.catalogue, fixture::references(&snapshot))
                    .map_err(|error| error.to_string())?;
            let chooser = match setup.first_chooser {
                Seat::P1 => Viewer::P1,
                Seat::P2 => Viewer::P2,
            };
            let game = Game::start_player_game(selected, decks, chooser, &setup.seed).map_err(
                |error| match error {
                    // The stage code that makes the run rebuild the decks with single copies.
                    StrictStartError::Admission(_) => "match".to_owned(),
                    error @ StrictStartError::OpeningHand(_) => error.to_string(),
                    StrictStartError::Initialization(fault) => fault.to_string(),
                },
            )?;
            Ok(LocalSession::new(
                game,
                references,
                fixture::identity("", ""),
                [SeatControl::Player, SeatControl::Player],
                HostLimits::default(),
            ))
        }),
        pools: Pools::new(pool, &BTreeSet::new()),
        printings,
        numbers,
    }
}

#[test]
fn fixed_seeds_play_real_cards_without_a_fault_and_replay() {
    let arena = arena();
    assert!(arena.pools.eligible().len() > 1_000);
    // Past the opening and into the first turns; a debug build takes about 0.1 s a submission.
    let limits = Limits {
        decisions: 30,
        ..Limits::default()
    };
    let stopped = |end: &End| match end {
        End::Finished { .. } => true,
        End::Failed(failure) => failure.kind == "decision_limit",
    };
    let [first, second] = [1, 2].map(|seed| play(&arena, seed, limits));
    for played in [&first, &second] {
        let seed = played.record.seed;
        assert!(stopped(&played.end), "seed {seed}: {:?}", played.end);
        assert!(!played.seen.is_empty(), "seed {seed} showed no card");
    }
    assert_ne!(first.record.setup, second.record.setup);
    let replayed = replay(&arena, &first.record, limits);
    assert_eq!(replayed.record, first.record);
    assert!(stopped(&replayed.end), "{:?}", replayed.end);
}
