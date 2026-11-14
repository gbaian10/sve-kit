//! Closed, read-only player observations without authority identities.

mod context;
pub use context::{
    AbilityQuota, DamageHistory, DiceHistory, EffectRecipient, HistoryEvent, PendingTrigger,
    PreventionChange, StatGainHistory, SubjectHistory, TriggerTiming, TurnHistory,
};

mod costs;
pub use costs::{
    CardCostQuote, CostGroup, CostGroupKind, CostLine, CostObservation, CostSummary, PendingCost,
    PlayCostQuote, PpQuote, QuoteBasis,
};

mod effects;
pub use effects::{
    EffectExpiry, EffectInterval, EffectSource, GrantedDefinition, InstalledChange,
    InstalledEffect, NumberChange,
};

use super::{ObjectHandle, Viewer};
use crate::game::{Game, Object, int};
use crate::ir::{EvolutionState, Phase, Posture, Stat};
use crate::{Result, invalid};

/// Player-visible zones, including attached evolution cards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservedZone {
    /// Main deck.
    Deck,
    /// Evolution deck.
    EvolveDeck,
    /// Hand.
    Hand,
    /// Field.
    Field,
    /// EX area.
    Ex,
    /// Cemetery.
    Cemetery,
    /// Banished cards.
    Banish,
    /// Resolving cards.
    Resolution,
    /// Evolution cards currently linked to field cards.
    Evolution,
    /// Race area.
    Race,
    /// Drive area.
    Drive,
    /// Trigger area.
    Trigger,
    /// Equipment area.
    Equipment,
}

/// Public turn information; pregame has no rules phase yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TurnObservation {
    /// Current turn player, when established.
    pub active: Option<Viewer>,
    /// Established first player.
    pub first_player: Option<Viewer>,
    /// Rules phase; absent during pregame.
    pub phase: Option<Phase>,
    /// Completed turn counts in seat order; absent neutral-position counts are zero.
    pub elapsed_turns: [u64; 2],
}

/// Public resources of one player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resources {
    /// Current leader life.
    pub life: i64,
    /// Available play points.
    pub pp: i64,
    /// Maximum play points.
    pub maximum_pp: i64,
    /// Available evolution points.
    pub ep: i64,
    /// Available super evolution points.
    pub sep: i64,
}

/// Effective follower stats from the existing numeric reader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stats {
    /// Effective attack.
    pub attack: i64,
    /// Remaining effective life.
    pub life: i64,
}

/// A named, currently authorized counter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Counter {
    /// Rules name or registered counter key.
    pub name: String,
    /// Current amount.
    pub amount: u64,
}

/// Explicit current state, omitted when only a hidden card's face is authorized.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardState {
    /// Current posture.
    pub posture: Posture,
    /// Current evolution state.
    pub evolution: EvolutionState,
    /// Effective field follower stats, if applicable.
    pub stats: Option<Stats>,
    /// Effective rules keywords.
    pub keywords: Vec<String>,
    /// Current named counters.
    pub counters: Vec<Counter>,
    /// Whether Box currently suppresses this card.
    pub boxed: bool,
    /// Authorized installed changes; effective properties above remain authoritative.
    pub effects: Vec<InstalledEffect>,
    /// Current limits; absent when this card cannot safely provide its budget.
    pub quotas: Option<Vec<AbilityQuota>>,
}

/// Authorized card characteristics, with no open authority state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardProperties {
    /// Public catalog identity, distinct from a physical object identity.
    pub card: String,
    /// Visible printed face index.
    pub face: u64,
    /// Current rules names.
    pub names: Vec<String>,
    /// Visible card kind.
    pub kind: String,
    /// Current rules traits.
    pub traits: Vec<String>,
    /// Owner seat.
    pub owner: Viewer,
    /// Current controller seat.
    pub controller: Viewer,
    /// Additional currently authorized state.
    pub state: Option<CardState>,
}

/// One currently identifiable object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardObservation {
    /// Reference for this viewer's uninterrupted identification period.
    pub handle: ObjectHandle,
    /// Physical printing and face, distinct from an attached evolve card's characteristics.
    pub physical: CardFace,
    /// Disclosed by a public zone or an active public revelation.
    pub public: bool,
    /// Whether the physical card is face up in its zone.
    pub face_up: bool,
    /// Explicit authorized characteristics.
    pub properties: CardProperties,
}

/// An authorized catalogue face, without a physical object identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardFace {
    /// Exact catalogue printing key.
    pub card: String,
    /// Physical face ordinal.
    pub face: u64,
}

/// A known member of a zone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZoneCard {
    /// Authorized offset; absent for an opponent's private unordered slots.
    pub position: Option<u64>,
    /// Visible card.
    pub card: CardObservation,
}

/// Zone size and its currently authorized members.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZoneObservation {
    /// Rules zone.
    pub kind: ObservedZone,
    /// Full size, including anonymous members.
    pub count: u64,
    /// Only the currently authorized members.
    pub cards: Vec<ZoneCard>,
}

/// One player's public resources and authorized zones.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerObservation {
    /// Player seat.
    pub player: Viewer,
    /// Public resource values.
    pub resources: Resources,
    /// Disclosed installed effects affecting this player.
    pub effects: Vec<InstalledEffect>,
    /// Zone counts and authorized cards.
    pub zones: Vec<ZoneObservation>,
}

/// Historical card memory without a link to a current object or hidden slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardMemory {
    /// Remembered public catalog identity.
    pub card: String,
    /// Face that was actually observed, without inferring the base face.
    pub face: u64,
    /// Remembered owner.
    pub owner: Viewer,
}

/// Observation v0; a Rust interface, without a stable wire format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    /// Authenticated perspective.
    pub viewer: Viewer,
    /// Current public turn state.
    pub turn: TurnObservation,
    /// Both players in seat order.
    pub players: [PlayerObservation; 2],
    /// Historical memory independent of current location knowledge.
    pub remembered: Vec<CardMemory>,
    /// Viewer-authorized estimates and unresolved play costs.
    pub costs: CostObservation,
    /// Disclosed pending occurrences, without question or authority identities.
    pub pending: Vec<PendingTrigger>,
    /// Committed public facts retained for this turn.
    pub history: TurnHistory,
}

pub(in crate::game) const fn seat(viewer: Viewer) -> &'static str {
    match viewer {
        Viewer::P1 => "P1",
        Viewer::P2 => "P2",
    }
}

pub(in crate::game) fn viewer(value: &str) -> Result<Viewer> {
    match value {
        "P1" => Ok(Viewer::P1),
        "P2" => Ok(Viewer::P2),
        _ => Err(invalid("invalid player seat")),
    }
}

pub(in crate::game) fn face_up(object: &Object) -> Result<bool> {
    match object.state.get("face_up") {
        Some(value) => value
            .as_bool()
            .ok_or_else(|| invalid("unknown card facing")),
        None => Ok(!matches!(
            object.zone.as_str(),
            "deck" | "hand" | "evolve_deck"
        )),
    }
}

pub(in crate::game) fn zone(value: &str) -> Result<ObservedZone> {
    match value {
        "deck" => Ok(ObservedZone::Deck),
        "hand" => Ok(ObservedZone::Hand),
        "field" => Ok(ObservedZone::Field),
        "ex" => Ok(ObservedZone::Ex),
        "cemetery" => Ok(ObservedZone::Cemetery),
        "banish" => Ok(ObservedZone::Banish),
        "evolve_deck" => Ok(ObservedZone::EvolveDeck),
        "evolution" => Ok(ObservedZone::Evolution),
        "resolution" => Ok(ObservedZone::Resolution),
        "race" => Ok(ObservedZone::Race),
        "drive" => Ok(ObservedZone::Drive),
        "trigger" => Ok(ObservedZone::Trigger),
        "equipment" => Ok(ObservedZone::Equipment),
        _ => Err(invalid("unknown player zone")),
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Player observations share authoritative readers."
)]
impl Game {
    /// Inspect authorized state without allocating handles or advancing the game.
    ///
    /// # Errors
    /// The authority cannot produce a complete, safe player observation.
    pub fn observe(&self, perspective: Viewer) -> Result<Observation> {
        self.player_observation(perspective)
            .map_err(|_authority_error| invalid("player observation unavailable"))
    }

    fn player_observation(&self, perspective: Viewer) -> Result<Observation> {
        if !self.state.continuous.is_empty() {
            return Err(invalid(
                "prototype continuous effects are not a typed observation",
            ));
        }
        let turn = &self.state.turn;
        let phase = match turn["phase"].as_str() {
            Some("start") => Some(Phase::Start),
            Some("main") => Some(Phase::Main),
            Some("end") => Some(Phase::End),
            Some("pregame") | None => None,
            Some(_) => return Err(invalid("unknown turn phase")),
        };
        let optional_seat = |key| turn[key].as_str().map(viewer).transpose();
        Ok(Observation {
            viewer: perspective,
            turn: TurnObservation {
                active: optional_seat("active")?,
                first_player: optional_seat("first_player")?,
                phase,
                elapsed_turns: [
                    turn["elapsed_turns"]["P1"].as_u64().unwrap_or_default(),
                    turn["elapsed_turns"]["P2"].as_u64().unwrap_or_default(),
                ],
            },
            players: [
                self.observed_player(perspective, Viewer::P1)?,
                self.observed_player(perspective, Viewer::P2)?,
            ],
            remembered: self.card_memory(perspective)?,
            costs: self.observed_costs(perspective)?,
            pending: self.observed_pending(perspective)?,
            history: self.observed_history(perspective)?,
        })
    }

    fn observed_player(&self, perspective: Viewer, player: Viewer) -> Result<PlayerObservation> {
        let data = self.player(seat(player))?;
        let mut zones = Vec::new();
        for (name, items) in &data.zones {
            if name == "void" {
                continue;
            }
            let mut cards = Vec::new();
            let mut offset = 0_u64;
            for item in items {
                if let Some(id) = item.as_str() {
                    if self.player_knows_object(perspective, id) {
                        cards.push(ZoneCard {
                            position: (name != "hand" || perspective == player).then_some(offset),
                            card: self.observed_card(perspective, id)?,
                        });
                    }
                    offset = offset
                        .checked_add(1)
                        .ok_or_else(|| invalid("zone count overflow"))?;
                } else {
                    offset = offset
                        .checked_add(
                            item["filler"]
                                .as_u64()
                                .ok_or_else(|| invalid("invalid anonymous zone member"))?,
                        )
                        .ok_or_else(|| invalid("zone count overflow"))?;
                }
            }
            if name == "hand" && perspective != player {
                cards.sort_by(|left, right| left.card.handle.cmp(&right.card.handle));
            }
            zones.push(ZoneObservation {
                kind: zone(name)?,
                count: offset,
                cards,
            });
        }
        Ok(PlayerObservation {
            player,
            resources: Resources {
                life: int(&data.leader["life"]),
                pp: int(&data.pp["current"]),
                maximum_pp: int(&data.pp["max"]),
                ep: data.ep,
                sep: data.sep,
            },
            effects: self.observed_player_effects(perspective, player)?,
            zones,
        })
    }

    fn card_memory(&self, perspective: Viewer) -> Result<Vec<CardMemory>> {
        self.state
            .knowledge
            .get(seat(perspective))
            .into_iter()
            .flat_map(|knowledge| &knowledge.faces)
            .map(|face| {
                Ok(CardMemory {
                    card: face.card.clone(),
                    face: face.face,
                    owner: viewer(&face.owner)?,
                })
            })
            .collect()
    }

    pub(in crate::game) fn player_knows_object(&self, perspective: Viewer, id: &str) -> bool {
        self.state
            .knowledge
            .get(seat(perspective))
            .is_some_and(|known| known.knows_location(id))
            && self
                .state
                .objects
                .get(id)
                .is_some_and(|object| object.zone != "void")
    }

    pub(in crate::game) fn observed_card(
        &self,
        perspective: Viewer,
        id: &str,
    ) -> Result<CardObservation> {
        let handle = self
            .state
            .knowledge
            .get(seat(perspective))
            .and_then(|knowledge| knowledge.handle(id))
            .ok_or_else(|| invalid("card is not identifiable"))?;
        self.resolve_player_object(perspective, &handle)?;
        let object = self.object(id)?;
        let face_up = face_up(object)?;
        Ok(CardObservation {
            handle,
            physical: self.physical_card_face(id)?,
            public: self.public_card_disclosure(id)?,
            face_up,
            properties: self.observed_card_properties(perspective, id)?,
        })
    }

    pub(in crate::game) fn physical_card_face(&self, id: &str) -> Result<CardFace> {
        let object = self.object(id)?;
        let face = if let Some(evolved) = object.state["evolved_with"].as_str() {
            self.object(evolved)?;
            // The base stores its evolve card's effective face; its physical front stays zero.
            0
        } else {
            object.state["face"]
                .as_u64()
                .ok_or_else(|| invalid("unknown physical card face"))?
        };
        self.catalog
            .face(&object.card, usize::try_from(face).map_err(invalid)?)?;
        Ok(CardFace {
            card: object.card.clone(),
            face,
        })
    }

    pub(in crate::game) fn observed_card_properties(
        &self,
        perspective: Viewer,
        id: &str,
    ) -> Result<CardProperties> {
        let object = self.object(id)?;
        let source = self.information_source(id)?;
        let public_state = !matches!(object.zone.as_str(), "hand" | "deck" | "evolve_deck")
            || (object.controller == seat(perspective) && object.zone != "deck");
        let state = public_state
            .then(|| self.observed_card_state(perspective, id, object))
            .transpose()?;
        Ok(CardProperties {
            card: source.card.clone(),
            face: source.state["face"]
                .as_u64()
                .ok_or_else(|| invalid("unknown effective card face"))?,
            names: self.card_names(id)?,
            kind: self.object_type(id)?.into(),
            traits: self.card_traits(id)?.into_iter().collect(),
            owner: viewer(&object.owner)?,
            controller: viewer(&object.controller)?,
            state,
        })
    }

    fn observed_card_state(
        &self,
        perspective: Viewer,
        id: &str,
        object: &Object,
    ) -> Result<CardState> {
        let mut counters = Vec::new();
        if let Some(values) = object.state["counters"].as_object() {
            for (key, amount) in values {
                let name = self.catalog.keyword_name(key);
                counters.push(Counter {
                    name: if name.is_empty() {
                        key.clone()
                    } else {
                        name.into()
                    },
                    amount: amount
                        .as_u64()
                        .ok_or_else(|| invalid("invalid card counter"))?,
                });
            }
        }
        let stats = if object.zone == "field" && self.object_type(id)?.contains("フォロワー") {
            Some(Stats {
                attack: self.internal_numeric_stat(id, Stat::Attack)?,
                life: self.internal_numeric_stat(id, Stat::Life)?,
            })
        } else {
            None
        };
        Ok(CardState {
            posture: if object.state["acted"] == true {
                Posture::Acted
            } else {
                Posture::Standing
            },
            evolution: if object.state["super_evolved"] == true {
                EvolutionState::SuperEvolved
            } else if object.state["evolved_with"].is_string() {
                EvolutionState::Evolved
            } else {
                EvolutionState::Base
            },
            stats,
            keywords: self
                .keywords(id)?
                .into_iter()
                .map(|key| {
                    let name = self.catalog.keyword_name(&key);
                    if name.is_empty() { key } else { name.into() }
                })
                .collect(),
            counters,
            boxed: self.internal_boxed(id),
            effects: self.observed_installed_effects(perspective, id)?,
            quotas: self.observed_quotas(perspective, id).ok(),
        })
    }
}
