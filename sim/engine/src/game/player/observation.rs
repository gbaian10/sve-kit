//! Closed, read-only player observations without authority identities.

mod context;
mod input;
mod processing;
pub use context::{
    AbilityQuota, DamageHistory, DiceHistory, EffectRecipient, HistoryEvent, PendingTrigger,
    PreventionChange, StatGainHistory, SubjectHistory, TriggerTiming, TurnHistory,
};
pub use processing::{
    PaymentKind, PaymentPurpose, ProcedureKind, ProcessingContext, ProcessingDeclaration,
    ProcessingPayment, ProcessingProcedure, ProcessingStage, ProcessingSubject,
};

mod board;
pub use board::{AttackObservation, Relationship, RelationshipKind};

mod costs;
pub use costs::{
    CardCostQuote, CostGroup, CostGroupKind, CostLine, CostObservation, CostSummary, PendingCost,
    PlayCostQuote, PpQuote, QuoteBasis,
};

mod effects;
pub use effects::{
    EffectExpiry, EffectInterval, EffectSource, GrantedDefinition, InstalledChange,
    InstalledEffect, NumberChange, TextProvenance, WordingField, WordingSource,
};

use super::{
    DecisionShape, EvolutionQuestion, Input, ObjectHandle, OpeningQuestion, QuestionShape, Viewer,
};
use crate::battle::{
    BattleReferences, ObjectTable, ProjectionError, QuestionProjection, RuleProcessing,
    build_objects, build_observation, project_turn,
};
use crate::game::internal::{observed_processing, public_number_declaration, zone_capacity};
use crate::game::{Game, Object};
use crate::ir::{EvolutionState, Phase, Posture, Stat};
use crate::{Result, invalid};
use core::result;
use serde_json::Value;
use sve_battle_contract::{
    QuestionId, Seat,
    observation::{self as wire, LimitBreakStage, TurnView},
};

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
    /// Public area for attached limit-break materials.
    LimitBreak,
}

impl ObservedZone {
    pub(crate) const fn supports_public_history_link(self) -> bool {
        match self {
            Self::Deck | Self::EvolveDeck | Self::Hand => false,
            Self::Field
            | Self::Ex
            | Self::Cemetery
            | Self::Banish
            | Self::Resolution
            | Self::Evolution
            | Self::Race
            | Self::Drive
            | Self::Trigger
            | Self::Equipment
            | Self::LimitBreak => true,
        }
    }
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
    /// Turns begun in seat order; absent neutral-position counts are zero.
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
    /// Retained EP slots, expanded by increases beyond the existing pool.
    pub ep_capacity: i64,
    /// Depleted EP slots; recovering EP reduces this value.
    pub ep_used: i64,
    /// Available super evolution points.
    pub sep: i64,
    /// Owner's turn on which allocated SEP unlocks; retained after spending.
    pub sep_available_from: Option<u64>,
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
    /// Current limit-break stage, public to both seats.
    pub limit_break: LimitBreakStage,
    /// Effective field or EX follower stats, if applicable.
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
    /// Rules capacity, absent for zones without a fixed limit.
    pub capacity: Option<u64>,
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
    /// Current attack, absent once its frozen participants are no longer valid.
    pub attack: Option<AttackObservation>,
    /// Both players in seat order.
    pub players: [PlayerObservation; 2],
    /// Current attachments whose two endpoints are identifiable to this viewer.
    pub relationships: Vec<Relationship>,
    /// Historical memory independent of current location knowledge.
    pub remembered: Vec<CardMemory>,
    /// Viewer-authorized estimates and unresolved play costs.
    pub costs: CostObservation,
    /// Disclosed pending occurrences, without question or authority identities.
    pub pending: Vec<PendingTrigger>,
    /// Authorized formal executions, ordered from outer to inner.
    /// Hidden frames leave no slot; private progress is visible only to the current actor.
    pub processing: Vec<ProcessingContext>,
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
    object.state.get("face_up").map_or(
        Ok(!matches!(
            object.zone.as_str(),
            "deck" | "hand" | "evolve_deck"
        )),
        |value| {
            value
                .as_bool()
                .ok_or_else(|| invalid("unknown card facing"))
        },
    )
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
        "limit_break" => Ok(ObservedZone::LimitBreak),
        _ => Err(invalid("unknown player zone")),
    }
}

fn public_resource(value: &Value) -> Result<i64> {
    value
        .as_i64()
        .ok_or_else(|| invalid("invalid public resource"))
}

const fn rule_stage(shape: &QuestionShape) -> Option<wire::ProcessingStage> {
    let stage = match shape {
        QuestionShape::Opening(_) => wire::ProcessingStage::Opening,
        QuestionShape::Closing(_) => wire::ProcessingStage::Closing,
        // No main action or evolution parameter commits payment; private cost shapes cannot change the stage.
        QuestionShape::ActionWindow(_)
        | QuestionShape::Evolution(
            EvolutionQuestion::Form(_)
            | EvolutionQuestion::Materials(_)
            | EvolutionQuestion::Discard(_)
            | EvolutionQuestion::Payment(_),
        ) => wire::ProcessingStage::Declare,
        QuestionShape::Decision(shape) => {
            if matches!(shape, DecisionShape::Quick { .. }) {
                wire::ProcessingStage::Quick
            } else if matches!(shape, DecisionShape::Pending(_)) {
                wire::ProcessingStage::PendingOrder
            } else if matches!(shape, DecisionShape::TriggerQuota(_)) {
                wire::ProcessingStage::TriggerAdmission
            } else {
                return None;
            }
        }
    };
    Some(stage)
}

impl Game {
    /// Inspect authorized state without allocating handles or advancing the game.
    ///
    /// # Errors
    /// The authority cannot produce a complete, safe player observation.
    pub fn observe(&self, perspective: Viewer) -> Result<Observation> {
        self.observed_from(perspective)
            .player_observation(perspective)
            .map_err(|_authority_error| invalid("player observation unavailable"))
    }

    /// The state a seat may see: other seats see an unplayed declaration's origin, since it can
    /// still be withdrawn without a trace.
    fn observed_from(&self, perspective: Viewer) -> &Self {
        match self.concealed_declaration() {
            Some((origin, controller)) if controller != perspective.player() => origin,
            Some(_) | None => self,
        }
    }

    /// Project all authorized board components from one read and object scope.
    ///
    /// The host authenticates the viewer and binds this value to the same stop as
    /// its question. Unsupported active processing refuses the entire observation.
    /// Hosts must preflight the full program closure before interactive use:
    /// a private branch must not decide whether a required stop is supported.
    ///
    /// # Errors
    /// Refuses unavailable facts, unsupported processing, mismatched references,
    /// invalid object links and numbers outside the exact wire range.
    pub fn battle_observation(
        &self,
        perspective: Viewer,
        references: &BattleReferences,
    ) -> result::Result<wire::BattleObservation, ProjectionError> {
        let observed = self.observed_from(perspective);
        let observation = observed
            .observe(perspective)
            .map_err(|_authority_error| ProjectionError::Unavailable)?;
        let processing = observed.battle_rule_processing(&observation)?;
        build_observation(&self.catalog, references, &observation, processing)
    }

    /// Bind this seat's current question and complete board to a host-issued ID.
    ///
    /// The host authenticates the viewer, issues a fresh ID per occurrence in this
    /// battle, and submits decoded commands to this same Game. Waiting and finished
    /// seats have no question; this method does not replace their public input summary.
    /// Supports all admitted player questions, including main-phase intents.
    /// The host must preflight the full program closure before interactive use.
    ///
    /// # Errors
    /// Refuses a fault, unsupported active shape, unavailable board or inconsistent
    /// candidate membership. No partial question or guessed rules reference is returned.
    pub fn battle_question(
        &self,
        perspective: Viewer,
        id: &QuestionId,
        references: &BattleReferences,
    ) -> result::Result<Option<QuestionProjection>, ProjectionError> {
        references.validate_catalog(&self.catalog)?;
        match self.input(perspective) {
            Input::Question(question) => self
                .project_battle_question(perspective, id, *question, references)
                .map(Some),
            Input::Waiting(_) | Input::Finished { .. } => Ok(None),
            Input::EngineFault => Err(ProjectionError::Unavailable),
        }
    }

    fn project_battle_question(
        &self,
        perspective: Viewer,
        id: &QuestionId,
        question: super::Question,
        references: &BattleReferences,
    ) -> result::Result<QuestionProjection, ProjectionError> {
        let native = self
            .observe(perspective)
            .map_err(|_error| ProjectionError::Unavailable)?;
        let rule_processing = self.battle_rule_processing(&native)?;
        let current = if rule_processing.is_empty() {
            let mut active = native
                .processing
                .iter()
                .enumerate()
                .filter(|(_, context)| context.actor == Some(question.actor));
            let current = active.next().map(|(index, _)| index);
            if active.next().is_some() {
                return Err(ProjectionError::Unavailable);
            }
            current
        } else {
            Some(
                native
                    .processing
                    .len()
                    .checked_add(rule_processing.len())
                    .and_then(|length| length.checked_sub(1))
                    .ok_or(ProjectionError::Unavailable)?,
            )
        };
        let observation = build_observation(&self.catalog, references, &native, rule_processing)?;
        let public_domain = match &question.shape {
            QuestionShape::Decision(DecisionShape::Integer {
                source: super::IntegerSource::NumberDeclaration,
                ..
            }) => public_number_declaration(self),
            QuestionShape::Decision(DecisionShape::Quick { .. }) => {
                self.publicly_empty_quick(perspective)
            }
            QuestionShape::Opening(_)
            | QuestionShape::Closing(_)
            | QuestionShape::Evolution(_)
            | QuestionShape::Decision(_)
            | QuestionShape::ActionWindow(_) => false,
        };
        QuestionProjection::project(
            id,
            question,
            observation,
            current,
            public_domain,
            &self.catalog,
            references,
        )
    }

    fn battle_rule_processing(
        &self,
        observation: &Observation,
    ) -> result::Result<Vec<RuleProcessing>, ProjectionError> {
        let question = match self.input(observation.viewer) {
            Input::Question(question) => question,
            Input::Waiting(waiting) => {
                let Input::Question(question) = self.input(waiting.actor) else {
                    return Err(ProjectionError::Unavailable);
                };
                if question.actor != waiting.actor {
                    return Err(ProjectionError::Unavailable);
                }
                question
            }
            Input::Finished { .. } => return Ok(vec![]),
            Input::EngineFault => return Err(ProjectionError::Unavailable),
        };
        let Some(stage) = rule_stage(&question.shape) else {
            return Ok(vec![]);
        };
        let occurrences = matches!(
            stage,
            wire::ProcessingStage::PendingOrder | wire::ProcessingStage::TriggerAdmission
        );
        // A hidden admission/order leaves no context slot for the other seat.
        if occurrences && observation.viewer != question.actor {
            let visible = if stage == wire::ProcessingStage::TriggerAdmission {
                self.observed_admission_group(observation.viewer)
                    .map_err(|_authority_error| ProjectionError::Unavailable)?
            } else {
                observation
                    .pending
                    .iter()
                    .any(|pending| pending.controller == question.actor)
            };
            if !visible {
                return Ok(vec![]);
            }
        }
        let actor_seat = match question.actor {
            Viewer::P1 => Seat::P1,
            Viewer::P2 => Seat::P2,
        };
        let controller = (occurrences
            || matches!(
                &question.shape,
                QuestionShape::Evolution(_) | QuestionShape::ActionWindow(_)
            ))
        .then_some(actor_seat);
        let procedure = matches!(
            &question.shape,
            QuestionShape::Opening(OpeningQuestion::Mulligan(_))
        )
        .then_some(wire::Procedure {
            kind: wire::ProcedureKind::EachPlayer,
            actor: actor_seat,
            round: None,
        });
        // An evolution declaration can still be withdrawn: other seats see the action window.
        let subject = (matches!(&question.shape, QuestionShape::Evolution(_))
            && question.actor == observation.viewer)
            .then(|| self.evolution_subject(observation.viewer))
            .transpose()
            .map_err(|_authority_error| ProjectionError::Unavailable)?;
        Ok(vec![RuleProcessing {
            actor: (question.actor == observation.viewer).then_some(actor_seat),
            stage,
            controller,
            subject,
            procedure,
        }])
    }

    /// Project this viewer's current wire object table from one authority read.
    ///
    /// The host authenticates the viewer and binds the returned table to this stop.
    /// Event-time faces must not be reconstructed from the current table.
    ///
    /// # Errors
    /// Refuses unavailable authority facts, mismatched snapshot references,
    /// unresolved faces or rule names, and inexact wire numbers.
    pub fn battle_objects(
        &self,
        perspective: Viewer,
        references: &BattleReferences,
    ) -> result::Result<ObjectTable, ProjectionError> {
        let observation = self
            .observe(perspective)
            .map_err(|_authority_error| ProjectionError::Unavailable)?;
        build_objects(&self.catalog, references, &observation)
    }

    /// Project this viewer's current public turn without advancing the authority.
    ///
    /// The host binds the authenticated viewer to this stop. Event-time turn facts
    /// must not be reconstructed from this current view.
    ///
    /// # Errors
    /// Refuses unavailable authority facts and turn counts outside the wire range.
    pub fn battle_turn(&self, perspective: Viewer) -> result::Result<TurnView, ProjectionError> {
        let observation = self
            .observe(perspective)
            .map_err(|_authority_error| ProjectionError::Unavailable)?;
        project_turn(&observation.turn)
    }

    fn player_observation(&self, perspective: Viewer) -> Result<Observation> {
        if !self.state.continuous.is_empty() {
            return Err(invalid(
                "prototype continuous effects are not a typed observation",
            ));
        }
        let turn = &self.state.turn;
        if !turn.is_object() {
            return Err(invalid("invalid turn state"));
        }
        let phase = match turn["phase"].as_str() {
            Some("start") => Some(Phase::Start),
            Some("main") => Some(Phase::Main),
            Some("end") => Some(Phase::End),
            Some("pregame") => None,
            None if turn["phase"].is_null() => None,
            Some(_) | None => return Err(invalid("unknown turn phase")),
        };
        let optional_seat = |key: &str| {
            turn.get(key)
                .filter(|value| !value.is_null())
                .map(|value| {
                    value
                        .as_str()
                        .ok_or_else(|| invalid("invalid turn seat"))
                        .and_then(viewer)
                })
                .transpose()
        };
        let elapsed_turns = |player: &str| {
            let Some(counts) = turn.get("elapsed_turns") else {
                return Ok(0);
            };
            let counts = counts
                .as_object()
                .ok_or_else(|| invalid("invalid elapsed turn counts"))?;
            counts.get(player).map_or(Ok(0), |count| {
                count
                    .as_u64()
                    .ok_or_else(|| invalid("invalid elapsed turn count"))
            })
        };
        let first_player = optional_seat("first_player")?;
        let active = optional_seat("active")?;
        // Opening keeps an authority-only placeholder before the first-player choice.
        let active = if turn["phase"] == "pregame" && first_player.is_none() {
            None
        } else {
            active
        };
        Ok(Observation {
            viewer: perspective,
            turn: TurnObservation {
                active,
                first_player,
                phase,
                elapsed_turns: [elapsed_turns("P1")?, elapsed_turns("P2")?],
            },
            attack: self.observed_attack(perspective)?,
            players: [
                self.observed_player(perspective, Viewer::P1)?,
                self.observed_player(perspective, Viewer::P2)?,
            ],
            relationships: self.observed_relationships(perspective)?,
            remembered: self.card_memory(perspective)?,
            costs: self.observed_costs(perspective)?,
            pending: self.observed_pending(perspective)?,
            processing: observed_processing(self, perspective)?,
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
                            position: (name != "limit_break"
                                && (name != "hand" || perspective == player))
                                .then_some(offset),
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
            if name == "limit_break" || (name == "hand" && perspective != player) {
                cards.sort_by(|left, right| left.card.handle.cmp(&right.card.handle));
            }
            zones.push(ZoneObservation {
                kind: zone(name)?,
                count: offset,
                capacity: zone_capacity(name)
                    .map(|limit| u64::try_from(limit).map_err(invalid))
                    .transpose()?,
                cards,
            });
        }
        Ok(PlayerObservation {
            player,
            resources: Resources {
                life: public_resource(&data.leader["life"])?,
                pp: public_resource(&data.pp["current"])?,
                maximum_pp: public_resource(&data.pp["max"])?,
                ep: data.ep,
                ep_capacity: data.ep_capacity,
                ep_used: data
                    .ep_capacity
                    .checked_sub(data.ep)
                    .ok_or_else(|| invalid("EP capacity difference overflow"))?,
                sep: data.sep,
                sep_available_from: data.sep_available_from,
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
        let handle = self.observed_object_handle(perspective, id)?;
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

    pub(in crate::game) fn observed_object_handle(
        &self,
        perspective: Viewer,
        id: &str,
    ) -> Result<ObjectHandle> {
        let handle = self
            .state
            .knowledge
            .get(seat(perspective))
            .and_then(|knowledge| knowledge.handle(id))
            .ok_or_else(|| invalid("card is not identifiable"))?;
        self.resolve_player_object(perspective, &handle)?;
        Ok(handle)
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
        let follower_with_stats = matches!(object.zone.as_str(), "field" | "ex")
            && self.object_type(id)?.contains("フォロワー");
        let stats = if follower_with_stats {
            let read = |stat| {
                if object.zone == "ex" {
                    self.internal_ex_follower_stat(id, stat)
                } else {
                    self.internal_numeric_stat(id, stat)
                }
            };
            Some(Stats {
                attack: read(Stat::Attack)?,
                life: read(Stat::Life)?,
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
            limit_break: if follower_with_stats && object.zone == "field" {
                self.limit_break_stage(id)
            } else {
                LimitBreakStage::None
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
