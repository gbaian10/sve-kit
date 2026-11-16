//! Effect domains retain their original native answer channels.

use super::{AnswerError, Result};
use crate::battle::{BattleReferences, ProjectionError};
use crate::catalog::Catalog;
use crate::game::player::{self, DecisionShape, QuestionToken, Viewer};
use alloc::boxed::Box;
use core::result;
use sve_battle_contract::{Answer, QuestionId, observation::BattleObservation, question as wire};

mod ability;
mod candidates;
mod capacity;
mod confirm;
mod distribution;
mod drive;
mod entry;
mod modes;
mod number;
mod order;
mod partition;
mod replacement;
mod resource;
mod search;
mod select;
mod text;
pub(super) use candidates::details;

#[derive(Clone, Copy)]
pub(super) struct Context<'projection> {
    pub(super) id: &'projection QuestionId,
    pub(super) actor: Viewer,
    pub(super) purpose: player::QuestionPurpose,
    pub(super) observation: &'projection BattleObservation,
    pub(super) catalog: &'projection Catalog,
    pub(super) references: &'projection BattleReferences,
}

#[derive(Clone)]
pub(super) enum Binding {
    Confirm(confirm::Binding),
    Distribution(distribution::Binding),
    Drive(drive::Binding),
    Number(number::Binding),
    Resource(Box<resource::Binding>),
    Select(Box<select::Binding>),
    Order(order::Binding),
    Search(Box<search::Binding>),
    Partition(partition::Binding),
    Capacity(capacity::Binding),
    Entry(entry::Binding),
    Modes(modes::Binding),
    Ability(ability::Binding),
}

pub(super) fn project(
    domain: &DecisionShape,
    context: Option<Context<'_>>,
) -> Result<(wire::QuestionUse, wire::QuestionShape, Binding)> {
    match domain {
        DecisionShape::OptionalCost => Ok(confirm::project(confirm::Binding::OptionalCost)),
        DecisionShape::ResolutionPayment => {
            Ok(confirm::project(confirm::Binding::ResolutionPayment))
        }
        DecisionShape::May => Ok(confirm::project(confirm::Binding::May)),
        DecisionShape::Integer { bounds, source } => number::integer(*bounds, *source),
        DecisionShape::DiceReroll { value, remaining } => number::dice(*value, *remaining),
        DecisionShape::Distribution(domain) => {
            distribution::project(domain, context.ok_or(ProjectionError::Unavailable)?)
        }
        DecisionShape::Targets(domain) => select::project(
            domain,
            select::Kind::Targets,
            context.ok_or(ProjectionError::Unavailable)?,
        ),
        DecisionShape::Discard {
            kind: player::DiscardKind::Cost,
            cards,
        } => select::project(
            cards,
            select::Kind::CostDiscard,
            context.ok_or(ProjectionError::Unavailable)?,
        ),
        DecisionShape::Discard {
            kind: player::DiscardKind::Effect,
            cards,
        } => select::project(
            cards,
            select::Kind::EffectDiscard,
            context.ok_or(ProjectionError::Unavailable)?,
        ),
        DecisionShape::CostMaterials(domain) => select::project(
            domain,
            select::Kind::CostMaterial,
            context.ok_or(ProjectionError::Unavailable)?,
        ),
        DecisionShape::ResourceEntryPayment {
            materials,
            ep_allowed,
        } => resource::project(
            materials,
            *ep_allowed,
            context.ok_or(ProjectionError::Unavailable)?,
        ),
        DecisionShape::DriveTrigger { icon, targets } => {
            drive::project(*icon, targets, context.ok_or(ProjectionError::Unavailable)?)
        }
        DecisionShape::Order(domain) => {
            order::project(domain, context.ok_or(ProjectionError::Unavailable)?)
        }
        DecisionShape::Search(domain) => {
            search::project(domain, context.ok_or(ProjectionError::Unavailable)?)
        }
        DecisionShape::SearchPartition(domain) => {
            partition::project(domain, context.ok_or(ProjectionError::Unavailable)?)
        }
        DecisionShape::BeforeDraw => entry::draw(context.ok_or(ProjectionError::Unavailable)?),
        DecisionShape::WardEntry { .. } => {
            entry::ward(domain, context.ok_or(ProjectionError::Unavailable)?)
        }
        DecisionShape::Modes {
            modes,
            cardinality,
            count,
        } => modes::project(
            modes,
            *cardinality,
            *count,
            wire::QuestionUse::Modes,
            context.ok_or(ProjectionError::Unavailable)?,
        ),
        DecisionShape::ResolutionAlternative {
            modes,
            cardinality,
            count,
        } => modes::project(
            modes,
            *cardinality,
            *count,
            wire::QuestionUse::ResolutionAlternative,
            context.ok_or(ProjectionError::Unavailable)?,
        ),
        DecisionShape::MovementCapacity(domain) => {
            capacity::movement(domain, context.ok_or(ProjectionError::Unavailable)?)
        }
        DecisionShape::TokenCapacity(domain) => {
            capacity::tokens(domain, context.ok_or(ProjectionError::Unavailable)?)
        }
        DecisionShape::Abilities(choices) => {
            ability::project(choices, context.ok_or(ProjectionError::Unavailable)?)
        }
        DecisionShape::DamageReplacements(choices) => {
            replacement::project(choices, context.ok_or(ProjectionError::Unavailable)?)
        }
        DecisionShape::Quick { .. }
        | DecisionShape::ExchangeRoutes(_)
        | DecisionShape::Pending(_)
        | DecisionShape::TriggerQuota(_)
        | DecisionShape::SearchReplacement => Err(ProjectionError::Unavailable),
    }
}

impl Binding {
    pub(super) fn answer(
        &self,
        token: QuestionToken,
        answer: &Answer,
    ) -> result::Result<player::Command, AnswerError> {
        Ok(player::Command::Answer {
            token,
            answer: self.decode(answer)?,
        })
    }

    fn decode(&self, answer: &Answer) -> result::Result<player::Answer, AnswerError> {
        match self {
            Self::Confirm(binding) => binding.answer(answer),
            Self::Distribution(binding) => binding.answer(answer),
            Self::Drive(binding) => binding.answer(answer),
            Self::Number(binding) => binding.answer(answer),
            Self::Resource(binding) => binding.answer(answer),
            Self::Select(binding) => binding.answer(answer),
            Self::Order(binding) => binding.answer(answer),
            Self::Search(binding) => binding.answer(answer),
            Self::Partition(binding) => binding.answer(answer),
            Self::Capacity(binding) => binding.answer(answer),
            Self::Entry(binding) => binding.answer(answer),
            Self::Modes(binding) => binding.answer(answer),
            Self::Ability(binding) => binding.answer(answer),
        }
    }
}

#[cfg(test)]
mod tests;
