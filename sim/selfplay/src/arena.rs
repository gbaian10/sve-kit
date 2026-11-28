//! Where matches are opened: a session factory plus the card facts the run needs.

use crate::lineup::Lineup;
use crate::pool::{Deck, Pools};
use alloc::collections::BTreeMap;
use core::fmt;
use serde::{Deserialize, Serialize};
use sve_battle_contract::Seat;
use sve_battle_host::LocalSession;

/// Seat controller in the host's setup document; self-play seats are always players.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Control {
    /// A wire player.
    Player,
}

/// The host's match parameters; a record keeps them verbatim.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Setup {
    /// Decks in seat order.
    pub decks: [Deck; 2],
    /// Seat that chooses who goes first.
    pub first_chooser: Seat,
    /// Engine randomness seed.
    pub seed: String,
    /// Both seats answer over the wire.
    pub control: [Control; 2],
    /// Budget of the host's own policy seats; unused here.
    pub max_submissions: u64,
}

/// Opens a session for a setup, or says which load stage refused it.
pub type Open = Box<dyn Fn(&Setup) -> Result<LocalSession, String> + Send + Sync>;

/// One prepared snapshot: how to open matches on it and what its printings are.
pub struct Arena {
    /// Session factory.
    pub open: Open,
    /// Construction pools over the admitted cards.
    pub pools: Pools,
    /// Listed decks drawn by weight in place of random construction.
    pub lineup: Option<Lineup>,
    /// Wire printing identifier of every deck reference.
    pub printings: BTreeMap<String, String>,
    /// Native card number of every wire printing identifier, for reports.
    pub numbers: BTreeMap<String, String>,
}

impl fmt::Debug for Arena {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Arena")
            .field("pools", &self.pools)
            .finish_non_exhaustive()
    }
}

impl Arena {
    /// The card number of a wire printing, or the identifier itself when unknown.
    #[must_use]
    pub fn number<'text>(&'text self, printing: &'text str) -> &'text str {
        self.numbers.get(printing).map_or(printing, String::as_str)
    }
}
