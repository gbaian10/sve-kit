//! A simultaneous admission commits quota and occurrences only after every controller chooses.
use super::{Game, PendingOccurrence, PendingOption, PlayerId, Result, invalid, player_id};
use crate::game::internal::{Execution, InputDomain, Wait};
use crate::ir::{AbilityInstanceId, DecisionId};
use alloc::collections::{BTreeMap, VecDeque};

/// One earlier controller's public choice in the same simultaneous admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuotaSelection {
    /// Controller who made this selection.
    pub actor: PlayerId,
    /// Public occurrence context; its handle is no longer answerable.
    pub occurrence: PendingOption,
}

/// Only this question's choices and the simultaneous batch's prior public choices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuotaDomain {
    /// Token-scoped handles, never event objects or authority identities.
    pub candidates: Vec<PendingOption>,
    /// Completed earlier choices visible under the turn-player-first rule.
    pub earlier: Vec<QuotaSelection>,
}

#[derive(Clone, Debug, Default)]
pub(in crate::game::internal) struct Admission {
    ready: Vec<PendingOccurrence>,
    groups: VecDeque<Vec<PendingOccurrence>>,
    earlier: Vec<QuotaSelection>,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Quota selection and pending admission share one transaction."
)]
impl Game {
    pub(super) fn internal_admit_pending(
        &mut self,
        candidates: Vec<PendingOccurrence>,
    ) -> Result<()> {
        if self.card_runtime.admission.is_some() {
            return Err(invalid("another trigger admission is still pending"));
        }
        let mut plan = Admission::default();
        let mut limited: BTreeMap<AbilityInstanceId, Vec<PendingOccurrence>> = BTreeMap::new();
        for candidate in candidates {
            let limited_entry = candidate
                .ability
                .node
                .trigger
                .as_ref()
                .is_some_and(|trigger| trigger.limit.is_some());
            if limited_entry
                && self.card_runtime.quotas.get(&candidate.source.instance)
                    == Some(&candidate.header.time.turn)
            {
                continue;
            }
            if limited_entry {
                limited
                    .entry(candidate.source.instance)
                    .or_default()
                    .push(candidate);
            } else {
                plan.ready.push(candidate);
            }
        }
        for group in limited.into_values() {
            if group.len() == 1 {
                plan.ready.extend(group);
            } else {
                plan.groups.push_back(group);
            }
        }
        let active = player_id(self.active())?;
        plan.groups
            .make_contiguous()
            .sort_by_key(|group| group.first().is_some_and(|p| p.source.controller != active));
        self.internal_commit_admission(plan);
        Ok(())
    }

    fn internal_commit_admission(&mut self, plan: Admission) {
        if !plan.groups.is_empty() {
            self.card_runtime.admission = Some(plan);
            return;
        }
        for pending in plan.ready {
            if pending
                .ability
                .node
                .trigger
                .as_ref()
                .is_some_and(|trigger| trigger.limit.is_some())
            {
                self.card_runtime
                    .quotas
                    .insert(pending.source.instance, pending.header.time.turn);
            }
            self.card_runtime.pending.push(pending);
        }
    }

    pub(in crate::game::internal) fn internal_quota_wait(
        &mut self,
        execution: &mut Execution,
    ) -> Result<bool> {
        let Some(plan) = &self.card_runtime.admission else {
            return Ok(false);
        };
        let group = plan
            .groups
            .front()
            .ok_or_else(|| invalid("missing quota group"))?;
        let actor = group
            .first()
            .ok_or_else(|| invalid("empty quota group"))?
            .source
            .controller;
        let candidates = group
            .iter()
            .enumerate()
            .map(|(index, pending)| {
                pending.public_option(
                    u64::try_from(index)
                        .map_err(invalid)?
                        .checked_add(1)
                        .ok_or_else(|| invalid("quota handles exhausted"))?,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let count = candidates.len();
        let domain = InputDomain::TriggerQuota(QuotaDomain {
            candidates,
            earlier: plan.earlier.clone(),
        });
        self.internal_wait(
            execution,
            DecisionId(0),
            actor,
            domain,
            Wait::TriggerQuota { count },
        )?;
        Ok(true)
    }

    pub(in crate::game::internal) fn internal_quota_answer(&mut self, handle: u64) -> Result<()> {
        let mut plan = self
            .card_runtime
            .admission
            .take()
            .ok_or_else(|| invalid("missing quota admission"))?;
        let mut group = plan
            .groups
            .pop_front()
            .ok_or_else(|| invalid("missing quota candidates"))?;
        let index = usize::try_from(
            handle
                .checked_sub(1)
                .ok_or_else(|| invalid("unknown quota handle"))?,
        )
        .map_err(invalid)?;
        if index >= group.len() {
            return Err(invalid("unknown quota handle"));
        }
        let chosen = group.remove(index);
        plan.earlier.push(QuotaSelection {
            actor: chosen.source.controller,
            occurrence: chosen.public_option(handle)?,
        });
        plan.ready.push(chosen);
        self.internal_commit_admission(plan);
        Ok(())
    }
}
