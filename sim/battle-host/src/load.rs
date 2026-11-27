//! Build a local session from the battle inputs document; nothing reads the filesystem.

use crate::session::{LocalSession, SeatControl};
use crate::wire::{Identity, Region, Seat, viewer_of};
use alloc::collections::BTreeMap;
use core::fmt;
use serde::Deserialize;
use serde_json::value::RawValue;
use sha2::{Digest as _, Sha256};
use sve_engine::battle::{BattleReferences, ReferenceInput};
use sve_engine::catalog::{Catalog, EngineIdentityInput, ProgramPrinting, SnapshotPrograms};
use sve_engine::game::Game;
use sve_engine::game::player::{DeckConstruction, PlayerDeck, StrictStartError};
use sve_engine::host::HostLimits;

/// The trusted host's match parameters. The seed and first chooser stay inside the Worker.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchSetup<Deck = DeckSetup> {
    /// Submitted decks in seat order.
    pub decks: [Deck; 2],
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
    /// A malformed inputs or setup document.
    Format,
    /// The snapshot, keywords, programs or rules were rejected.
    Selection,
    /// The stated content identity does not match the loaded programs or rules.
    Identity,
    /// The card references do not bind exactly this snapshot and every source the decks reach.
    References,
    /// The decks or opening could not start a match.
    Match,
    /// The submitting seat's deck document is malformed.
    DeckFormat,
    /// At least one submitted printing does not exist in the snapshot.
    DeckPrinting,
    /// The submitting seat's deck could not enter strict play.
    DeckAdmission,
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
            Self::DeckFormat => "deck_format",
            Self::DeckPrinting => "deck_printing",
            Self::DeckAdmission => "deck_admission",
        }
    }
}

/// Opening failure with the detailed code scoped to each submitted deck.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct MatchLoadError {
    shared: LoadError,
    seats: [LoadError; 2],
}

impl fmt::Debug for MatchLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MatchLoadError")
            .field("shared", &self.shared)
            .finish_non_exhaustive()
    }
}

impl MatchLoadError {
    /// Code safe to use without a seat.
    #[must_use]
    pub const fn code(self) -> &'static str {
        self.shared.code()
    }

    /// Code to send only to the channel bound to this seat.
    #[must_use]
    pub const fn code_for(self, seat: Seat) -> &'static str {
        let [p1, p2] = self.seats;
        match seat {
            Seat::P1 => p1.code(),
            Seat::P2 => p2.code(),
        }
    }
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
    pub fn open(&self, setup: &str) -> Result<LocalSession, MatchLoadError> {
        let setup: MatchSetup<Box<RawValue>> =
            serde_json::from_str(setup).map_err(|_error| MatchLoadError {
                shared: LoadError::Format,
                seats: [LoadError::Format; 2],
            })?;
        let decks = setup.decks.each_ref().map(|input| self.resolve_deck(input));
        if let [Ok(p1), Ok(p2)] = &decks
            && let Ok(game) =
                self.open_game([p1.clone(), p2.clone()], setup.first_chooser, &setup.seed)
        {
            return Ok(LocalSession::new(
                game,
                self.references.clone(),
                self.identity.clone(),
                setup.control.map(|control| match control {
                    Control::Player => SeatControl::Player,
                    Control::Policy => SeatControl::Policy,
                }),
                HostLimits {
                    max_submissions: setup.max_submissions,
                    max_retries: 1,
                },
            ));
        }
        // A match-level rejection cannot decide another seat's diagnostic. Mirroring
        // each deck here makes its admission result independent of the other submission.
        let seats = decks.map(|resolved| {
            resolved
                .and_then(|deck| {
                    self.open_game([deck.clone(), deck], setup.first_chooser, &setup.seed)
                })
                .err()
                .unwrap_or(LoadError::Match)
        });
        Err(MatchLoadError {
            shared: LoadError::Match,
            seats,
        })
    }

    fn resolve_deck(&self, input: &RawValue) -> Result<PlayerDeck, LoadError> {
        let mut input: DeckSetup =
            serde_json::from_str(input.get()).map_err(|_error| LoadError::DeckFormat)?;
        for printing in input.main.iter_mut().chain(&mut input.evolve) {
            *printing = self
                .programs
                .native_printing(printing)
                .ok_or(LoadError::DeckPrinting)?
                .to_owned();
        }
        Ok(PlayerDeck {
            construction: match input.construction {
                Construction::Class(class) => DeckConstruction::Class(class),
                Construction::Title(title) => DeckConstruction::Title(title),
            },
            main: input.main,
            evolve: input.evolve,
        })
    }

    fn open_game(
        &self,
        decks: [PlayerDeck; 2],
        first_chooser: Seat,
        seed: &str,
    ) -> Result<Game, LoadError> {
        let selected = Game::select_snapshot_decks(&self.programs, &decks)
            .map_err(|_error| LoadError::Selection)?;
        let admission = Game::check_battle_decks(&selected, &decks, &self.references)
            .map_err(|_error| LoadError::References)?;
        if !admission.is_admitted() {
            return Err(LoadError::DeckAdmission);
        }
        Game::start_player_game(selected, decks, viewer_of(first_chooser), seed).map_err(|error| {
            match error {
                StrictStartError::Admission(_) | StrictStartError::OpeningHand(_) => {
                    LoadError::DeckAdmission
                }
                StrictStartError::Initialization(_) => LoadError::Match,
            }
        })
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

fn hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .flat_map(|byte| [byte >> 4_u8, byte & 15])
        .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
        .collect()
}
