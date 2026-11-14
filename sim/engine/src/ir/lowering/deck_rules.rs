//! Source-checked construction metadata, separate from in-game abilities.

use alloc::vec::Vec;
use serde::Deserialize;

/// The independently counted physical deck section.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeckSection {
    /// Main deck.
    Main,
    /// Evolve deck.
    Evolve,
}

/// Activation condition of a printed construction restriction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeckRuleCondition {
    /// At least one printing of the permanent source card is in the main deck.
    SelfInMain,
}

/// Typed card-level metadata retained after complete source reconciliation.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum CardDeckRule {
    /// Existing printed exception for this card's own name; host construction applies it.
    SelfName {
        /// Section to which the exception applies.
        deck: DeckSection,
        /// Printed maximum.
        same_name_max: u64,
    },
    /// A restriction on every non-exempt construction name in each selected section.
    Conditional {
        /// Presence condition, checked before counting cards.
        when: DeckRuleCondition,
        /// Nonempty, unique sections, enforced by the author schema.
        decks: Vec<DeckSection>,
        /// Literal substring of exempt construction names.
        exclude_name_contains: String,
        /// Maximum per name and per section.
        same_name_max: u64,
    },
}
