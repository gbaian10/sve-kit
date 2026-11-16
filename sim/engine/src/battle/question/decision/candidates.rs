use super::{AnswerError, Context, Result};
use crate::battle::question::common::seat;
use crate::battle::{CandidateDetails, ProjectionError};
use crate::game::player::{CardOption, SelectionMember};
use alloc::collections::{BTreeMap, BTreeSet};
use core::result;
use sve_battle_contract::{Int, OptionId, Subject, Zone, question as wire};

#[derive(Clone)]
pub(super) struct Candidates {
    pub(super) values: Vec<wire::Candidate>,
    coordinates: BTreeMap<OptionId, u64>,
}

impl Candidates {
    pub(super) fn members(members: &[SelectionMember], context: Context<'_>) -> Result<Self> {
        if context.observation.viewer != seat(context.actor) {
            return Err(ProjectionError::Unavailable);
        }
        let mut result = Self {
            values: vec![],
            coordinates: BTreeMap::new(),
        };
        let mut seen = BTreeSet::new();
        for member in members {
            if !seen.insert(member.handle) {
                return Err(ProjectionError::Unavailable);
            }
            let facts = details(member, context)?;
            if facts.controller != Some(seat(member.player)) {
                return Err(ProjectionError::Unavailable);
            }
            let id = OptionId::new(format!("{}:c{}", context.id.as_str(), result.values.len()));
            result.coordinates.insert(id.clone(), member.handle);
            result.values.push(wire::Candidate {
                id,
                subject: facts.subject,
                controller: facts.controller,
                origin: facts.origin,
                rules_names: member.rules_names.clone(),
                original_cost: member.original_cost.map(Int::try_from).transpose()?,
                classes: facts.classes,
            });
        }
        Ok(result)
    }

    pub(super) fn drive(cards: &[CardOption], context: Context<'_>) -> Result<Self> {
        let members = cards
            .iter()
            .map(|card| SelectionMember {
                handle: card.handle,
                player: context.actor,
                card: Some(card.card.clone()),
                name: Some(card.name.clone()),
                object: card.object.clone(),
                rules_names: card.rules_names.clone(),
                original_cost: card.original_cost,
            })
            .collect::<Vec<_>>();
        let candidates = Self::members(&members, context)?;
        if candidates.values.iter().any(|candidate| {
            candidate.origin.is_none_or(|origin| {
                origin.zone != Zone::Field || origin.player != seat(context.actor)
            })
        }) {
            return Err(ProjectionError::Unavailable);
        }
        Ok(candidates)
    }

    pub(super) fn lookup(&self, ids: &[OptionId]) -> result::Result<Vec<u64>, AnswerError> {
        let mut seen = BTreeSet::new();
        ids.iter()
            .map(|id| {
                if !seen.insert(id) {
                    return Err(AnswerError);
                }
                self.coordinates.get(id).copied().ok_or(AnswerError)
            })
            .collect()
    }
}

pub(in crate::battle::question) fn details(
    member: &SelectionMember,
    context: Context<'_>,
) -> Result<CandidateDetails> {
    if member.card.is_none() {
        if member.name.is_some() || member.object.is_some() {
            return Err(ProjectionError::Unavailable);
        }
        return Ok(CandidateDetails {
            subject: Some(Subject::Leader {
                player: seat(member.player),
            }),
            controller: Some(seat(member.player)),
            origin: None,
            classes: None,
        });
    }
    let handle = member.object.as_ref().ok_or(ProjectionError::Unavailable)?;
    // Only a unique handle in this authorized board can identify the candidate.
    // Missing or repeated handles cannot be repaired from another source.
    let mut found = None;
    for player in &context.observation.players {
        let zones = &player.zones;
        for (kind, zone) in [
            (Zone::Deck, &zones.deck),
            (Zone::EvolveDeck, &zones.evolve_deck),
            (Zone::Hand, &zones.hand),
            (Zone::Field, &zones.field),
            (Zone::Ex, &zones.ex),
            (Zone::Cemetery, &zones.cemetery),
            (Zone::Banish, &zones.banish),
            (Zone::Resolution, &zones.resolution),
            (Zone::Evolution, &zones.evolution),
            (Zone::Race, &zones.race),
            (Zone::Drive, &zones.drive),
            (Zone::Trigger, &zones.trigger),
            (Zone::Equipment, &zones.equipment),
        ] {
            for located in &zone.cards {
                if located.object.as_str() != handle.as_str() {
                    continue;
                }
                if found.is_some() {
                    return Err(ProjectionError::Unavailable);
                }
                let object = context
                    .observation
                    .objects
                    .get(&located.object)
                    .ok_or(ProjectionError::Unavailable)?;
                found = Some(CandidateDetails {
                    subject: Some(Subject::Card {
                        card: object.effective_face.clone(),
                        object: Some(located.object.clone()),
                    }),
                    controller: Some(object.controller),
                    origin: Some(wire::Origin {
                        player: player.seat,
                        zone: kind,
                        position: located.position,
                    }),
                    // No captured class constraint is present in this native domain.
                    classes: None,
                });
            }
        }
    }
    found.ok_or(ProjectionError::Unavailable)
}
