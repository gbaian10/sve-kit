//! Candidates and the complete constraints of a selection.
//!
//! Every limit is frozen by the authority; a client never recomputes one from a card face.

use crate::common::{OptionId, Seat, Subject, Zone};
use crate::number::{Int, Nat};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A count range; an absent maximum means unbounded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Bounds {
    /// Effective minimum, already reduced when fewer choices exist.
    pub min: Nat,
    /// Effective maximum.
    pub max: Option<Nat>,
}

/// End of an ordered zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Edge {
    /// Top.
    Top,
    /// Bottom.
    Bottom,
}

/// Where a candidate currently is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Origin {
    /// Player whose zone holds it.
    pub player: Seat,
    /// The zone.
    pub zone: Zone,
    /// Authorized offset within the zone.
    pub position: Option<Nat>,
}

/// Where chosen cards go.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Destination {
    /// Player whose zone receives them.
    pub player: Seat,
    /// Receiving zone.
    pub zone: Zone,
    /// End of an ordered zone, when the instruction names one.
    pub position: Option<Edge>,
}

/// Remaining space that arrivals compete for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Capacity {
    /// Zone with limited space.
    pub destination: Destination,
    /// Spaces available when the question was prepared.
    pub available: Nat,
}

/// One choice, addressed only within its question.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    /// Code to answer with.
    pub id: OptionId,
    /// Authorized subject; absent when the viewer may not know it.
    pub subject: Option<Subject>,
    /// Controller captured with a card candidate, when known.
    pub controller: Option<Seat>,
    /// Authorized current location.
    pub origin: Option<Origin>,
    /// Complete frozen rules names, when a name constraint uses them.
    pub rules_names: Option<Vec<String>>,
    /// Original cost, when a cost constraint uses it.
    pub original_cost: Option<Int>,
    /// Classes, when a class constraint uses them.
    pub classes: Option<Vec<String>>,
}

/// Candidates eligible for one group and the group's bounds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SelectionGroup {
    /// Printed position of the group, counted from zero.
    pub group: Nat,
    /// Candidates the group accepts; groups may overlap.
    pub eligible: Vec<OptionId>,
    /// Bounds of the part assigned to the group.
    pub bounds: Bounds,
}

/// How the answer relates the selection to its groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GroupAssignment {
    /// Some disjoint assignment must exist; the answer does not state it.
    Existence,
    /// The answer states every group, empty ones included.
    Explicit,
}

/// Printed rule for the size of the subset given one role.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RoleSize {
    /// A count within bounds.
    Fixed {
        /// The bounds.
        bounds: Bounds,
    },
    /// Exactly this many when anything is selected, otherwise none.
    IfNonempty {
        /// Required count.
        count: Nat,
    },
    /// Everything selected that no other role takes.
    Remainder,
}

/// One role in the complete, disjoint partition of the selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SelectionRole {
    /// Printed identifier of the role.
    pub role: Nat,
    /// Size rule of the role.
    pub size: RoleSize,
    /// Where cards given this role go, when the viewer may know it.
    pub destination: Option<Destination>,
}

/// A property that no two selected candidates may share.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Distinct {
    /// No rules name in common.
    RulesName,
    /// Different original costs.
    OriginalCost,
    /// No class in common.
    Class,
}

/// Limits on the sum of the selected original costs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CostTotal {
    /// Lower limit, if any.
    pub min: Option<Int>,
    /// Upper limit, if any.
    pub max: Option<Int>,
}

/// A selection with all of its constraints.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    /// Selectable candidates.
    pub candidates: Vec<Candidate>,
    /// Authorized subjects that are shown but cannot be selected.
    pub shown: Vec<Subject>,
    /// Bounds of the whole selection.
    pub bounds: Bounds,
    /// Groups the selection must satisfy; empty when there is none.
    pub groups: Vec<SelectionGroup>,
    /// Whether the answer states the group assignment.
    pub group_assignment: GroupAssignment,
    /// Roles to assign with the selection; empty when there is none.
    pub roles: Vec<SelectionRole>,
    /// Properties that must differ between selected candidates.
    pub distinct: Vec<Distinct>,
    /// Limits on the original-cost sum.
    pub original_cost_total: CostTotal,
    /// Space the selected arrivals compete for, when that is what is asked.
    pub capacity: Option<Capacity>,
}
