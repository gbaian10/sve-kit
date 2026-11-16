use super::{AnswerError, Context, Result};
use crate::battle::question::common::seat;
use crate::battle::selection::zone;
use crate::battle::{CandidateDetails, ProjectionError, SelectionProjection};
use crate::game::player::{self, SelectionMember, SelectionQuestion};
use crate::ir::Position;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use core::result;
use sve_battle_contract::{Answer, Subject, Zone, question as wire};

#[derive(Clone)]
pub(in crate::battle::question) struct Binding {
    projection: SelectionProjection,
    kind: Kind,
}

#[derive(Clone, Copy)]
pub(super) enum Kind {
    Targets,
    CostMaterial,
    CostDiscard,
    EffectDiscard,
}

pub(super) fn project(
    domain: &SelectionQuestion,
    kind: Kind,
    context: Context<'_>,
) -> Result<(wire::QuestionUse, wire::QuestionShape, super::Binding)> {
    if context.observation.viewer != seat(context.actor)
        || u64::try_from(domain.members.len())
            .ok()
            .is_none_or(|count| domain.cardinality.minimum.0 > count)
    {
        return Err(ProjectionError::Unavailable);
    }
    let usage = match kind {
        Kind::Targets => match context.purpose {
            player::QuestionPurpose::PlayParameter => wire::QuestionUse::Targets,
            player::QuestionPurpose::ResolutionChoice => wire::QuestionUse::ResolutionSelection,
            player::QuestionPurpose::RuleChoice
            | player::QuestionPurpose::QuickWindow
            | player::QuestionPurpose::MainAction => return Err(ProjectionError::Unavailable),
        },
        Kind::CostDiscard | Kind::CostMaterial => {
            if context.purpose != player::QuestionPurpose::PlayParameter || !domain.roles.is_empty()
            {
                return Err(ProjectionError::Unavailable);
            }
            if matches!(kind, Kind::CostMaterial) {
                wire::QuestionUse::CostMaterial
            } else {
                wire::QuestionUse::CostDiscard
            }
        }
        Kind::EffectDiscard => {
            if context.purpose != player::QuestionPurpose::ResolutionChoice
                || !domain.roles.is_empty()
            {
                return Err(ProjectionError::Unavailable);
            }
            wire::QuestionUse::EffectDiscard
        }
    };
    let mut destinations = BTreeMap::new();
    for role in &domain.roles {
        if let Some(native) = role.destination {
            let position = match role.position {
                Some(Position::Top {}) => Some(wire::Edge::Top),
                Some(Position::Bottom {}) => Some(wire::Edge::Bottom),
                Some(Position::Index { .. }) => {
                    return Err(ProjectionError::Unavailable);
                }
                None => None,
            };
            destinations.insert(
                role.role.0,
                wire::Destination {
                    player: seat(native.player),
                    zone: zone(native.zone),
                    position,
                },
            );
        }
    }
    let projection = SelectionProjection::new(context.id, domain, &destinations, |coordinate| {
        let member = domain
            .members
            .iter()
            .find(|member| member.handle == coordinate)
            .ok_or(ProjectionError::Unavailable)?;
        details(member, context)
    })?;
    Ok((
        usage,
        wire::QuestionShape::Select {
            selection: projection.selection().clone(),
        },
        super::Binding::Select(Box::new(Binding { projection, kind })),
    ))
}

fn details(member: &SelectionMember, context: Context<'_>) -> Result<CandidateDetails> {
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

impl Binding {
    pub(super) fn answer(&self, answer: &Answer) -> result::Result<player::Answer, AnswerError> {
        let Answer::Select { selected, .. } = answer else {
            return Err(AnswerError);
        };
        let count = u64::try_from(selected.len()).map_err(|_range| AnswerError)?;
        let bounds = self.projection.selection().bounds;
        if count < bounds.min.get() || bounds.max.is_some_and(|max| count > max.get()) {
            return Err(AnswerError);
        }
        let native = self
            .projection
            .targets(answer)
            .map_err(|_projection| AnswerError)?;
        match self.kind {
            Kind::Targets | Kind::EffectDiscard => Ok(native),
            Kind::CostDiscard | Kind::CostMaterial => {
                let player::Answer::Targets(handles) = native else {
                    return Err(AnswerError);
                };
                Ok(player::Answer::CostMaterials(handles))
            }
        }
    }
}
