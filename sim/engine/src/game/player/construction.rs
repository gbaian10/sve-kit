//! Pure evaluation of source-checked conditional card construction restrictions.

use super::PlayerDeck;
use crate::Result;
use crate::catalog::SelectedEffects;
use crate::ir::lowering::{CardDeckRule, DeckRuleCondition, DeckSection};
use alloc::collections::{BTreeMap, BTreeSet};

/// Return the printing references exceeding an active card-level restriction.
///
/// Counts use construction names, combine printings, and keep sections independent.
/// Host validation still applies deck sizes, default limits and printed self-name exceptions.
/// This pure check can also serve a deck builder using the same selected snapshot.
///
/// # Errors
/// Rejects a changed selection or a referenced snapshot card without a construction name.
pub fn check_card_deck_rules(selected: &SelectedEffects, deck: &PlayerDeck) -> Result<Vec<String>> {
    selected.check_runtime()?;
    let main_ids = deck
        .main
        .iter()
        .filter_map(|number| selected.current.get(number).map(|card| &card.card_id))
        .collect::<BTreeSet<_>>();
    let mut violations = BTreeSet::new();
    for card in selected.current.values() {
        let Some(CardDeckRule::Conditional {
            when: DeckRuleCondition::SelfInMain,
            decks,
            exclude_name_contains,
            same_name_max,
        }) = &card.deck_rule
        else {
            continue;
        };
        if !main_ids.contains(&card.card_id) {
            continue;
        }
        for section in decks {
            let numbers = match section {
                DeckSection::Main => &deck.main,
                DeckSection::Evolve => &deck.evolve,
            };
            let mut names = BTreeMap::<String, Vec<&String>>::new();
            for number in numbers {
                if !selected.prototypes.cards.contains_key(number) {
                    continue;
                }
                let name = selected.prototypes.rules_label(number, 0)?;
                if !name.contains(exclude_name_contains) {
                    names.entry(name).or_default().push(number);
                }
            }
            for copies in names.values() {
                if usize::try_from(*same_name_max).is_ok_and(|maximum| copies.len() > maximum) {
                    violations.extend(copies.iter().map(|number| (*number).clone()));
                }
            }
        }
    }
    Ok(violations.into_iter().collect())
}
