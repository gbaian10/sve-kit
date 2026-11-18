use super::{AnswerError, Context, Result, pending};
use crate::battle::question::common::seat;
use crate::game::player::{self, QuotaQuestion};
use core::result;
use sve_battle_contract::{Answer, question as wire};

#[derive(Clone)]
pub(in crate::battle::question) struct Binding {
    candidates: pending::Binding,
}

pub(super) fn project(
    domain: &QuotaQuestion,
    context: Context<'_>,
) -> Result<(wire::QuestionShape, Binding)> {
    let (options, candidates) = pending::options(&domain.candidates, context)?;
    let earlier = domain
        .earlier
        .iter()
        .map(|selection| {
            Ok(wire::EarlierChoice {
                actor: seat(selection.actor),
                value: pending::value(&selection.occurrence, selection.actor, context)?,
            })
        })
        .collect::<Result<_>>()?;
    Ok((pending::shape(options, earlier), Binding { candidates }))
}

impl Binding {
    pub(super) fn answer(&self, answer: &Answer) -> result::Result<player::Answer, AnswerError> {
        Ok(player::Answer::TriggerQuota(
            self.candidates.chosen(answer)?,
        ))
    }
}
