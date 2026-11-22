//! Search the formal questions for one complete declaration.

use super::super::super::{
    Answer, DamageShare, DecisionToken, DeclarationMode, Execution, Game, InputDomain,
    TargetDomain, TargetRoleAssignment, TargetRoleDomain, Wait,
};
use super::super::selection::{selection_indices, sized_selections};
use crate::game::player::events::Journal;
use crate::ir::{Cardinality, Nat, PlayerId, RoleCardinality};
use crate::{EngineFailure, Result, invalid};
use core::iter::once;

/// Count answers across all branches of one intent, including rejected answers and retries.
/// An intent without a proven payment in this budget is omitted.
pub(in crate::game::internal::card) const MAX_ANSWERS: u32 = 128;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Declaration probes use the same question and payment authority as live play."
)]
impl Game {
    pub(in crate::game::internal::card) fn declaration_probe(&self) -> Self {
        let mut probe = self.clone();
        // Payment needs rules facts and movements. Viewer publication belongs to live play.
        probe.player_events = Journal::default();
        #[cfg(feature = "runner")]
        drop(probe.referee_events.take());
        probe
    }

    pub(in crate::game::internal::card) fn declaration_payable(
        mut self,
        execution: Result<Execution>,
    ) -> Result<bool> {
        let attempt = (|| {
            let mut execution = execution?;
            execution.declaration_mode = DeclarationMode::Probe;
            self.internal_play_parameters(&mut execution)?;
            if execution.waiting.is_none() {
                return Ok(true);
            }
            self.typed = Some(execution);
            let mut budget = MAX_ANSWERS;
            self.follow_declaration(&mut budget)
        })();
        rejected_branch(attempt)
    }

    fn follow_declaration(mut self, budget: &mut u32) -> Result<bool> {
        let execution = self
            .typed
            .take()
            .ok_or_else(|| invalid("missing declaration execution"))?;
        let (view, wait) = execution
            .waiting
            .clone()
            .ok_or_else(|| invalid("missing declaration question"))?;
        let mut answers = declaration_answers(&view.domain, &wait)?.peekable();
        let (actor, token) = (view.actor, view.token);
        while *budget > 0 {
            let Some(answer) = answers.next() else {
                break;
            };
            *budget = budget.saturating_sub(1);
            if *budget == 0 || answers.peek().is_none() {
                self.typed = Some(execution);
                return self.try_declaration_answer(actor, token, answer, budget);
            }
            let mut branch = self.clone();
            branch.typed = Some(execution.clone());
            if branch.try_declaration_answer(actor, token, answer, budget)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn try_declaration_answer(
        mut self,
        actor: PlayerId,
        token: DecisionToken,
        answer: Answer,
        budget: &mut u32,
    ) -> Result<bool> {
        if self
            .typed
            .as_ref()
            .is_some_and(|execution| execution.declines_required_head(&answer))
        {
            return Ok(false);
        }
        let mut validated = false;
        match self.answer_internal_step(actor, token, answer, &mut validated) {
            Err(EngineFailure::Invalid(_)) if !validated => Ok(false),
            Ok(progress) if progress.complete => Ok(true),
            Ok(_) if self.typed.is_some() => self.follow_declaration(budget),
            Ok(_) => Ok(false),
            Err(error) => rejected_branch(Err(error)),
        }
    }
}

fn rejected_branch(result: Result<bool>) -> Result<bool> {
    match result {
        Err(EngineFailure::IllegalPlay(_)) => Ok(false),
        other => other,
    }
}

fn selections(domain: &TargetDomain) -> impl Iterator<Item = Vec<u64>> + '_ {
    selection_indices(domain.members.len(), domain.cardinality).map(|indices| {
        indices
            .into_iter()
            .filter_map(|index| domain.members.get(index).map(|member| member.handle))
            .collect()
    })
}

pub(in crate::game::internal::card) fn cost_selections<'question>(
    domain: &'question TargetDomain,
    wait: &'question Wait,
) -> impl Iterator<Item = Vec<u64>> + 'question {
    let preferred = match wait {
        Wait::CostDiscard(wait) => wait.declaration_witness.as_ref(),
        _ => None,
    };
    preferred
        .into_iter()
        .cloned()
        .chain(selections(domain).filter(move |handles| preferred != Some(handles)))
}

fn declaration_answers<'question>(
    domain: &'question InputDomain,
    wait: &'question Wait,
) -> Result<Box<dyn Iterator<Item = Answer> + 'question>> {
    Ok(match domain {
        InputDomain::OptionalCost => Box::new([false, true].into_iter().map(Answer::OptionalCost)),
        InputDomain::Integer(bounds) => match wait {
            Wait::ExchangeRoute { affordable, .. } => Box::new(
                affordable
                    .iter()
                    .filter_map(|index| u64::try_from(*index).ok().map(Nat).map(Answer::Integer)),
            ),
            _ => Box::new(
                (bounds.minimum.0
                    ..=bounds
                        .maximum
                        .ok_or_else(|| invalid("unbounded declaration number"))?
                        .0)
                    .map(|number| Answer::Integer(Nat(number))),
            ),
        },
        InputDomain::Modes {
            modes, cardinality, ..
        } => Box::new(sized_selections(modes, *cardinality).map(Answer::Modes)),
        InputDomain::CostMaterials(domain) => {
            Box::new(cost_selections(domain, wait).map(Answer::CostMaterials))
        }
        InputDomain::ResourceEntryPayment {
            materials,
            ep_allowed,
        } => Box::new(selections(materials).flat_map(move |materials| {
            [false, true]
                .into_iter()
                .filter(move |use_ep| !*use_ep || *ep_allowed)
                .map(move |use_ep| Answer::ResourceEntryPayment {
                    materials: materials.clone(),
                    use_ep,
                })
        })),
        InputDomain::Targets(domain) if domain.roles.is_empty() => {
            Box::new(selections(domain).map(Answer::Targets))
        }
        InputDomain::Targets(domain) => Box::new(selections(domain).flat_map(|targets| {
            let count = u64::try_from(targets.len()).unwrap_or(u64::MAX);
            role_answers(&domain.roles, targets.clone(), count).map(move |roles| {
                Answer::TargetsWithRoles {
                    targets: targets.clone(),
                    roles,
                }
            })
        })),
        InputDomain::Distribution(domain) => {
            // The allocation is used during resolution. One valid split witnesses this question.
            let count = u64::try_from(domain.members.len()).map_err(invalid)?;
            let extra = domain
                .total
                .0
                .saturating_sub(domain.minimum.0.saturating_mul(count));
            let shares = domain
                .members
                .iter()
                .enumerate()
                .map(|(index, member)| DamageShare {
                    handle: member.handle,
                    amount: Nat(domain.minimum.0.saturating_add(if index == 0 {
                        extra
                    } else {
                        0
                    })),
                })
                .collect();
            Box::new(once(Answer::Distribution(shares)))
        }
        _ => return Err(invalid("unexpected formal declaration question")),
    })
}

fn role_answers(
    roles: &[TargetRoleDomain],
    remaining: Vec<u64>,
    total: u64,
) -> Box<dyn Iterator<Item = Vec<TargetRoleAssignment>> + '_> {
    let Some((first, rest)) = roles.split_first() else {
        return Box::new(remaining.is_empty().then(Vec::new).into_iter());
    };
    let bounds = match first.cardinality {
        RoleCardinality::Fixed { cardinality } => cardinality,
        RoleCardinality::IfNonempty { count } => {
            let size = if total == 0 { Nat(0) } else { count };
            Cardinality {
                minimum: size,
                maximum: Some(size),
            }
        }
        RoleCardinality::Remainder {} => Cardinality {
            minimum: Nat(0),
            maximum: Some(Nat(total)),
        },
    };
    if rest.is_empty() {
        let count = u64::try_from(remaining.len()).unwrap_or(u64::MAX);
        let valid =
            count >= bounds.minimum.0 && bounds.maximum.is_none_or(|maximum| count <= maximum.0);
        return Box::new(
            valid
                .then(|| {
                    vec![TargetRoleAssignment {
                        role: first.role,
                        targets: remaining,
                    }]
                })
                .into_iter(),
        );
    }
    Box::new(
        selection_indices(remaining.len(), bounds).flat_map(move |indices| {
            let (assigned, rest_handles): (Vec<_>, Vec<_>) = remaining
                .iter()
                .copied()
                .enumerate()
                .partition(|(index, _)| indices.contains(index));
            let targets = assigned
                .into_iter()
                .map(|(_, handle)| handle)
                .collect::<Vec<_>>();
            let rest_handles = rest_handles.into_iter().map(|(_, handle)| handle).collect();
            role_answers(rest, rest_handles, total).map(move |mut assignments| {
                assignments.push(TargetRoleAssignment {
                    role: first.role,
                    targets: targets.clone(),
                });
                assignments
            })
        }),
    )
}
