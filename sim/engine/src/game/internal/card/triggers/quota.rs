//! A simultaneous admission commits quota and occurrences only after every controller chooses.
use super::{
    Game, PendingOccurrence, PendingOption, PlayerId, Result, invalid, player_id, unsupported,
};
use crate::game::internal::{Execution, InputDomain, Wait};
use crate::ir::{
    AbilityCostPolicy, AbilityInstanceId, BatchPolicy, DecisionId, LimitSpec, TriggerAggregation,
};
use alloc::collections::{BTreeMap, BTreeSet, VecDeque};

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
    groups: VecDeque<QuotaGroup>,
    earlier: Vec<QuotaSelection>,
}

#[derive(Clone, Debug)]
struct QuotaGroup {
    candidates: Vec<PendingOccurrence>,
    remaining: u64,
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
        let mut limited: BTreeMap<AbilityInstanceId, (LimitSpec, Vec<PendingOccurrence>)> =
            BTreeMap::new();
        let mut batches = BTreeSet::new();
        for candidate in candidates {
            if let Some(trigger) = &candidate.ability.node.trigger {
                match trigger.aggregation {
                    TriggerAggregation::PerFact {} => {}
                    TriggerAggregation::PerBatch {
                        key: BatchPolicy::Instruction {},
                    } => {
                        let batch = candidate
                            .header
                            .batch
                            .ok_or_else(|| invalid("missing instruction batch"))?;
                        if !batches.insert((candidate.source.instance, batch)) {
                            continue;
                        }
                    }
                    TriggerAggregation::PerBatch {
                        key: BatchPolicy::Simultaneous { .. },
                    } => {
                        return Err(unsupported("simultaneous trigger aggregation"));
                    }
                }
            }
            let limit = candidate
                .ability
                .node
                .trigger
                .as_ref()
                .and_then(|trigger| trigger.limit);
            if let Some(limit) = limit {
                if self.card_runtime.quota_available(
                    candidate.source.instance,
                    candidate.header.time.turn,
                    limit,
                ) {
                    limited
                        .entry(candidate.source.instance)
                        .or_insert_with(|| (limit, Vec::new()))
                        .1
                        .push(candidate);
                }
            } else {
                plan.ready.push(candidate);
            }
        }
        for (limit, group) in limited.into_values() {
            // One producer supplies a simultaneous batch, so every candidate shares a turn.
            let first = group
                .first()
                .ok_or_else(|| invalid("empty limited trigger group"))?;
            let remaining = self.card_runtime.quota_remaining(
                first.source.instance,
                first.header.time.turn,
                limit,
            );
            if u64::try_from(group.len()).map_err(invalid)? <= remaining {
                plan.ready.extend(group);
            } else {
                plan.groups.push_back(QuotaGroup {
                    candidates: group,
                    remaining,
                });
            }
        }
        let active = player_id(self.active())?;
        plan.groups.make_contiguous().sort_by_key(|group| {
            group
                .candidates
                .first()
                .is_some_and(|p| p.source.controller != active)
        });
        self.internal_commit_admission(plan)
    }

    fn internal_commit_admission(&mut self, plan: Admission) -> Result<()> {
        if !plan.groups.is_empty() {
            self.card_runtime.admission = Some(plan);
            return Ok(());
        }
        for pending in plan.ready {
            if let Some(limit) = pending
                .ability
                .node
                .trigger
                .as_ref()
                .and_then(|trigger| trigger.limit)
            {
                self.card_runtime.consume_quota(
                    pending.source.instance,
                    pending.header.time.turn,
                    limit,
                    AbilityCostPolicy::Normal,
                )?;
            }
            self.card_runtime.pending.push(pending);
        }
        Ok(())
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
            .candidates
            .first()
            .ok_or_else(|| invalid("empty quota group"))?
            .source
            .controller;
        let candidates = group
            .candidates
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
        if index >= group.candidates.len() {
            return Err(invalid("unknown quota handle"));
        }
        let chosen = group.candidates.remove(index);
        plan.earlier.push(QuotaSelection {
            actor: chosen.source.controller,
            occurrence: chosen.public_option(handle)?,
        });
        plan.ready.push(chosen);
        group.remaining = group
            .remaining
            .checked_sub(1)
            .ok_or_else(|| invalid("empty quota allowance"))?;
        if group.remaining > 0 {
            plan.groups.push_front(group);
        }
        self.internal_commit_admission(plan)
    }
}
