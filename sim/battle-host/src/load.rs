//! Build a local session from host-supplied text; nothing reads the filesystem.

use crate::session::{LocalSession, SeatControl};
use crate::wire::{Identity, Region, Seat, viewer_of};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use sve_engine::battle::{BattleReferences, ReferenceInput};
use sve_engine::catalog::{CurrentSelectionLoader, EngineIdentityInput};
use sve_engine::game::Game;
use sve_engine::game::player::{DeckConstruction, PlayerDeck};
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
    /// Total submissions allowed for the policy seats.
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
    /// The stated content identity does not match the loaded programs or rules.
    Identity,
    /// The card references do not bind exactly this snapshot and every source the decks reach.
    References,
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
            Self::Identity => "identity",
            Self::References => "references",
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
    /// Printing, wording and rule identities produced with the snapshot by the same reader (JSON).
    pub references: &'text str,
    /// The contract `Identity` (JSON): the verified public manifest the snapshot and references
    /// were read from, the programs, the engine build and the rules. Each seat receives it in
    /// `Ready`.
    pub identity: &'text str,
}

/// Select the effects the decks can reach, bind the snapshot's references, check the whole
/// match closure, start the opening and bind the seat controls.
///
/// # Errors
/// Returns the failing stage without engine diagnostics.
pub fn load(inputs: Inputs<'_>, setup: &str) -> Result<LocalSession, LoadError> {
    let setup: MatchSetup = serde_json::from_str(setup).map_err(|_error| LoadError::Format)?;
    let selection: Value =
        serde_json::from_str(inputs.selection).map_err(|_error| LoadError::Format)?;
    let references: ReferenceInput =
        serde_json::from_str(inputs.references).map_err(|_error| LoadError::Format)?;
    let identity: Identity =
        serde_json::from_str(inputs.identity).map_err(|_error| LoadError::Format)?;
    let region = match identity.region {
        Region::Jp => "jp",
        Region::En => "en",
    };
    // The engine build cannot hash itself; the other identities must name exactly these bytes.
    if identity.programs.sha256 != hex(inputs.selection)
        || identity.rules.sha256 != hex(inputs.rules)
    {
        return Err(LoadError::Identity);
    }
    if references.region != region {
        return Err(LoadError::References);
    }
    let [first, second] = setup.decks;
    let decks = [deck(first), deck(second)];
    let selected = CurrentSelectionLoader::new()
        .and_then(|loader| {
            // Lowering every author took seconds in the browser; only the decks' reach is needed.
            // Permanent printing identities arrive separately as references, never inferred.
            Game::select_player_decks(
                &loader,
                inputs.snapshot,
                inputs.keywords,
                &selection,
                Some((inputs.rules, &EngineIdentityInput::LegacyJp)),
                &decks,
            )
        })
        .map_err(|_error| LoadError::Selection)?;
    // Every face of the snapshot, not only the decks' reach, must have an exact identity.
    let references = BattleReferences::new(&selected.prototypes, references)
        .map_err(|_error| LoadError::References)?;
    // The engine matches the wording and rule manifests with each other; rules supplied without
    // wording are matched only here, against the verified manifest this game is pinned to.
    if references
        .manifest_sha256()
        .is_some_and(|manifest| manifest.strip_prefix("sha256:") != Some(&identity.snapshot.sha256))
    {
        return Err(LoadError::References);
    }
    // The selection already loaded, so a failure here is a reference or source gap.
    let admission = Game::check_battle_decks(&selected, &decks, &references)
        .map_err(|_error| LoadError::References)?;
    if !admission.is_admitted() {
        return Err(LoadError::Match);
    }
    let game =
        Game::start_player_game(selected, decks, viewer_of(setup.first_chooser), &setup.seed)
            .map_err(|_error| LoadError::Match)?;
    Ok(LocalSession::new(
        game,
        references,
        identity,
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

fn hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .flat_map(|byte| [byte >> 4_u8, byte & 15])
        .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
        .collect()
}
