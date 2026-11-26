use super::{AnswerError, Context, Result};
use crate::battle::question::common::seat;
use crate::battle::selection::zone;
use crate::battle::{ProjectionError, SelectionProjection};
use crate::game::player::{self, SelectionQuestion};
use crate::ir::Position;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use core::result;
use sve_battle_contract::{Answer, question as wire};

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
    let usage = match kind {
        Kind::Targets => match context.purpose {
            player::QuestionPurpose::PlayParameter => wire::QuestionUse::Targets,
            player::QuestionPurpose::ResolutionChoice => wire::QuestionUse::ResolutionSelection,
            // Well-formed programs make one rule choice among objects: the Stack recipient.
            player::QuestionPurpose::RuleChoice => wire::QuestionUse::StackRecipient,
            player::QuestionPurpose::QuickWindow | player::QuestionPurpose::MainAction => {
                return Err(ProjectionError::Unavailable);
            }
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
    let projection = freeze(domain, context)?;
    Ok((
        usage,
        wire::QuestionShape::Select {
            selection: projection.selection().clone(),
        },
        super::Binding::Select(Box::new(Binding { projection, kind })),
    ))
}

pub(super) fn freeze(
    domain: &SelectionQuestion,
    context: Context<'_>,
) -> Result<SelectionProjection> {
    if context.observation.viewer != seat(context.actor)
        || u64::try_from(domain.members.len())
            .ok()
            .is_none_or(|count| domain.cardinality.minimum.0 > count)
    {
        return Err(ProjectionError::Unavailable);
    }
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
    SelectionProjection::new(context.id, domain, &destinations, |coordinate| {
        let member = domain
            .members
            .iter()
            .find(|member| member.handle == coordinate)
            .ok_or(ProjectionError::Unavailable)?;
        super::candidates::details(member, context)
    })
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
