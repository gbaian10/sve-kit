//! Local answer invariants; these do not execute actions or authorize observations.

use super::{
    Cardinality, ChoiceDomain, ModeDecision, ModeId, Nat, ObjectRef, RoleCardinality,
    StepwiseTiming, TargetRoleSpec,
};
use alloc::collections::BTreeSet;

/// A concrete allocation after the executor has resolved symbolic recipients.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedDamageAllocation {
    /// Object generations are part of membership.
    pub recipient: super::RuntimeDamageRecipient,
    /// Nonnegative damage assigned to this member, subject to the decision's minimum.
    pub amount: Nat,
}

/// Deduplicated membership evaluated once at the distribution decision's snapshot.
/// This local value does not evaluate selectors or authorize observations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedRecipients {
    members: Vec<super::RuntimeDamageRecipient>,
}
impl ResolvedRecipients {
    /// Normalize an evaluated domain, including unions and the empty set.
    #[must_use]
    pub fn new<T>(members: T) -> Self
    where
        T: IntoIterator<Item = super::RuntimeDamageRecipient>,
    {
        let mut unique = BTreeSet::new();
        Self {
            members: members
                .into_iter()
                .filter(|member| unique.insert(recipient_key(*member)))
                .collect(),
        }
    }

    /// Inspect the frozen, unique members.
    #[must_use]
    pub fn members(&self) -> &[super::RuntimeDamageRecipient] {
        &self.members
    }

    /// Require exactly one entry per member and the exact total without overflow.
    /// The executor supplies the rule minimum (one for selected play targets).
    /// Empty membership skips distribution regardless of total (CR 10.6.2.4.1).
    /// The same membership must be used by the corresponding damage producer.
    #[must_use]
    pub fn accepts_answer(
        &self,
        total: Nat,
        minimum: Nat,
        answer: &[ResolvedDamageAllocation],
    ) -> bool {
        if self.members.is_empty() {
            return answer.is_empty();
        }
        if answer.len() != self.members.len() {
            return false;
        }
        let mut seen = BTreeSet::new();
        let mut sum = 0_u64;
        for allocation in answer {
            if allocation.amount.0 < minimum.0
                || !self.members.contains(&allocation.recipient)
                || !seen.insert(recipient_key(allocation.recipient))
            {
                return false;
            }
            let Some(next) = sum.checked_add(allocation.amount.0) else {
                return false;
            };
            sum = next;
        }
        sum == total.0
    }
}

fn recipient_key(recipient: super::RuntimeDamageRecipient) -> (bool, u64, u64) {
    match recipient {
        super::RuntimeDamageRecipient::Object { object } => {
            (false, object.id.0, object.generation.0)
        }
        super::RuntimeDamageRecipient::Leader { player } => (true, u64::from(player.seat), 0),
    }
}

/// One role assignment in the same answer as the selected target set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetRoleAnswer {
    /// Role identity from the checked specification.
    pub role: Nat,
    /// Members retain their selected generation.
    pub objects: Vec<ObjectRef>,
}
impl TargetRoleSpec {
    /// Validate a complete, disjoint partition of an already validated selection.
    /// Domain membership and target legality remain the enclosing decision's responsibility.
    #[must_use]
    pub fn accepts_answer(&self, selected: &[ObjectRef], answer: &[TargetRoleAnswer]) -> bool {
        let selected_keys: BTreeSet<_> = selected.iter().map(object_key).collect();
        if selected_keys.len() != selected.len()
            || answer.len() != self.roles.len()
            || self.roles.is_empty()
        {
            return false;
        }
        let mut assigned = BTreeSet::new();
        let mut roles = BTreeSet::new();
        let mut remainders = 0_u64;
        for specification in &self.roles {
            if !roles.insert(specification.role.0) {
                return false;
            }
            let mut matching = answer
                .iter()
                .filter(|answer| answer.role == specification.role);
            let Some(role) = matching.next() else {
                return false;
            };
            if matching.next().is_some() {
                return false;
            }
            let Ok(count) = u64::try_from(role.objects.len()) else {
                return false;
            };
            let valid = match specification.cardinality {
                RoleCardinality::Fixed { cardinality } => contains(cardinality, count),
                RoleCardinality::IfNonempty { count: required } => {
                    required == Nat(1) && count == u64::from(!selected.is_empty())
                }
                RoleCardinality::Remainder {} => {
                    remainders = remainders.saturating_add(1);
                    remainders == 1
                }
            };
            if !valid {
                return false;
            }
            for object in &role.objects {
                let key = object_key(object);
                if !selected_keys.contains(&key) || !assigned.insert(key) {
                    return false;
                }
            }
        }
        assigned == selected_keys
    }
}

const fn object_key(object: &ObjectRef) -> (u64, u64) {
    (object.id.0, object.generation.0)
}
fn contains(bounds: Cardinality, count: u64) -> bool {
    count >= bounds.minimum.0 && bounds.maximum.is_none_or(|maximum| count <= maximum.0)
}

/// The only payload allowed at an up-to instruction's decision point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepwiseAnswer {
    /// Perform the next single-card operation.
    Continue,
    /// Finish this instruction without another card operation.
    Stop,
}
/// A protocol phase contains no card identities or hidden deck information.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepwisePhase {
    /// Await a continue/stop answer from the instruction's authorized actor.
    Decide,
    /// Perform exactly one card operation before accepting another answer.
    Perform,
    /// The instruction is terminal and accepts no more input.
    Complete,
}
/// Pure progression model for the repeated decision, independent of the executor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepwiseSession {
    timing: StepwiseTiming,
    remaining: u64,
    phase: StepwisePhase,
}
impl StepwiseSession {
    /// Start after evaluating the instruction's nonnegative effective limit.
    /// Both may stop before the first card. Later look answers follow the previous observation.
    #[must_use]
    pub const fn new(timing: StepwiseTiming, limit: Nat) -> Self {
        let phase = if limit.0 == 0 {
            StepwisePhase::Complete
        } else {
            StepwisePhase::Decide
        };
        Self {
            timing,
            remaining: limit.0,
            phase,
        }
    }

    /// The action determines whether each step draws or grants a look observation.
    #[must_use]
    pub const fn timing(&self) -> StepwiseTiming {
        self.timing
    }

    /// Current phase; no unrevealed card information is exposed.
    #[must_use]
    pub const fn phase(&self) -> StepwisePhase {
        self.phase
    }

    /// Apply one answer only when the protocol is waiting for it.
    /// Returns false without changing state if the answer arrives at the wrong phase.
    pub fn answer(&mut self, answer: StepwiseAnswer) -> bool {
        if self.phase != StepwisePhase::Decide {
            return false;
        }
        self.phase = match answer {
            StepwiseAnswer::Continue => StepwisePhase::Perform,
            StepwiseAnswer::Stop => StepwisePhase::Complete,
        };
        true
    }

    /// Confirm one attempted card operation, including attempts against an empty deck.
    /// The executor owns the resulting receipt and any authorized private observation.
    pub fn performed(&mut self) -> bool {
        if self.phase != StepwisePhase::Perform {
            return false;
        }
        self.remaining = self.remaining.saturating_sub(1);
        self.phase = if self.remaining == 0 {
            StepwisePhase::Complete
        } else {
            StepwisePhase::Decide
        };
        true
    }
}

impl ModeDecision {
    /// Check IDs and cardinality after evaluating the checked group's count at its specified time.
    /// This does not prove that a mode's parameters or costs are executable.
    #[must_use]
    pub fn accepts_answer(&self, count: Nat, answer: &[ModeId]) -> bool {
        let ChoiceDomain::Modes {
            modes, cardinality, ..
        } = &self.domain
        else {
            return false;
        };
        let Ok(available) = u64::try_from(modes.len()) else {
            return false;
        };
        let Ok(selected) = u64::try_from(answer.len()) else {
            return false;
        };
        let cap = count.0.min(available);
        if cap == 0 {
            return answer.is_empty();
        }
        if cardinality.minimum == Nat(0) && cap != available {
            return false;
        }
        let unique: BTreeSet<_> = answer.iter().collect();
        selected <= cap
            && contains(*cardinality, selected)
            && unique.len() == answer.len()
            && answer.iter().all(|id| modes.contains(id))
    }
}
