//! Construction pools and random legal decks over every admitted printing.
//!
//! Base construction only: 40 to 50 main cards and up to 10 evolve cards of one class plus
//! neutral, or of one title, with at most three cards per rules name. Tournament formats and
//! ban lists are deliberately ignored so that restricted cards stay exercised.

use alloc::collections::{BTreeMap, BTreeSet};
use core::iter::repeat_n;
use rand::RngExt as _;
use rand::seq::SliceRandom as _;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

pub(crate) const NEUTRAL: &str = "ニュートラル";
pub(crate) const MAIN_MINIMUM: usize = 40;
pub(crate) const MAIN_MAXIMUM: usize = 50;
const MAIN_SPREAD: usize = MAIN_MAXIMUM - MAIN_MINIMUM;
pub(crate) const EVOLVE_MAXIMUM: usize = 10;
pub(crate) const NAME_LIMIT: usize = 3;

/// One deck-eligible printing and the facts construction needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoolCard {
    /// Reference the host's deck setup takes for this printing.
    pub reference: String,
    /// Native card number, for reports.
    pub number: String,
    /// Rules name shared by every printing under the three-copy limit.
    pub name: String,
    /// Printed class, when the card has one.
    pub class: Option<String>,
    /// Registered title code, when the card belongs to a title.
    pub title: Option<String>,
    /// Whether the card belongs to the evolve deck.
    pub evolve: bool,
    /// Whether the card is a Start Amulet.
    pub start_amulet: bool,
}

/// Construction basis of a deck, as the host's setup document spells it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Basis {
    /// Printed class value.
    Class(String),
    /// Registered engine title code.
    Title(String),
}

/// One deck in the host's setup shape.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deck {
    /// Construction basis.
    pub construction: Basis,
    /// One reference per main-deck card.
    pub main: Vec<String>,
    /// One reference per evolve-deck card.
    pub evolve: Vec<String>,
}

#[derive(Debug)]
struct Pool {
    basis: Basis,
    main: Vec<usize>,
    evolve: Vec<usize>,
    mandatory_amulet: bool,
}

/// Every construction basis that can field a legal deck from the admitted cards.
#[derive(Debug)]
pub struct Pools {
    cards: Vec<PoolCard>,
    pools: Vec<Pool>,
}

impl Pools {
    /// Group the admitted cards by class (with neutral) and by title.
    ///
    /// `amulet_titles` lists the title codes whose opening requires a Start Amulet.
    #[must_use]
    pub fn new(cards: Vec<PoolCard>, amulet_titles: &BTreeSet<String>) -> Self {
        let mut classes = BTreeMap::<&str, Vec<usize>>::new();
        let mut titles = BTreeMap::<&str, Vec<usize>>::new();
        let mut neutral = Vec::new();
        for (index, card) in cards.iter().enumerate() {
            match card.class.as_deref() {
                Some(NEUTRAL) => neutral.push(index),
                Some(class) => classes.entry(class).or_default().push(index),
                None => {}
            }
            if let Some(title) = card.title.as_deref() {
                titles.entry(title).or_default().push(index);
            }
        }
        let pool = |basis: Basis, members: Vec<usize>, mandatory_amulet: bool| {
            let (evolve, main) = members
                .into_iter()
                .partition(|index| cards.get(*index).is_some_and(|card| card.evolve));
            Pool {
                basis,
                main,
                evolve,
                mandatory_amulet,
            }
        };
        let pools = classes
            .into_iter()
            .map(|(class, mut members)| {
                members.extend(&neutral);
                pool(Basis::Class(class.to_owned()), members, false)
            })
            .chain(titles.into_iter().map(|(title, members)| {
                pool(
                    Basis::Title(title.to_owned()),
                    members,
                    amulet_titles.contains(title),
                )
            }))
            .filter(|pool| {
                let names: BTreeSet<_> = pool
                    .main
                    .iter()
                    .filter_map(|index| cards.get(*index))
                    .filter(|card| !card.start_amulet)
                    .map(|card| card.name.as_str())
                    .collect();
                let amulet = pool
                    .main
                    .iter()
                    .filter_map(|index| cards.get(*index))
                    .any(|card| card.start_amulet);
                // Enough names for the largest deck even at one copy each.
                names.len() >= MAIN_MAXIMUM && (!pool.mandatory_amulet || amulet)
            })
            .collect();
        Self { cards, pools }
    }

    /// Whether any basis can field a deck.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.pools.is_empty()
    }

    /// Every printing some pool can put in a deck.
    #[must_use]
    pub fn eligible(&self) -> BTreeSet<&str> {
        self.pools
            .iter()
            .flat_map(|pool| pool.main.iter().chain(&pool.evolve))
            .filter_map(|index| self.cards.get(*index))
            .map(|card| card.number.as_str())
            .collect()
    }

    /// Admitted printings no pool holds: no class, and a title that cannot field a deck.
    #[must_use]
    pub fn unpooled(&self) -> Vec<&str> {
        let eligible = self.eligible();
        self.cards
            .iter()
            .map(|card| card.number.as_str())
            .filter(|number| !eligible.contains(number))
            .collect()
    }

    /// A random legal deck; `copies` caps the cards per rules name (one to three).
    ///
    /// Returns `None` only when no pool exists.
    pub fn deck(&self, copies: usize, rng: &mut ChaCha8Rng) -> Option<Deck> {
        let pool = self
            .pools
            .get(rng.random_range(0..self.pools.len().max(1)))?;
        let limit = copies.clamp(1, NAME_LIMIT);
        let size = MAIN_MINIMUM.saturating_add(if rng.random() {
            0
        } else {
            rng.random_range(0..=MAIN_SPREAD)
        });
        let mut main = Vec::new();
        let mut names = BTreeMap::<&str, usize>::new();
        let mut candidates: Vec<&PoolCard> = pool
            .main
            .iter()
            .filter_map(|index| self.cards.get(*index))
            .collect();
        candidates.shuffle(rng);
        if pool.mandatory_amulet
            && let Some(amulet) = candidates.iter().find(|card| card.start_amulet)
        {
            main.push(amulet.reference.clone());
        }
        for card in candidates.iter().filter(|card| !card.start_amulet) {
            let used = names.entry(card.name.as_str()).or_default();
            let room = size
                .saturating_sub(main.len())
                .min(limit.saturating_sub(*used));
            if room == 0 {
                continue;
            }
            let take = rng.random_range(1..=room);
            *used = used.saturating_add(take);
            main.extend(repeat_n(card.reference.clone(), take));
        }
        Some(Deck {
            construction: pool.basis.clone(),
            evolve: self.evolve(pool, &names, limit, rng),
            main,
        })
    }

    fn evolve(
        &self,
        pool: &Pool,
        main_names: &BTreeMap<&str, usize>,
        limit: usize,
        rng: &mut ChaCha8Rng,
    ) -> Vec<String> {
        // Mostly full evolve decks: evolution is where most forms get exercised.
        let size = if rng.random_range(0..10_u8) < 7 {
            EVOLVE_MAXIMUM
        } else {
            rng.random_range(0..=EVOLVE_MAXIMUM)
        };
        let mut candidates: Vec<&PoolCard> = pool
            .evolve
            .iter()
            .filter_map(|index| self.cards.get(*index))
            .collect();
        candidates.shuffle(rng);
        // Forms of followers in the main deck first; an unrelated form can rarely be used.
        candidates.sort_by_key(|card| !main_names.contains_key(card.name.as_str()));
        let mut evolve = Vec::new();
        let mut names = BTreeMap::<&str, usize>::new();
        for card in candidates {
            let used = names.entry(card.name.as_str()).or_default();
            let room = size
                .saturating_sub(evolve.len())
                .min(limit.saturating_sub(*used));
            if room == 0 {
                continue;
            }
            let take = rng.random_range(1..=room);
            *used = used.saturating_add(take);
            evolve.extend(repeat_n(card.reference.clone(), take));
        }
        evolve
    }
}

#[cfg(test)]
mod tests {
    use super::{Basis, PoolCard, Pools};
    use alloc::collections::{BTreeMap, BTreeSet};
    use rand::SeedableRng as _;
    use rand_chacha::ChaCha8Rng;

    fn card(number: usize, class: &str, title: Option<&str>, evolve: bool) -> PoolCard {
        PoolCard {
            reference: format!("p-{class}-{number}"),
            number: format!("{class}-{number}"),
            // Two printings share every rules name.
            name: format!("{class}-{}", number >> 1_u8),
            class: Some(class.to_owned()),
            title: title.map(str::to_owned),
            evolve,
            start_amulet: false,
        }
    }

    fn pools() -> Pools {
        let mut cards: Vec<PoolCard> = (0..80)
            .map(|number| card(number, "ロイヤル", None, number >= 60))
            .chain(
                (0..120).map(|number| card(number, "ニュートラル", Some("guild"), number >= 110)),
            )
            .collect();
        cards.extend((0..2).map(|number| PoolCard {
            start_amulet: true,
            ..card(number, "amulet", Some("guild"), false)
        }));
        // Too few names to field a deck, and no class to join another pool.
        cards.extend((0..4).map(|number| PoolCard {
            class: None,
            ..card(number, "tiny", Some("tiny"), false)
        }));
        Pools::new(cards, &BTreeSet::from(["guild".to_owned()]))
    }

    #[test]
    fn every_deck_is_legal_for_its_basis() {
        let pools = pools();
        assert!(!pools.is_empty());
        assert!(pools.eligible().contains("ロイヤル-0"));
        assert!(!pools.eligible().contains("tiny-0"));
        assert_eq!(pools.unpooled(), ["tiny-0", "tiny-1", "tiny-2", "tiny-3"]);
        let mut bases = BTreeSet::new();
        for seed in 0..60_u64 {
            let copies = usize::try_from(seed.rem_euclid(3))
                .unwrap()
                .saturating_add(1);
            let deck = pools
                .deck(copies, &mut ChaCha8Rng::seed_from_u64(seed))
                .unwrap();
            assert!((40..=50).contains(&deck.main.len()), "{deck:?}");
            assert!(deck.evolve.len() <= 10, "{deck:?}");
            let title = deck.construction == Basis::Title("guild".into());
            bases.insert(title);
            let name = |reference: &String| {
                let (class, number) = reference.trim_start_matches("p-").split_once('-').unwrap();
                (class.to_owned(), number.parse::<usize>().unwrap())
            };
            for (section, evolve) in [(&deck.main, false), (&deck.evolve, true)] {
                let mut names = BTreeMap::<_, usize>::new();
                for reference in section {
                    let (class, number) = name(reference);
                    assert!(class != "ロイヤル" || !title, "{reference}");
                    if class != "amulet" {
                        let form = number >= if class == "ロイヤル" { 60 } else { 110 };
                        assert_eq!(form, evolve, "{reference} is in the wrong deck");
                    }
                    let count = names.entry((class, number >> 1_u8)).or_default();
                    *count = usize::saturating_add(*count, 1);
                }
                assert!(names.values().all(|count| *count <= copies), "{deck:?}");
            }
            let amulets = deck
                .main
                .iter()
                .filter(|card| name(card).0 == "amulet")
                .count();
            assert_eq!(amulets, usize::from(title), "{deck:?}");
        }
        assert_eq!(
            bases.len(),
            2,
            "both the class and the title basis are drawn"
        );
    }

    #[test]
    fn no_pool_means_no_deck() {
        let pools = Pools::new(Vec::new(), &BTreeSet::new());
        assert!(pools.is_empty());
        assert!(pools.deck(1, &mut ChaCha8Rng::seed_from_u64(0)).is_none());
    }
}
