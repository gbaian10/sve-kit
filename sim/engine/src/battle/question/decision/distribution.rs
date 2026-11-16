use super::candidates::Candidates;
use super::{AnswerError, Context, Result};
use crate::battle::ProjectionError;
use crate::game::player::{self, DistributionQuestion};
use crate::ir::Nat;
use core::result;
use sve_battle_contract::{Answer, Nat as WireNat, question as wire};

#[derive(Clone)]
pub(in crate::battle::question) struct Binding {
    candidates: Candidates,
    total: WireNat,
    minimum: WireNat,
}

pub(super) fn project(
    domain: &DistributionQuestion,
    context: Context<'_>,
) -> Result<(wire::QuestionUse, wire::QuestionShape, super::Binding)> {
    let total = WireNat::try_from(domain.total.0)?;
    let minimum = WireNat::try_from(domain.minimum.0)?;
    let count =
        u64::try_from(domain.members.len()).map_err(|_range| ProjectionError::Unavailable)?;
    if minimum
        .get()
        .checked_mul(count)
        .is_none_or(|lower| lower > total.get())
    {
        return Err(ProjectionError::Unavailable);
    }
    let candidates = Candidates::members(&domain.members, context)?;
    Ok((
        wire::QuestionUse::DamageDistribution,
        wire::QuestionShape::Distribute {
            recipients: candidates.values.clone(),
            total,
            share: wire::Bounds {
                min: minimum,
                max: (!domain.members.is_empty()).then_some(total),
            },
            // Native empty allocations bypass the total and continue without damage.
            empty: wire::EmptyDistribution::AllowWithoutTotal,
        },
        super::Binding::Distribution(Binding {
            candidates,
            total,
            minimum,
        }),
    ))
}

impl Binding {
    pub(super) fn answer(&self, answer: &Answer) -> result::Result<player::Answer, AnswerError> {
        let Answer::Distribute { shares } = answer else {
            return Err(AnswerError);
        };
        if shares.len() != self.candidates.values.len() {
            return Err(AnswerError);
        }
        let handles = self.candidates.lookup(
            &shares
                .iter()
                .map(|share| share.option.clone())
                .collect::<Vec<_>>(),
        )?;
        let mut sum = 0_u64;
        let mut values = Vec::new();
        for (share, handle) in shares.iter().zip(handles) {
            if share.amount < self.minimum {
                return Err(AnswerError);
            }
            sum = sum.checked_add(share.amount.get()).ok_or(AnswerError)?;
            values.push(player::DamageShare {
                handle,
                amount: Nat(share.amount.get()),
            });
        }
        if !shares.is_empty() && sum != self.total.get() {
            return Err(AnswerError);
        }
        Ok(player::Answer::Distribution(values))
    }
}
