//! How often each printing did something in a match, read from the seats' event feeds.

use alloc::collections::BTreeMap;
use serde::Serialize;
use sve_battle_contract::event::{EventBody, EventCard, EventDelivery};
use sve_battle_contract::{BattleUpdate, Seat, SourceRef, Zone};

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
fn counted(body: &EventBody) -> Option<(&str, Kind)> {
    if let EventBody::CardPlayed {
        card: Some(card), ..
    } = body
    {
        return Some((known(card), Kind::Played));
    }
    if let EventBody::AbilityPlayed {
        source: Some(source),
        ..
    } = body
    {
        return Some((text_printing(source), Kind::Ability));
    }
    if let EventBody::TriggerPending { source, .. } = body {
        return Some((text_printing(source), Kind::Trigger));
    }
    if let EventBody::Evolved {
        form: Some(card), ..
    } = body
    {
        return Some((known(card), Kind::Evolved));
    }
    if let EventBody::Movement { after, .. } = body
        && let Some(card) = &after.card
    {
        if after.zone == Some(Zone::Field) {
            return Some((known(card), Kind::Entered));
        }
        if after.zone == Some(Zone::Cemetery) {
            return Some((known(card), Kind::Cemetery));
        }
    }
    None
}

/// Each seat's own count by wire printing.
///
/// A public event reaches both seats and a private one only its owner, so the per-printing
/// maximum over the two feeds counts every event once.
#[derive(Debug, Default)]
pub struct Tally {
    seats: [BTreeMap<String, Uses>; 2],
}

impl Tally {
    /// Count the events one update delivers.
    pub fn update(&mut self, update: &BattleUpdate) {
        let EventDelivery::Available { batches, .. } = &update.events else {
            return;
        };
        let seat = match update.viewer {
            Seat::P1 => self.seats.first_mut(),
            Seat::P2 => self.seats.last_mut(),
        };
        let Some(seat) = seat else {
            return;
        };
        let mut count = |printing: &str, kind: Kind| {
            let uses = seat.entry(printing.to_owned()).or_default().field(kind);
            *uses = uses.saturating_add(1);
        };
        for event in batches.iter().flat_map(|batch| &batch.events) {
            if let Some((printing, kind)) = counted(&event.body) {
                count(printing, kind);
            }
        }
    }

    /// Both feeds merged, by wire printing.
    #[must_use]
    pub fn merged(self) -> BTreeMap<String, Uses> {
        let [mut first, second] = self.seats;
        for (printing, uses) in second {
            first.entry(printing).or_default().widest(&uses);
        }
        first
    }
}
