//! Components share the same authorized object scope rather than infer identity.

use super::{BattleReferences, ProjectionError, effects, sources};
use crate::catalog::Catalog;
use crate::game::player::{self, Viewer, observation as native};
use crate::ir::{ConsumptionPoint, EvolutionState, Posture};
use alloc::collections::{BTreeMap, BTreeSet};
use core::result;
use sve_battle_contract::{Int, Nat, ObjectHandle, Seat, observation as wire};

type Result<T> = result::Result<T, ProjectionError>;

/// One authenticated viewer's currently identifiable wire objects.
///
/// Keys retire when identification is lost. Remembered faces never restore a key.
/// `ObjectView::card` is physical; `effective_face` supplies current characteristics.
pub type ObjectTable = BTreeMap<ObjectHandle, wire::ObjectView>;

pub(crate) fn build_objects(
    catalog: &Catalog,
    references: &BattleReferences,
    observation: &native::Observation,
) -> Result<ObjectTable> {
    references.validate_catalog(catalog)?;
    let scope = ObjectScope::new(observation)?;
    build_scoped(catalog, references, observation, &scope)
}

pub(super) fn build_scoped(
    catalog: &Catalog,
    references: &BattleReferences,
    observation: &native::Observation,
    scope: &ObjectScope<'_>,
) -> Result<ObjectTable> {
    observation
        .players
        .iter()
        .flat_map(|player| &player.zones)
        .flat_map(|zone| &zone.cards)
        .map(|member| {
            let card = &member.card;
            let properties = &card.properties;
            Ok((
                scope.handle(&card.handle)?,
                wire::ObjectView {
                    card: references.resolve(catalog, &card.physical)?,
                    effective_face: references.resolve(
                        catalog,
                        &native::CardFace {
                            card: properties.card.clone(),
                            face: properties.face,
                        },
                    )?,
                    owner: seat(properties.owner),
                    controller: seat(properties.controller),
                    knowledge: if card.public {
                        wire::Knowledge::Public
                    } else {
                        wire::Knowledge::Private
                    },
                    facing: if card.face_up {
                        wire::Facing::Up
                    } else {
                        wire::Facing::Down
                    },
                    characteristics: wire::Characteristics {
                        rules_names: properties.names.clone(),
                        card_type: properties.kind.clone(),
                        traits: properties.traits.clone(),
                        // The native observation does not supply these current values.
                        classes: None,
                        cost: None,
                    },
                    state: properties
                        .state
                        .as_ref()
                        .map(|state| card_state(catalog, references, scope, state))
                        .transpose()?,
                },
            ))
        })
        .collect()
}

fn card_state(
    catalog: &Catalog,
    references: &BattleReferences,
    scope: &ObjectScope<'_>,
    state: &native::CardState,
) -> Result<wire::CardState> {
    Ok(wire::CardState {
        posture: match state.posture {
            Posture::Standing => wire::Posture::Standing,
            Posture::Acted => wire::Posture::Acted,
        },
        evolution: match state.evolution {
            EvolutionState::Base => wire::EvolutionState::Base,
            EvolutionState::Evolved => wire::EvolutionState::Evolved,
            EvolutionState::SuperEvolved => wire::EvolutionState::SuperEvolved,
        },
        limit_break: state.limit_break,
        stats: state
            .stats
            .map(|stats| -> Result<_> {
                Ok(wire::Stats {
                    attack: Int::try_from(stats.attack)?,
                    life: Int::try_from(stats.life)?,
                })
            })
            .transpose()?,
        keywords: state.keywords.clone(),
        counters: state
            .counters
            .iter()
            .map(|counter| {
                let name = catalog
                    .keywords
                    .iter()
                    .find_map(|(key, entry)| {
                        let name = entry["ja"].as_str()?;
                        (entry["role"] == "counter"
                            && !name.is_empty()
                            && (key == &counter.name || name == counter.name))
                            .then_some(name)
                    })
                    .ok_or(ProjectionError::Unavailable)?;
                Ok(wire::Counter {
                    name: name.into(),
                    amount: Nat::try_from(counter.amount)?,
                })
            })
            .collect::<Result<Vec<_>>>()?,
        boxed: state.boxed,
        effects: effects::installed(catalog, references, scope, &state.effects)?,
        quotas: state
            .quotas
            .as_ref()
            .map(|quotas| {
                quotas
                    .iter()
                    .map(|quota| {
                        Ok(wire::AbilityQuota {
                            source: sources::source(catalog, references, scope, &quota.source)?,
                            ability: effects::ability_kind(quota.kind),
                            maximum: Nat::try_from(quota.maximum)?,
                            remaining: Nat::try_from(quota.remaining)?,
                            consume_at: match quota.consume_at {
                                ConsumptionPoint::PendingAdmission => {
                                    wire::ConsumptionPoint::PendingAdmission
                                }
                                ConsumptionPoint::PlayCommitted => {
                                    wire::ConsumptionPoint::PlayCommitted
                                }
                                ConsumptionPoint::RuleDefined => {
                                    wire::ConsumptionPoint::RuleDefined
                                }
                            },
                        })
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?,
    })
}

const fn seat(viewer: Viewer) -> Seat {
    match viewer {
        Viewer::P1 => Seat::P1,
        Viewer::P2 => Seat::P2,
    }
}

pub(super) struct ObjectScope<'observation> {
    known: BTreeSet<&'observation player::ObjectHandle>,
    quotable: BTreeSet<&'observation player::ObjectHandle>,
    history: BTreeSet<&'observation player::ObjectHandle>,
}

impl<'observation> ObjectScope<'observation> {
    pub(super) fn new(observation: &'observation native::Observation) -> Result<Self> {
        if observation
            .players
            .iter()
            .map(|player| player.player)
            .ne([Viewer::P1, Viewer::P2])
        {
            return Err(ProjectionError::Unavailable);
        }
        let mut objects = Self {
            known: BTreeSet::new(),
            quotable: BTreeSet::new(),
            history: BTreeSet::new(),
        };
        for player in &observation.players {
            for zone in &player.zones {
                if player.player == observation.viewer
                    && zone.kind == native::ObservedZone::Hand
                    && u64::try_from(zone.cards.len()).ok() != Some(zone.count)
                {
                    return Err(ProjectionError::Unavailable);
                }
                for member in &zone.cards {
                    let handle = &member.card.handle;
                    if !objects.known.insert(handle) {
                        return Err(ProjectionError::Unavailable);
                    }
                    if zone.kind.supports_public_history_link() {
                        objects.history.insert(handle);
                    }
                    if player.player == observation.viewer
                        && member.card.properties.controller == observation.viewer
                        && matches!(
                            zone.kind,
                            native::ObservedZone::Hand | native::ObservedZone::Ex
                        )
                    {
                        objects.quotable.insert(handle);
                    }
                }
            }
        }
        Ok(objects)
    }

    pub(super) fn quote(&self, handle: &player::ObjectHandle) -> Result<ObjectHandle> {
        if !self.quotable.contains(handle) {
            return Err(ProjectionError::Unavailable);
        }
        self.handle(handle)
    }

    pub(super) fn handle(&self, handle: &player::ObjectHandle) -> Result<ObjectHandle> {
        if !self.known.contains(handle) {
            return Err(ProjectionError::Unavailable);
        }
        Ok(ObjectHandle::new(handle.as_str().into()))
    }

    pub(super) fn optional(
        &self,
        handle: Option<&player::ObjectHandle>,
    ) -> Result<Option<ObjectHandle>> {
        handle.map(|handle| self.handle(handle)).transpose()
    }

    pub(super) fn history_link(
        &self,
        handle: Option<&player::ObjectHandle>,
    ) -> Result<Option<ObjectHandle>> {
        handle
            .map(|handle| {
                if !self.history.contains(handle) {
                    return Err(ProjectionError::Unavailable);
                }
                self.handle(handle)
            })
            .transpose()
    }
}
