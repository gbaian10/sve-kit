//! Invariants every stable stop must satisfy, read from the two seats' wire updates alone.

use alloc::collections::{BTreeMap, BTreeSet};
use serde_json::Value;
use sve_battle_contract::observation::{BattleObservation, Knowledge, PlayerView, ZoneView, Zones};
use sve_battle_contract::question::QuestionPurpose;
use sve_battle_contract::{BattleInput, BattleUpdate, CardRef, Seat};

/// A broken invariant: a stable name for grouping and the detail for a person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    /// Which invariant.
    pub name: &'static str,
    /// What was observed.
    pub detail: String,
}

const fn violation(name: &'static str, detail: String) -> Violation {
    Violation { name, detail }
}

const fn index(seat: Seat) -> usize {
    match seat {
        Seat::P1 => 0,
        Seat::P2 => 1,
    }
}

const fn seat_name(seat: Seat) -> &'static str {
    match seat {
        Seat::P1 => "p1",
        Seat::P2 => "p2",
    }
}

const fn other(seat: Seat) -> Seat {
    match seat {
        Seat::P1 => Seat::P2,
        Seat::P2 => Seat::P1,
    }
}

const fn zones(zones: &Zones) -> [(&'static str, &ZoneView); 14] {
    [
        ("deck", &zones.deck),
        ("evolve_deck", &zones.evolve_deck),
        ("hand", &zones.hand),
        ("field", &zones.field),
        ("ex", &zones.ex),
        ("cemetery", &zones.cemetery),
        ("banish", &zones.banish),
        ("resolution", &zones.resolution),
        ("evolution", &zones.evolution),
        ("race", &zones.race),
        ("drive", &zones.drive),
        ("trigger", &zones.trigger),
        ("equipment", &zones.equipment),
        ("limit_break", &zones.limit_break),
    ]
}

/// Every `printing_id` string anywhere in a serialized message.
pub(crate) fn printings(value: &Value, found: &mut BTreeSet<String>) {
    match value {
        Value::Object(fields) => {
            for (key, field) in fields {
                if key == "printing_id"
                    && let Some(id) = field.as_str()
                {
                    found.insert(id.to_owned());
                } else {
                    printings(field, found);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                printings(item, found);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

/// What one seat's latest update says, reduced to what the cross-seat checks compare.
#[derive(Debug, Default)]
struct Seen {
    mentioned: BTreeSet<String>,
    public: BTreeSet<String>,
    noticed: BTreeSet<String>,
    /// Printings named by the committed events of this update.
    events: BTreeSet<String>,
    /// Whether this update arrived since the previous stop.
    fresh: bool,
}

/// Checks across a whole match; it remembers what each seat has legitimately been shown.
#[derive(Debug)]
pub struct Checker {
    /// Printings only the other seat's deck contains: this seat must be shown them first.
    sensitive: [BTreeSet<String>; 2],
    /// Printings of deck cards; any other object was generated during the match.
    deck_printings: BTreeSet<String>,
    deck_cards: usize,
    exposed: [BTreeSet<String>; 2],
    seen: [Seen; 2],
    sequence: [Option<u64>; 2],
    /// Deck cards counted at the previous stop; cards may leave the game but never appear.
    counted: Option<usize>,
}

impl Checker {
    /// `decks` are the wire printing identifiers of every card of each seat's decks.
    #[must_use]
    pub fn new(decks: &[Vec<String>; 2]) -> Self {
        let [first, second] = decks;
        let own = |deck: &Vec<String>| deck.iter().cloned().collect::<BTreeSet<_>>();
        let (first_set, second_set) = (own(first), own(second));
        Self {
            sensitive: [
                second_set.difference(&first_set).cloned().collect(),
                first_set.difference(&second_set).cloned().collect(),
            ],
            deck_printings: first_set.union(&second_set).cloned().collect(),
            deck_cards: first.len().saturating_add(second.len()),
            exposed: [BTreeSet::new(), BTreeSet::new()],
            seen: [Seen::default(), Seen::default()],
            sequence: [None, None],
            counted: None,
        }
    }

    /// Record one delivered update: its sequence must grow and its own structure must hold.
    ///
    /// # Errors
    /// The update repeats a sequence number or contradicts itself.
    pub fn update(&mut self, update: &BattleUpdate, json: &Value) -> Result<(), Violation> {
        let seat = index(update.viewer);
        let sequence = update.update_seq.get();
        if let Some(slot) = self.sequence.get_mut(seat) {
            if slot.is_some_and(|previous| sequence < previous) {
                return Err(violation(
                    "update_sequence",
                    format!("update {sequence} follows a later one"),
                ));
            }
            *slot = Some(sequence);
        }
        structure(&update.observation)?;
        if let BattleInput::Question { question } = &update.input
            && !question.is_coherent()
        {
            return Err(violation(
                "question_shape",
                "question shape contradicts its use".into(),
            ));
        }
        let mut seen = Seen::default();
        printings(json, &mut seen.mentioned);
        if let Some(notices) = json.get("notices") {
            printings(notices, &mut seen.noticed);
        }
        if let Some(events) = json.get("events") {
            printings(events, &mut seen.events);
        }
        seen.fresh = true;
        seen.public = update
            .observation
            .objects
            .values()
            .filter(|object| object.knowledge == Knowledge::Public)
            .flat_map(|object| [&object.card, &object.effective_face])
            .map(|card| card.printing_id.clone())
            .collect();
        if let Some(slot) = self.seen.get_mut(seat) {
            *slot = seen;
        }
        Ok(())
    }

    /// Compare the two seats' current updates at one stable stop.
    ///
    /// # Errors
    /// The seats disagree on public facts, cards were created from nothing, or a seat was told
    /// a card of the other deck that it was never shown.
    pub fn stop(&mut self, latest: &[Option<BattleUpdate>; 2]) -> Result<(), Violation> {
        let [Some(first), Some(second)] = latest else {
            return Err(violation("routing", "a seat has no update".into()));
        };
        routing(first, second)?;
        if settled(first, second) {
            agreement(&first.observation, &second.observation)?;
        }
        self.conservation(&first.observation)?;
        self.conservation(&second.observation)?;
        // An event both seats were told at this stop names cards both seats saw.
        let [one, two] = &mut self.seen;
        let shared: Vec<String> = if one.fresh && two.fresh {
            one.events.intersection(&two.events).cloned().collect()
        } else {
            Vec::new()
        };
        one.fresh = false;
        two.fresh = false;
        for exposed in &mut self.exposed {
            exposed.extend(shared.iter().cloned());
        }
        for viewer in [Seat::P1, Seat::P2] {
            let (mine, theirs) = (index(viewer), index(other(viewer)));
            let (Some(own), Some(opponent)) = (self.seen.get(mine), self.seen.get(theirs)) else {
                continue;
            };
            let shown: Vec<String> = opponent
                .public
                .iter()
                .chain(&own.noticed)
                .cloned()
                .collect();
            let Some(exposed) = self.exposed.get_mut(mine) else {
                continue;
            };
            exposed.extend(shown);
            let leaked: Vec<&str> = self
                .sensitive
                .get(mine)
                .into_iter()
                .flatten()
                .filter(|printing| {
                    own.mentioned.contains(*printing) && !exposed.contains(*printing)
                })
                .map(String::as_str)
                .collect();
            if !leaked.is_empty() {
                return Err(violation(
                    "hidden_information",
                    format!(
                        "{} was told unexposed printings {}",
                        seat_name(viewer),
                        leaked.join(" ")
                    ),
                ));
            }
        }
        Ok(())
    }

    /// Deck cards this view accounts for: every zone count minus the generated objects.
    fn deck_cards(&self, observation: &BattleObservation) -> usize {
        let total = observation
            .players
            .iter()
            .flat_map(|player| zones(&player.zones))
            .fold(0_usize, |sum, (_, zone)| {
                sum.saturating_add(usize::try_from(zone.count.get()).unwrap_or(usize::MAX))
            });
        let generated = observation
            .players
            .iter()
            .flat_map(|player| zones(&player.zones))
            .flat_map(|(_, zone)| &zone.cards)
            .filter(|member| {
                observation
                    .objects
                    .get(&member.object)
                    .is_some_and(|object| !self.deck_printings.contains(&object.card.printing_id))
            })
            .count();
        total.saturating_sub(generated)
    }

    fn conservation(&self, observation: &BattleObservation) -> Result<(), Violation> {
        let cards = self.deck_cards(observation);
        let limit = self.counted.unwrap_or(self.deck_cards);
        if cards > limit {
            return Err(violation(
                "card_conservation",
                format!(
                    "{} counts {cards} deck cards where at most {limit} exist",
                    seat_name(observation.viewer)
                ),
            ));
        }
        Ok(())
    }

    /// Lower the conservation ceiling once the seats agree that cards left the game.
    pub fn settle(&mut self, latest: &[Option<BattleUpdate>; 2]) {
        if let Some(most) = latest
            .iter()
            .flatten()
            .map(|update| self.deck_cards(&update.observation))
            .max()
        {
            self.counted = Some(most);
        }
    }
}

/// Whether both seats must see the same public board: while a seat is still declaring a play,
/// its own view runs ahead of what the other seat has been told.
fn settled(first: &BattleUpdate, second: &BattleUpdate) -> bool {
    [first, second]
        .into_iter()
        .all(|update| match &update.input {
            BattleInput::Question { question } => match question.purpose {
                QuestionPurpose::MainAction
                | QuestionPurpose::QuickWindow
                | QuestionPurpose::ResolutionChoice => true,
                QuestionPurpose::PlayParameter | QuestionPurpose::RuleChoice => false,
            },
            BattleInput::Waiting { .. }
            | BattleInput::Finished { .. }
            | BattleInput::EngineFault { .. } => true,
        })
}

fn routing(first: &BattleUpdate, second: &BattleUpdate) -> Result<(), Violation> {
    let consistent = match (&first.input, &second.input) {
        (BattleInput::Question { question }, BattleInput::Waiting { actor, .. }) => {
            question.actor == Seat::P1 && *actor == Seat::P1
        }
        (BattleInput::Waiting { actor, .. }, BattleInput::Question { question }) => {
            question.actor == Seat::P2 && *actor == Seat::P2
        }
        (
            BattleInput::Finished { winner, reason },
            BattleInput::Finished {
                winner: other_winner,
                reason: other_reason,
            },
        ) => winner == other_winner && reason == other_reason,
        (BattleInput::EngineFault { .. }, BattleInput::EngineFault { .. }) => true,
        (
            BattleInput::Question { .. }
            | BattleInput::Waiting { .. }
            | BattleInput::Finished { .. }
            | BattleInput::EngineFault { .. },
            _,
        ) => false,
    };
    if consistent {
        Ok(())
    } else {
        Err(violation(
            "routing",
            "the seats disagree on who must act or how the game ended".into(),
        ))
    }
}

fn structure(observation: &BattleObservation) -> Result<(), Violation> {
    let mut placed = BTreeSet::new();
    for player in &observation.players {
        for (name, zone) in zones(&player.zones) {
            let known = zone.cards.len();
            let size = usize::try_from(zone.count.get()).unwrap_or(usize::MAX);
            if known > size {
                return Err(violation(
                    "zone_structure",
                    format!("{name} lists {known} members of {size}"),
                ));
            }
            if zone
                .capacity
                .is_some_and(|capacity| zone.count.get() > capacity.get())
            {
                return Err(violation(
                    "zone_capacity",
                    format!("{name} holds {size} cards over its capacity"),
                ));
            }
            for member in &zone.cards {
                if !observation.objects.contains_key(&member.object) {
                    return Err(violation(
                        "zone_structure",
                        format!("{name} lists an object missing from the table"),
                    ));
                }
                if !placed.insert(&member.object) {
                    return Err(violation(
                        "zone_structure",
                        format!("an object is listed twice, the second time in {name}"),
                    ));
                }
            }
        }
    }
    Ok(())
}

type PublicCards = BTreeMap<(usize, &'static str), Vec<CardRef>>;

fn public_cards(observation: &BattleObservation) -> PublicCards {
    let mut cards = PublicCards::new();
    for (seat, player) in observation.players.iter().enumerate() {
        for (name, zone) in zones(&player.zones) {
            let mut members: Vec<_> = zone
                .cards
                .iter()
                .filter_map(|member| {
                    let object = observation.objects.get(&member.object)?;
                    // Positions are compared nowhere: a hidden zone gives them to its owner only.
                    (object.knowledge == Knowledge::Public).then(|| object.card.clone())
                })
                .collect();
            members.sort();
            cards.insert((seat, name), members);
        }
    }
    cards
}

fn numbers(values: &[i64]) -> String {
    values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("/")
}

fn resources(player: &PlayerView) -> [i64; 6] {
    [
        player.life.get(),
        player.pp.get(),
        player.max_pp.get(),
        player.ep.get(),
        player.sep.get(),
        i64::try_from(player.ep_capacity.get()).unwrap_or(i64::MAX),
    ]
}

fn agreement(first: &BattleObservation, second: &BattleObservation) -> Result<(), Violation> {
    if first.turn != second.turn {
        return Err(violation(
            "view_agreement",
            "the seats see different turn states".into(),
        ));
    }
    for (mine, theirs) in first.players.iter().zip(&second.players) {
        if resources(mine) != resources(theirs) {
            return Err(violation(
                "view_agreement",
                format!(
                    "the seats see different resources for {}: {} and {}",
                    seat_name(mine.seat),
                    numbers(&resources(mine)),
                    numbers(&resources(theirs))
                ),
            ));
        }
        for ((name, zone), (_, same)) in zones(&mine.zones).into_iter().zip(zones(&theirs.zones)) {
            if zone.count != same.count || zone.capacity != same.capacity {
                return Err(violation(
                    "view_agreement",
                    format!(
                        "the seats count {}'s {name} differently: {} and {}",
                        seat_name(mine.seat),
                        zone.count.get(),
                        same.count.get()
                    ),
                ));
            }
        }
    }
    let (mine, theirs) = (public_cards(first), public_cards(second));
    if let Some(((seat, name), _)) = mine
        .iter()
        .find(|(key, members)| theirs.get(*key) != Some(*members))
    {
        return Err(violation(
            "view_agreement",
            format!("the seats list different public cards in player {seat}'s {name}"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
