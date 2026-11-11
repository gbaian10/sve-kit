//! Frozen selection data, without authority references or bindings.

use super::{ObjectHandle, Viewer};
use crate::ir::{Cardinality, Nat, Position, RoleCardinality};
use alloc::collections::BTreeMap;

/// One recipient or physical-card choice within this question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionMember {
    /// Original answer coordinate.
    pub handle: u64,
    /// Leader seat or captured card controller.
    pub player: Viewer,
    /// Authorized printing; absent for a leader.
    pub card: Option<String>,
    /// Authorized display name; absent for a leader.
    pub name: Option<String>,
    /// Current Observation capability, only while its generation remains identifiable.
    pub object: Option<ObjectHandle>,
    /// Complete captured rules names when a name constraint uses them.
    pub rules_names: Option<Vec<String>>,
    /// Original cost used by this question's cost constraints.
    pub original_cost: Option<i64>,
}

/// Candidates eligible for one group and its effective bounds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionGroup {
    /// Original coordinates from this question, including overlapping eligibility.
    pub handles: Vec<u64>,
    /// Every group must receive a disjoint part satisfying these bounds.
    pub cardinality: Cardinality,
}

/// One role in the complete disjoint target partition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectionRole {
    /// Authorized destination edge for private-card roles.
    pub position: Option<Position>,
    /// Original role coordinate.
    pub role: Nat,
    /// Printed rule for the assigned subset's size.
    pub cardinality: RoleCardinality,
}

/// Complete frozen selection constraints; answers still undergo native revalidation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionQuestion {
    /// Chooser-authorized candidates.
    pub members: Vec<SelectionMember>,
    /// Effective bounds for the whole selection.
    pub cardinality: Cardinality,
    /// No rules name may occur in two selected candidates.
    pub distinct_names: bool,
    /// Original costs keyed by the unchanged answer coordinates.
    pub original_costs: Option<BTreeMap<u64, i64>>,
    /// Selected candidates must have pairwise distinct original costs.
    pub distinct_original_costs: bool,
    /// There must exist a disjoint assignment to every advertised group.
    pub groups: Vec<SelectionGroup>,
    /// Answer all roles together with the complete selected set.
    pub roles: Vec<SelectionRole>,
    /// Lower bound on the selected original-cost sum.
    pub minimum_original_cost: Option<i64>,
    /// Upper bound on the selected original-cost sum.
    pub maximum_original_cost: Option<i64>,
}

/// A complete recipient allocation using only this occurrence's coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DistributionQuestion {
    /// Each member must appear exactly once in the allocation.
    pub members: Vec<SelectionMember>,
    /// Exact sum required for a nonempty set.
    pub total: Nat,
    /// Minimum share for each recipient.
    pub minimum: Nat,
}

impl SelectionQuestion {
    pub(in crate::game) fn set_original_costs(&mut self, costs: BTreeMap<u64, i64>) {
        for member in &mut self.members {
            member.original_cost = costs.get(&member.handle).copied();
        }
        self.original_costs = Some(costs);
    }
}
