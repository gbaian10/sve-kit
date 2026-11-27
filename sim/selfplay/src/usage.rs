//! How often each printing did something in a match, read from the seats' event feeds.

use alloc::collections::BTreeMap;
use serde::Serialize;
use sve_battle_contract::event::{EventBody, EventCard, EventDelivery};
use sve_battle_contract::{Seat, SourceRef, Zone};

/// Things a printing did, counted per event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Uses {
    /// Card plays established.
    pub played: u64,
    /// Ability plays established from its text.
    pub abilities: u64,
    /// Triggers from its text that became pending.
    pub triggers: u64,
    /// Moves onto a field.
    pub entered: u64,
    /// Moves into a cemetery.
    pub cemetery: u64,
    /// Evolutions that placed it.
    pub evolved: u64,
}

impl Uses {
    const fn field(&mut self, kind: Kind) -> &mut u64 {
        match kind {
            Kind::Played => &mut self.played,
            Kind::Ability => &mut self.abilities,
            Kind::Trigger => &mut self.triggers,
            Kind::Entered => &mut self.entered,
            Kind::Cemetery => &mut self.cemetery,
            Kind::Evolved => &mut self.evolved,
        }
    }

    /// Add another tally.
    pub const fn add(&mut self, other: &Self) {
        self.played = self.played.saturating_add(other.played);
        self.abilities = self.abilities.saturating_add(other.abilities);
        self.triggers = self.triggers.saturating_add(other.triggers);
        self.entered = self.entered.saturating_add(other.entered);
        self.cemetery = self.cemetery.saturating_add(other.cemetery);
        self.evolved = self.evolved.saturating_add(other.evolved);
    }

    fn widest(&mut self, other: &Self) {
        self.played = self.played.max(other.played);
        self.abilities = self.abilities.max(other.abilities);
        self.triggers = self.triggers.max(other.triggers);
        self.entered = self.entered.max(other.entered);
        self.cemetery = self.cemetery.max(other.cemetery);
        self.evolved = self.evolved.max(other.evolved);
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Played,
    Ability,
    Trigger,
    Entered,
    Cemetery,
    Evolved,
}

const fn text_printing(source: &SourceRef) -> &str {
    match source {
        SourceRef::Text { text } => text.card.printing_id.as_str(),
        SourceRef::Card { printing_id, .. } => printing_id.as_str(),
    }
}

const fn known(card: &EventCard) -> &str {
    card.card.printing_id.as_str()
}

/// The printing an event counts for, and as what.
fn counted(body: &EventBody) -> Option<(&str, Kind, Option<Seat>)> {
    if let EventBody::CardPlayed {
        card: Some(card),
        player,
    } = body
    {
        return Some((known(card), Kind::Played, Some(*player)));
    }
    if let EventBody::AbilityPlayed {
        source: Some(source),
        player,
        ..
    } = body
    {
        return Some((text_printing(source), Kind::Ability, Some(*player)));
    }
    if let EventBody::TriggerPending {
        source, controller, ..
    } = body
    {
        return Some((text_printing(source), Kind::Trigger, Some(*controller)));
    }
    if let EventBody::Evolved {
        form: Some(card), ..
    } = body
    {
        return Some((known(card), Kind::Evolved, None));
    }
    if let EventBody::Movement { after, .. } = body
        && let Some(card) = &after.card
    {
        if after.zone == Some(Zone::Field) {
            return Some((known(card), Kind::Entered, Some(after.player)));
        }
        if after.zone == Some(Zone::Cemetery) {
            return Some((known(card), Kind::Cemetery, Some(after.player)));
        }
    }
    None
}

/// Each viewer's counts by wire printing and playing or destination seat, if supplied.
#[derive(Debug, Default)]
pub struct Tally {
    first: BTreeMap<(String, Option<Seat>), Uses>,
    second: BTreeMap<(String, Option<Seat>), Uses>,
}

impl Tally {
    /// Count the events one update delivers.
    pub fn update(&mut self, viewer: Seat, events: &EventDelivery) {
        let EventDelivery::Available { batches, .. } = events else {
            return;
        };
        let seat = match viewer {
            Seat::P1 => &mut self.first,
            Seat::P2 => &mut self.second,
        };
        for event in batches.iter().flat_map(|batch| &batch.events) {
            if let Some((printing, kind, player)) = counted(&event.body) {
                let count = seat
                    .entry((printing.to_owned(), player))
                    .or_default()
                    .field(kind);
                *count = count.saturating_add(1);
            }
        }
    }

    /// Both feeds merged, by wire printing.
    #[must_use]
    pub fn merged(self) -> BTreeMap<String, Uses> {
        let Self { mut first, second } = self;
        for (key, uses) in second {
            first.entry(key).or_default().widest(&uses);
        }
        // The two private Start Amulet placements in a mirror have different destinations.
        let mut by_printing = BTreeMap::<String, Uses>::new();
        for ((printing, _), uses) in first {
            by_printing.entry(printing).or_default().add(&uses);
        }
        by_printing
    }
}

#[cfg(test)]
mod tests {
    use super::{Tally, Uses};
    use serde_json::{Value, json};
    use sve_battle_contract::{Seat, event::EventDelivery};

    fn feed(viewer: Seat) -> EventDelivery {
        let events = [(Seat::P1, "e1"), (Seat::P2, "e2"), (Seat::P1, "e3")]
            .into_iter()
            .map(|(player, id)| {
                let card = if player == viewer || id == "e3" {
                    json!({"card": {"printing_id": "test-amulet", "face": 0_u8}, "object": null})
                } else {
                    Value::Null
                };
                json!({
                    "id": id, "cause": "c", "parent": null, "about": [], "presentation_id": null,
                    "body": {
                        "kind": "movement", "method": "put", "by_effect": false,
                        "before": {"player": player, "zone": "deck", "position": null, "card": null},
                        "after": {"player": player, "zone": "field", "position": null, "card": card}
                    }
                })
            })
            .collect::<Vec<_>>();
        serde_json::from_value(json!({
            "status": "available", "batches": [{"id": "b", "events": events}], "animate": []
        }))
        .unwrap()
    }

    #[test]
    fn private_placements_on_both_sides_count_alongside_shared_events() {
        let mut tally = Tally::default();
        for viewer in [Seat::P1, Seat::P2] {
            tally.update(viewer, &feed(viewer));
        }
        assert_eq!(
            tally.merged().get("test-amulet"),
            Some(&Uses {
                entered: 3,
                ..Uses::default()
            })
        );
    }
}
