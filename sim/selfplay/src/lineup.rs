//! Weighted deck lists read from a file, played in place of random construction.
//!
//! A list names each deck by card number and count only. Every match draws both seats'
//! decks independently by weight, so a heavy deck also meets itself.

use crate::pool::{Basis, Deck, EVOLVE_MAXIMUM, MAIN_MAXIMUM, MAIN_MINIMUM, NAME_LIMIT};
use alloc::collections::BTreeMap;
use rand::RngExt as _;
use rand_chacha::ChaCha8Rng;
use serde::Deserialize;

/// One deck as a list file spells it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListedDeck {
    /// Name for reports; the deck's position when absent.
    #[serde(default)]
    pub label: Option<String>,
    /// Relative weight; a deck of weight zero is refused.
    pub weight: u64,
    /// Construction basis.
    pub construction: Basis,
    /// Main-deck card numbers and their counts.
    pub main: BTreeMap<String, usize>,
    /// Evolve-deck card numbers and their counts.
    #[serde(default)]
    pub evolve: BTreeMap<String, usize>,
}

/// The list file.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeckList {
    /// Decks in file order.
    pub decks: Vec<ListedDeck>,
}

/// A listed deck the run will not play, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    /// The deck's label.
    pub label: String,
    /// Construction rule, unknown or ineligible printing, lowering, admission or load stage.
    pub reason: String,
}

/// Admitted decks with their cumulative weights.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lineup {
    entries: Vec<WeightedDeck>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WeightedDeck {
    label: String,
    deck: Deck,
    bound: u128,
}

impl Lineup {
    /// Add an admitted deck; one of weight zero is never drawn.
    pub fn push(&mut self, label: String, weight: u64, deck: Deck) {
        if weight == 0 {
            return;
        }
        let total = self.entries.last().map_or(0, |entry| entry.bound);
        self.entries.push(WeightedDeck {
            label,
            deck,
            bound: total.saturating_add(u128::from(weight)),
        });
    }

    /// Whether no deck can be drawn.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of admitted decks.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn card_references(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().flat_map(|entry| {
            entry
                .deck
                .main
                .iter()
                .chain(&entry.deck.evolve)
                .map(String::as_str)
        })
    }

    /// The label of a drawn deck.
    #[must_use]
    pub fn label(&self, index: usize) -> &str {
        self.entries
            .get(index)
            .map_or("", |entry| entry.label.as_str())
    }

    /// One deck drawn by weight, with its position.
    pub fn draw(&self, rng: &mut ChaCha8Rng) -> Option<(usize, Deck)> {
        let total = self.entries.last()?.bound;
        let point = rng.random_range(0..total);
        let index = self.entries.partition_point(|entry| entry.bound <= point);
        Some((index, self.entries.get(index)?.deck.clone()))
    }
}

/// Main deck of 40 to 50 cards, an evolve deck of at most ten, one to three of each entry.
///
/// # Errors
/// The first rule the deck breaks.
pub fn shape(deck: &ListedDeck) -> Result<(), String> {
    let size = |cards: &BTreeMap<String, usize>| {
        cards
            .values()
            .fold(0_usize, |total, count| total.saturating_add(*count))
    };
    if let Some((number, count)) = deck
        .main
        .iter()
        .chain(&deck.evolve)
        .find(|(_, count)| !(1..=NAME_LIMIT).contains(*count))
    {
        return Err(format!("{number} has {count} copies"));
    }
    let main = size(&deck.main);
    if !(MAIN_MINIMUM..=MAIN_MAXIMUM).contains(&main) {
        return Err(format!("main deck has {main} cards"));
    }
    let evolve = size(&deck.evolve);
    if evolve > EVOLVE_MAXIMUM {
        return Err(format!("evolve deck has {evolve} cards"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{DeckList, Lineup, shape};
    use crate::pool::{Basis, Deck};
    use alloc::collections::BTreeSet;
    use core::iter::repeat_with;
    use rand::SeedableRng as _;
    use rand_chacha::ChaCha8Rng;

    fn deck(name: &str) -> Deck {
        Deck {
            construction: Basis::Class(name.into()),
            main: Vec::new(),
            evolve: Vec::new(),
        }
    }

    #[test]
    fn draws_follow_the_weights_and_skip_weightless_decks() {
        let mut lineup = Lineup::default();
        assert!(lineup.draw(&mut ChaCha8Rng::seed_from_u64(0)).is_none());
        lineup.push("heavy".into(), 3, deck("heavy"));
        lineup.push("none".into(), 0, deck("none"));
        lineup.push("light".into(), 1, deck("light"));
        assert_eq!(lineup.len(), 2);
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let mut drawn = [0_u32; 2];
        for _ in 0..4_000_u32 {
            let (index, picked) = lineup.draw(&mut rng).unwrap();
            assert_eq!(picked, deck(lineup.label(index)));
            if let Some(count) = drawn.get_mut(index) {
                *count = count.saturating_add(1);
            }
        }
        // Three to one, within a generous margin.
        assert!((2_700..3_300).contains(&drawn[0]), "{drawn:?}");
    }

    #[test]
    fn equal_maximum_weights_can_both_be_drawn() {
        let mut lineup = Lineup::default();
        lineup.push("first".into(), u64::MAX, deck("first"));
        lineup.push("second".into(), u64::MAX, deck("second"));
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let drawn: BTreeSet<_> = repeat_with(|| lineup.draw(&mut rng).unwrap().0)
            .take(64)
            .collect();
        assert_eq!(drawn, BTreeSet::from([0, 1]));
    }

    #[test]
    fn the_file_states_card_numbers_and_counts_only() {
        let list: DeckList = serde_json::from_str(
            r#"{"decks":[{"weight":2,"construction":{"kind":"title","value":"t"},
                "main":{"A-00":3,"A-01":3,"A-02":3,"A-03":3,"A-04":3,"A-05":3,"A-06":3,
                    "A-07":3,"A-08":3,"A-09":3,"A-10":3,"A-11":3,"A-12":3,"A-13":1}}]}"#,
        )
        .unwrap();
        let [listed] = list.decks.as_slice() else {
            panic!("one deck");
        };
        assert_eq!(listed.label, None);
        assert!(listed.evolve.is_empty());
        assert_eq!(shape(listed), Ok(()));
        let mut wrong = listed.clone();
        wrong.main.insert("A-13".into(), 4);
        assert_eq!(shape(&wrong), Err("A-13 has 4 copies".into()));
        wrong.main.remove("A-13");
        assert_eq!(shape(&wrong), Err("main deck has 39 cards".into()));
        let mut evolve = listed.clone();
        evolve.evolve = (0..4_u8).map(|index| (format!("E-{index}"), 3)).collect();
        assert_eq!(shape(&evolve), Err("evolve deck has 12 cards".into()));
        let unknown = serde_json::from_str::<DeckList>(
            r#"{"decks":[{"weight":1,"construction":{"kind":"class","value":"c"},"main":{},"name":"x"}]}"#,
        );
        assert!(unknown.is_err(), "unknown fields are refused");
    }
}
