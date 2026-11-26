//! Build a local session from the battle inputs document; nothing reads the filesystem.

use crate::session::{LocalSession, SeatControl};
use crate::wire::{Identity, Region, Seat, viewer_of};
use alloc::collections::BTreeMap;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use sve_engine::battle::{BattleReferences, ReferenceInput};
use sve_engine::catalog::{
    Catalog, CurrentSelectionLoader, EngineIdentityInput, ProgramPrinting, SelectedEffects,
    SnapshotPrograms,
};
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

/// The battle inputs document `build:engine` writes from one verified public export: the
/// reader's native snapshot and references, the programs and engine rules bound to those exact
/// snapshot bytes, and the content identity naming them.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inputs {
    /// Engine card snapshot (JSON lines).
    snapshot: String,
    /// Keyword registry.
    keywords: String,
    /// Current effect selection (JSON).
    selection: String,
    /// Engine rule resources bound to this snapshot.
    rules: String,
    /// Printing, wording and rule identities produced with the snapshot by the same reader.
    references: ReferenceInput,
    /// The verified public manifest, the programs, the engine build and the rules; each seat
    /// receives it in `Ready`.
    identity: Identity,
}

/// Select the effects the decks can reach, bind the snapshot's references, check the whole
/// match closure, start the opening and bind the seat controls.
///
/// `inputs` is the battle inputs document (JSON) exactly as `build:engine` wrote it.
///
/// # Errors
/// Returns the failing stage without engine diagnostics.
pub fn load(inputs: &str, setup: &str) -> Result<LocalSession, LoadError> {
    let setup: MatchSetup = serde_json::from_str(setup).map_err(|_error| LoadError::Format)?;
    let Inputs {
        snapshot,
        keywords,
        selection: programs,
        rules,
        references,
        identity,
    } = serde_json::from_str(inputs).map_err(|_error| LoadError::Format)?;
    let selection: Value = serde_json::from_str(&programs).map_err(|_error| LoadError::Format)?;
    let region = match identity.region {
        Region::Jp => "jp",
        Region::En => "en",
    };
    // The engine build cannot hash itself; the other identities must name exactly these bytes.
    if identity.programs.sha256 != hex(&programs) || identity.rules.sha256 != hex(&rules) {
        return Err(LoadError::Identity);
    }
    // The references name the exact native bytes they were read with, not a display catalogue.
    if references.region != region || references.snapshot_sha256 != hex(&snapshot) {
        return Err(LoadError::References);
    }
    let [first, second] = setup.decks.clone();
    let decks = [deck(first), deck(second)];
    let selected = CurrentSelectionLoader::new()
        .and_then(|loader| {
            // Lowering every author took seconds in the browser; only the decks' reach is needed.
            // Permanent printing identities arrive separately as references, never inferred.
            Game::select_player_decks(
                &loader,
                &snapshot,
                &keywords,
                &selection,
                Some((&rules, &EngineIdentityInput::LegacyJp)),
                &decks,
            )
        })
        .map_err(|_error| LoadError::Selection)?;
    let references = bind_references(&selected.prototypes, references, &identity)?;
    open(selected, decks, references, identity, &setup)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PublishedInputs {
    snapshot: String,
    keywords: String,
    rules: String,
    references: ReferenceInput,
    identity: Identity,
    printings: Vec<ProgramPrinting>,
    engine_identity: EngineIdentityInput,
}

/// One verified snapshot input, reusable across local matches.
#[derive(Debug)]
pub struct PreparedBattle {
    programs: SnapshotPrograms,
    references: BattleReferences,
    identity: Identity,
}

impl PreparedBattle {
    /// Bind the published program bytes to the exact native catalogue and manifest identities.
    ///
    /// # Errors
    /// Refuses malformed, mismatched or unsupported published inputs.
    pub fn new(inputs: &str, programs: &str) -> Result<Self, LoadError> {
        let document: PublishedInputs =
            serde_json::from_str(inputs).map_err(|_error| LoadError::Format)?;
        if document.identity.region != Region::Jp
            || document.identity.programs.sha256 != hex(programs)
            || document.identity.rules.sha256 != hex(&document.rules)
        {
            return Err(LoadError::Identity);
        }
        if document.references.region != "jp"
            || document.references.snapshot_sha256 != hex(&document.snapshot)
            || !same_printings(&document.references, &document.printings)
        {
            return Err(LoadError::References);
        }
        let programs = SnapshotPrograms::new(
            &document.snapshot,
            &document.keywords,
            programs,
            &document.printings,
            &document.rules,
            &document.engine_identity,
        )
        .map_err(|_error| LoadError::Selection)?;
        let references = bind_references(
            programs.catalogue(),
            document.references,
            &document.identity,
        )?;
        Ok(Self {
            programs,
            references,
            identity: document.identity,
        })
    }

    /// Start a match from these assets, lowering only the decks' reachable programs.
    ///
    /// # Errors
    /// Refuses malformed setups, illegal decks or reachable programs the engine cannot admit.
    pub fn open(&self, setup: &str) -> Result<LocalSession, LoadError> {
        let setup: MatchSetup = serde_json::from_str(setup).map_err(|_error| LoadError::Format)?;
        let decks = setup.decks.clone().map(|mut input| {
            for number in input.main.iter_mut().chain(&mut input.evolve) {
                if let Some(locator) = self.programs.native_printing(number) {
                    *number = locator.to_owned();
                }
            }
            deck(input)
        });
        let selected = Game::select_snapshot_decks(&self.programs, &decks)
            .map_err(|_error| LoadError::Selection)?;
        open(
            selected,
            decks,
            self.references.clone(),
            self.identity.clone(),
            &setup,
        )
    }
}

fn same_printings(references: &ReferenceInput, printings: &[ProgramPrinting]) -> bool {
    let indexed = printings
        .iter()
        .map(|printing| (printing.card_no.as_str(), printing))
        .collect::<BTreeMap<_, _>>();
    let Some(wording) = &references.wording else {
        return false;
    };
    indexed.len() == printings.len()
        && references.faces.iter().all(|face| {
            indexed
                .get(face.card_no.as_str())
                .is_some_and(|printing| printing.printing_id == face.card.printing_id.as_str())
        })
        && wording.faces.iter().all(|face| {
            indexed
                .get(face.card_no.as_str())
                .and_then(|printing| printing.faces.get(usize::try_from(face.native_face).ok()?))
                .is_some_and(|projected| {
                    projected.face_id == face.face_id
                        && face
                            .revisions
                            .iter()
                            .any(|revision| revision.revision_id == projected.revision_id)
                })
        })
}

fn bind_references(
    catalogue: &Catalog,
    references: ReferenceInput,
    identity: &Identity,
) -> Result<BattleReferences, LoadError> {
    let references =
        BattleReferences::new(catalogue, references).map_err(|_error| LoadError::References)?;
    if references
        .manifest_sha256()
        .and_then(|manifest| manifest.strip_prefix("sha256:"))
        != Some(identity.snapshot.sha256.as_str())
    {
        return Err(LoadError::References);
    }
    Ok(references)
}

fn open(
    selected: SelectedEffects,
    decks: [PlayerDeck; 2],
    references: BattleReferences,
    identity: Identity,
    setup: &MatchSetup,
) -> Result<LocalSession, LoadError> {
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
