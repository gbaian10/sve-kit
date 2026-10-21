//! Frozen distribution occurrences and their authority-only damage producer.

use super::{Execution, Game, InputDomain, Result, Wait, invalid, seat_name, unsupported};
use crate::ir::{
    Binding, ChoiceDomain, DamageAllocation, DamageAmount, Decision, DecisionId, ExecutionId, Nat,
    PlayerId, RecipientSet, ResolvedDamageAllocation, ResolvedRecipients, RuntimeDamageRecipient,
    ScopeId, SnapshotId,
};
use serde_json::{Value, json};

/// One public recipient represented by a handle scoped to the question token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DistributionMember {
    /// Opaque coordinate, never a physical object ID or generation.
    pub handle: u64,
    /// Leader player or current controller of the public follower.
    pub player: PlayerId,
    /// Printed identity for a follower; absent for a leader.
    pub card: Option<String>,
    /// Printed rules name for a follower; absent for a leader.
    pub name: Option<String>,
}

/// Complete frozen question, excluding authority bindings and object references.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DistributionDomain {
    /// Every member must occur exactly once in the answer.
    pub members: Vec<DistributionMember>,
    /// Exact sum required for a nonempty domain.
    pub total: Nat,
    /// Minimum for each member, read from the checked IR.
    pub minimum: Nat,
}

/// One answer item using this occurrence's opaque handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DamageShare {
    /// Token-scoped member coordinate.
    pub handle: u64,
    /// Nonnegative assigned amount.
    pub amount: Nat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Producer {
    execution: ExecutionId,
    scope: ScopeId,
    decision: DecisionId,
    slot: u64,
}

#[derive(Clone, Debug)]
pub(super) struct DistributionWait {
    producer: Producer,
    snapshot: SnapshotId,
    recipients: ResolvedRecipients,
    total: Nat,
    minimum: Nat,
    expression: RecipientSet,
}

#[derive(Clone, Debug)]
pub(super) struct Allocation {
    occurrence: DistributionWait,
    answer: Vec<ResolvedDamageAllocation>,
}

pub(super) fn packet(domain: &DistributionDomain) -> Value {
    json!({
        "kind":"distribution", "total":domain.total.0, "minimum":domain.minimum.0,
        "members":domain.members.iter().map(|member| json!({
            "handle":member.handle, "player":member.player.seat,
            "card":member.card, "name":member.name
        })).collect::<Vec<_>>()
    })
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Distribution uses the shared authority and continuation."
)]
impl Game {
    pub(super) fn internal_distribution(
        &mut self,
        execution: &mut Execution,
        decision: &Decision,
    ) -> Result<()> {
        let ChoiceDomain::Distribution {
            recipients,
            total,
            minimum,
            binding,
        } = &decision.domain
        else {
            return Err(unsupported("distribution domain"));
        };
        let actor = execution.player(self, &decision.actor)?;
        let expression = recipients.clone();
        let recipients = execution.recipients(self, recipients)?;
        let total = execution.natural(self, total)?;
        let mut members = Vec::new();
        for (index, recipient) in recipients.members().iter().enumerate() {
            let (player, card, name) = match *recipient {
                RuntimeDamageRecipient::Leader { player } => (player, None, None),
                RuntimeDamageRecipient::Object { object } => {
                    let id = execution.object_id(object)?;
                    let current = self.object(id)?;
                    if !self.object_type(id)?.contains("フォロワー") {
                        return Err(unsupported("non-follower distribution recipient"));
                    }
                    // No LKI identity is exposed through a stale or originally off-field domain.
                    if current.generation != object.generation.0 || current.zone != "field" {
                        return Err(unsupported("non-current-field distribution recipient"));
                    }
                    (
                        super::player_id(&current.controller)?,
                        Some(current.card.clone()),
                        Some(self.card_name(id)?),
                    )
                }
            };
            seat_name(player)?;
            members.push(DistributionMember {
                handle: u64::try_from(index).map_err(invalid)?,
                player,
                card,
                name,
            });
        }
        if !members.is_empty() {
            let lower = minimum
                .0
                .checked_mul(u64::try_from(members.len()).map_err(invalid)?)
                .ok_or_else(|| invalid("distribution minimum overflow"))?;
            if lower > total.0 {
                return Err(unsupported("infeasible distribution minimum"));
            }
            i64::try_from(total.0)
                .map_err(|_error| unsupported("distribution total exceeds Damage receipt range"))?;
        }
        let wait = DistributionWait {
            producer: Producer {
                execution: execution.id,
                scope: binding.key().0,
                decision: decision.id,
                slot: binding.key().1,
            },
            snapshot: SnapshotId(self.state.ir_epoch),
            recipients,
            total,
            minimum: *minimum,
            expression,
        };
        if members.is_empty() {
            return execution.distribution_answer(&wait, &[]);
        }
        self.internal_wait(
            execution,
            decision.id,
            actor,
            InputDomain::Distribution(DistributionDomain {
                members,
                total,
                minimum: *minimum,
            }),
            Wait::Distribution(wait),
        )
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Allocation reads the same private execution environment."
)]
impl Execution {
    pub(super) fn distribution_answer(
        &mut self,
        wait: &DistributionWait,
        shares: &[DamageShare],
    ) -> Result<()> {
        if wait.producer.execution != self.id {
            return Err(invalid("foreign distribution execution"));
        }
        let answer = shares
            .iter()
            .map(|share| {
                let recipient = wait
                    .recipients
                    .members()
                    .get(usize::try_from(share.handle).map_err(invalid)?)
                    .copied()
                    .ok_or_else(|| invalid("foreign distribution handle"))?;
                Ok(ResolvedDamageAllocation {
                    recipient,
                    amount: share.amount,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if !wait
            .recipients
            .accepts_answer(wait.total, wait.minimum, &answer)
        {
            return Err(invalid(
                "distribution answer is not complete or violates its bounds",
            ));
        }
        let key = (wait.producer.scope, wait.producer.slot);
        if self.allocations.contains_key(&key) {
            return Err(invalid("distribution producer slot reused"));
        }
        self.allocations.insert(
            key,
            Allocation {
                occurrence: wait.clone(),
                answer,
            },
        );
        Ok(())
    }

    pub(super) fn damage_shares(
        &self,
        game: &Game,
        recipients: &RecipientSet,
        amount: &DamageAmount,
    ) -> Result<(Vec<ResolvedDamageAllocation>, i64)> {
        match amount {
            DamageAmount::Uniform { amount } => {
                let recipients = self.recipients(game, recipients)?;
                let amount = self.integer(game, amount)?.0.max(0);
                let requested = amount
                    .checked_mul(i64::try_from(recipients.members().len()).map_err(invalid)?)
                    .ok_or_else(|| invalid("damage requested overflow"))?;
                let amount = Nat(u64::try_from(amount).map_err(invalid)?);
                Ok((
                    recipients
                        .members()
                        .iter()
                        .map(|&recipient| ResolvedDamageAllocation { recipient, amount })
                        .collect(),
                    requested,
                ))
            }
            DamageAmount::Allocation { binding } => self.allocated_shares(*binding, recipients),
        }
    }

    fn allocated_shares(
        &self,
        binding: Binding<Vec<DamageAllocation>>,
        expression: &RecipientSet,
    ) -> Result<(Vec<ResolvedDamageAllocation>, i64)> {
        let allocation = self
            .allocations
            .get(&binding.key())
            .ok_or_else(|| invalid("missing distribution producer"))?;
        let occurrence = &allocation.occurrence;
        if occurrence.snapshot.0 < self.id.0
            || occurrence.producer.execution != self.id
            || (occurrence.producer.scope, occurrence.producer.slot) != binding.key()
            || &occurrence.expression != expression
        {
            return Err(invalid("foreign distribution producer"));
        }
        let requested = allocation.answer.iter().try_fold(0_i64, |sum, item| {
            sum.checked_add(i64::try_from(item.amount.0).map_err(invalid)?)
                .ok_or_else(|| invalid("allocation requested overflow"))
        })?;
        Ok((allocation.answer.clone(), requested))
    }
}
