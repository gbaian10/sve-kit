//! Build a local session from host-supplied text; nothing reads the filesystem.

use crate::session::{LocalSession, SeatControl};
use crate::wire::Seat;
use serde::Deserialize;
use serde_json::Value;
use sve_engine::catalog::{CurrentSelectionLoader, EngineIdentityInput};
use sve_engine::game::Game;
use sve_engine::game::player::{DeckConstruction, PlayerDeck, Viewer};
use sve_engine::host::HostLimits;

/// The trusted host's match parameters. The seed and first chooser stay inside the Worker.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchSetup {
    /// Host-validated decks in seat order.
    pub decks: [DeckSetup; 2],
    /// Seat that chooses who goes first.
    pub first_chooser: Seat,
    /// Private randomness seed.
    pub seed: String,
    /// Who answers each seat's questions, in seat order.
    pub control: [Control; 2],
    /// Total policy submissions allowed, including delegated player answers.
    pub max_submissions: u64,
}

/// One validated deck.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeckSetup {
    /// Construction basis.
    pub construction: Construction,
    /// Snapshot printing references, one per main-deck card.
    pub main: Vec<String>,
    /// Snapshot printing references, one per evolve-deck card.
    pub evolve: Vec<String>,
}

/// Deck construction basis.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Construction {
    /// Printed class value.
    Class(String),
    /// Registered engine title code.
    Title(String),
}

/// Seat controller on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Control {
    /// A player channel.
    Player,
    /// The basic policy.
    Policy,
}

/// Why a session could not be created; engine diagnostics stay out of the message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// A malformed selection or setup document.
    Format,
    /// The snapshot, keywords, selection or rules were rejected.
    Selection,
    /// The decks or opening could not start a match.
    Match,
}

impl LoadError {
    /// Stable code for the host.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Format => "format",
            Self::Selection => "selection",
            Self::Match => "match",
        }
    }
}

/// Inputs exactly as the host downloaded them.
#[derive(Clone, Copy, Debug)]
pub struct Inputs<'text> {
    /// Engine card snapshot (JSON lines).
    pub snapshot: &'text str,
    /// Keyword registry.
    pub keywords: &'text str,
    /// Current effect selection (JSON).
    pub selection: &'text str,
    /// Engine rule resources bound to this snapshot.
    pub rules: &'text str,
}

/// Select effects, start the opening and bind the seat controls.
///
/// # Errors
/// Returns the failing stage without engine diagnostics.
pub fn load(inputs: Inputs<'_>, setup: &str) -> Result<LocalSession, LoadError> {
    let setup: MatchSetup = serde_json::from_str(setup).map_err(|_error| LoadError::Format)?;
    let selection: Value =
        serde_json::from_str(inputs.selection).map_err(|_error| LoadError::Format)?;
    let selected = CurrentSelectionLoader::new()
        .and_then(|loader| {
            // The first slice loads the reviewed JP inputs; resolved identities come with strict admission.
            loader.select_with_rules(
                inputs.snapshot,
                inputs.keywords,
                &selection,
                inputs.rules,
                &EngineIdentityInput::LegacyJp,
            )
        })
        .map_err(|_error| LoadError::Selection)?;
    let [first, second] = setup.decks;
    let game = Game::start_player_game(
        selected,
        [deck(first), deck(second)],
        Viewer::from(setup.first_chooser),
        &setup.seed,
    )
    .map_err(|_error| LoadError::Match)?;
    Ok(LocalSession::new(
        game,
        setup.control.map(|control| match control {
            Control::Player => SeatControl::Player,
            Control::Policy => SeatControl::Policy,
        }),
        HostLimits {
            max_submissions: setup.max_submissions,
            max_retries: 1,
        },
    ))
}

fn deck(setup: DeckSetup) -> PlayerDeck {
    PlayerDeck {
        construction: match setup.construction {
            Construction::Class(class) => DeckConstruction::Class(class),
            Construction::Title(title) => DeckConstruction::Title(title),
        },
        main: setup.main,
        evolve: setup.evolve,
    }
}
