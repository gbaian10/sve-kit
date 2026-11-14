use super::ProjectionError;
use crate::game::player::{
    SearchAnswer, SearchQuestion, SelectionQuestion, TargetRoleAssignment, Viewer,
};
use crate::ir::{Cardinality, RoleCardinality};
use alloc::collections::{BTreeMap, BTreeSet};
use sve_battle_contract::question::{
    Bounds, Candidate, CostTotal, Destination, Distinct, GroupAssignment, Origin, RoleSize,
    Selection, SelectionGroup, SelectionRole,
};
use sve_battle_contract::{Answer, Int, Nat, OptionId, QuestionId, Seat, Subject};

type Result<T> = core::result::Result<T, ProjectionError>;

/// Candidate facts resolved by the authority against the pinned snapshot and viewer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateDetails {
    /// Authorized subject; absent for a candidate whose face cannot be disclosed.
    pub subject: Option<Subject>,
    /// Frozen controller, when known.
    pub controller: Option<Seat>,
    /// Authorized location, without an opponent's hidden slot.
    pub origin: Option<Origin>,
    /// Frozen classes, when known.
    pub classes: Option<Vec<String>>,
}

/// One frozen selection and its private native-coordinate lookup.
#[derive(Clone, Debug)]
pub struct SelectionProjection {
    selection: Selection,
    coordinates: BTreeMap<OptionId, u64>,
}

impl SelectionProjection {
    /// Project a complete native selection. The resolver receives only actor-authorized members.
    /// Role destinations come from the native procedure, never from a guessed deck edge.
    ///
    /// # Errors
    /// Refuses missing required facts, inconsistent coordinates, bounds or destinations.
    pub fn new(
        id: &QuestionId,
        question: &SelectionQuestion,
        destinations: &BTreeMap<u64, Destination>,
        mut resolve: impl FnMut(u64) -> Result<CandidateDetails>,
    ) -> Result<Self> {
        let mut projection = Self::empty(question.cardinality)?;
        for member in &question.members {
            let details = resolve(member.handle)?;
            if details.controller != Some(seat(member.player)) {
                return Err(ProjectionError::Unavailable);
            }
            let cost = question
                .original_costs
                .as_ref()
                .map_or(member.original_cost, |costs| {
                    costs.get(&member.handle).copied()
                });
            if member.original_cost.is_some() && member.original_cost != cost {
                return Err(ProjectionError::Unavailable);
            }
            projection.push(id, member.handle, details, member.rules_names.clone(), cost)?;
        }
        if question.original_costs.as_ref().is_some_and(|costs| {
            costs.len() != question.members.len()
                || question
                    .members
                    .iter()
                    .any(|member| !costs.contains_key(&member.handle))
        }) {
            return Err(ProjectionError::Unavailable);
        }
        projection.groups(&question.groups)?;
        let mut roles = BTreeSet::new();
        for role in &question.roles {
            if !roles.insert(role.role.0) {
                return Err(ProjectionError::Unavailable);
            }
            let destination = destinations.get(&role.role.0).copied();
            if let Some(position) = role.position {
                let edge = match position {
                    crate::ir::Position::Top {} => sve_battle_contract::question::Edge::Top,
                    crate::ir::Position::Bottom {} => sve_battle_contract::question::Edge::Bottom,
                    crate::ir::Position::Index { .. } => return Err(ProjectionError::Unavailable),
                };
                if destination.is_none_or(|destination| destination.position != Some(edge)) {
                    return Err(ProjectionError::Unavailable);
                }
            }
            projection.selection.roles.push(SelectionRole {
                role: Nat::try_from(role.role.0)?,
                size: match role.cardinality {
                    RoleCardinality::Fixed { cardinality } => RoleSize::Fixed {
                        bounds: bounds(cardinality)?,
                    },
                    RoleCardinality::IfNonempty { count } => RoleSize::IfNonempty {
                        count: Nat::try_from(count.0)?,
                    },
                    RoleCardinality::Remainder {} => RoleSize::Remainder,
                },
                destination,
            });
        }
        if destinations.keys().any(|role| !roles.contains(role)) {
            return Err(ProjectionError::Unavailable);
        }
        projection.constraints(
            question.distinct_names,
            question.distinct_original_costs,
            question.minimum_original_cost,
            question.maximum_original_cost,
        )?;
        Ok(projection)
    }

    /// Project a search, retaining explicit assignments for every printed group.
    ///
    /// # Errors
    /// The same completeness and numeric checks as [`Self::new`].
    pub fn search(
        id: &QuestionId,
        question: &SearchQuestion,
        mut resolve: impl FnMut(u64) -> Result<CandidateDetails>,
    ) -> Result<Self> {
        let mut projection = Self::empty(question.cardinality)?;
        for card in &question.cards {
            projection.push(
                id,
                card.handle,
                resolve(card.handle)?,
                card.rules_names.clone(),
                card.original_cost,
            )?;
        }
        if let Some(groups) = &question.groups {
            projection.groups(groups)?;
            projection.selection.group_assignment = GroupAssignment::Explicit;
        }
        projection.constraints(
            question.distinct_names,
            false,
            None,
            question.maximum_original_cost,
        )?;
        Ok(projection)
    }

    /// Complete wire selection; no native coordinate is serialized.
    #[must_use]
    pub const fn selection(&self) -> &Selection {
        &self.selection
    }

    /// Translate a target answer; the native procedure still checks all game legality.
    ///
    /// # Errors
    /// Refuses a different shape, foreign/duplicate candidates or incomplete roles.
    pub fn targets(&self, answer: &Answer) -> Result<crate::game::player::Answer> {
        let Answer::Select {
            selected,
            groups,
            roles,
        } = answer
        else {
            return Err(ProjectionError::Unavailable);
        };
        if groups.is_some() || roles.len() != self.selection.roles.len() {
            return Err(ProjectionError::Unavailable);
        }
        let targets = self.decode(selected)?;
        if roles.is_empty() {
            return Ok(crate::game::player::Answer::Targets(targets));
        }
        let mut assigned = BTreeSet::new();
        let mut seen_roles = BTreeSet::new();
        let roles = roles
            .iter()
            .map(|role| {
                if !seen_roles.insert(role.role)
                    || !self
                        .selection
                        .roles
                        .iter()
                        .any(|expected| expected.role == role.role)
                {
                    return Err(ProjectionError::Unavailable);
                }
                let subset = self.decode(&role.selected)?;
                if subset
                    .iter()
                    .any(|value| !targets.contains(value) || !assigned.insert(*value))
                {
                    return Err(ProjectionError::Unavailable);
                }
                Ok(TargetRoleAssignment {
                    role: crate::ir::Nat(role.role.get()),
                    targets: subset,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if assigned.len() != targets.len() {
            return Err(ProjectionError::Unavailable);
        }
        Ok(crate::game::player::Answer::TargetsWithRoles { targets, roles })
    }

    /// Translate a complete search answer, including empty groups in printed order.
    ///
    /// # Errors
    /// Refuses missing/extra groups, overlapping assignments or a different selected union.
    pub fn search_answer(&self, answer: &Answer) -> Result<SearchAnswer> {
        let Answer::Select {
            selected,
            groups,
            roles,
        } = answer
        else {
            return Err(ProjectionError::Unavailable);
        };
        if !roles.is_empty() {
            return Err(ProjectionError::Unavailable);
        }
        let selected = self.decode(selected)?;
        match (self.selection.group_assignment, groups) {
            (GroupAssignment::Existence, None) => Ok(SearchAnswer::Ungrouped(selected)),
            (GroupAssignment::Explicit, Some(groups)) => {
                if groups.len() != self.selection.groups.len() {
                    return Err(ProjectionError::Unavailable);
                }
                let mut assigned = BTreeSet::new();
                let mut result = BTreeMap::new();
                for group in groups {
                    let Some(expected) = self
                        .selection
                        .groups
                        .iter()
                        .find(|expected| expected.group == group.group)
                    else {
                        return Err(ProjectionError::Unavailable);
                    };
                    if group
                        .selected
                        .iter()
                        .any(|id| !expected.eligible.contains(id))
                    {
                        return Err(ProjectionError::Unavailable);
                    }
                    let members = self.decode(&group.selected)?;
                    if members
                        .iter()
                        .any(|value| !selected.contains(value) || !assigned.insert(*value))
                        || result.insert(group.group, members).is_some()
                    {
                        return Err(ProjectionError::Unavailable);
                    }
                }
                if assigned.len() != selected.len() {
                    return Err(ProjectionError::Unavailable);
                }
                Ok(SearchAnswer::Grouped(result.into_values().collect()))
            }
            (GroupAssignment::Existence, Some(_)) | (GroupAssignment::Explicit, None) => {
                Err(ProjectionError::Unavailable)
            }
        }
    }

    fn empty(cardinality: Cardinality) -> Result<Self> {
        Ok(Self {
            selection: Selection {
                candidates: Vec::new(),
                shown: Vec::new(),
                bounds: bounds(cardinality)?,
                groups: Vec::new(),
                group_assignment: GroupAssignment::Existence,
                roles: Vec::new(),
                distinct: Vec::new(),
                original_cost_total: CostTotal {
                    min: None,
                    max: None,
                },
                capacity: None,
            },
            coordinates: BTreeMap::new(),
        })
    }

    fn push(
        &mut self,
        question: &QuestionId,
        coordinate: u64,
        details: CandidateDetails,
        names: Option<Vec<String>>,
        cost: Option<i64>,
    ) -> Result<()> {
        if self.coordinates.values().any(|value| *value == coordinate) {
            return Err(ProjectionError::Unavailable);
        }
        let id = OptionId::new(format!("{}:c{}", question.as_str(), self.coordinates.len()));
        self.coordinates.insert(id.clone(), coordinate);
        self.selection.candidates.push(Candidate {
            id,
            subject: details.subject,
            controller: details.controller,
            origin: details.origin,
            rules_names: names,
            original_cost: cost.map(Int::try_from).transpose()?,
            classes: details.classes,
        });
        Ok(())
    }

    fn groups(&mut self, groups: &[crate::game::player::SelectionGroup]) -> Result<()> {
        for (index, group) in groups.iter().enumerate() {
            let mut seen = BTreeSet::new();
            let eligible = group
                .handles
                .iter()
                .map(|handle| {
                    if !seen.insert(*handle) {
                        return Err(ProjectionError::Unavailable);
                    }
                    self.coordinates
                        .iter()
                        .find(|(_, coordinate)| **coordinate == *handle)
                        .map(|(id, _)| id.clone())
                        .ok_or(ProjectionError::Unavailable)
                })
                .collect::<Result<_>>()?;
            self.selection.groups.push(SelectionGroup {
                group: Nat::try_from(index)?,
                eligible,
                bounds: bounds(group.cardinality)?,
            });
        }
        Ok(())
    }

    fn constraints(
        &mut self,
        names: bool,
        costs: bool,
        min: Option<i64>,
        max: Option<i64>,
    ) -> Result<()> {
        if self.selection.candidates.iter().any(|candidate| {
            (names && candidate.rules_names.as_ref().is_none_or(Vec::is_empty))
                || ((costs || min.is_some() || max.is_some()) && candidate.original_cost.is_none())
        }) || min.zip(max).is_some_and(|(min, max)| min > max)
        {
            return Err(ProjectionError::Unavailable);
        }
        if names {
            self.selection.distinct.push(Distinct::RulesName);
        }
        if costs {
            self.selection.distinct.push(Distinct::OriginalCost);
        }
        self.selection.original_cost_total = CostTotal {
            min: min.map(Int::try_from).transpose()?,
            max: max.map(Int::try_from).transpose()?,
        };
        Ok(())
    }

    fn decode(&self, ids: &[OptionId]) -> Result<Vec<u64>> {
        let mut seen = BTreeSet::new();
        ids.iter()
            .map(|id| {
                if !seen.insert(id) {
                    return Err(ProjectionError::Unavailable);
                }
                self.coordinates
                    .get(id)
                    .copied()
                    .ok_or(ProjectionError::Unavailable)
            })
            .collect()
    }
}

fn bounds(value: Cardinality) -> Result<Bounds> {
    if value.maximum.is_some_and(|max| max.0 < value.minimum.0) {
        return Err(ProjectionError::Unavailable);
    }
    Ok(Bounds {
        min: Nat::try_from(value.minimum.0)?,
        max: value
            .maximum
            .map(|value| Nat::try_from(value.0))
            .transpose()?,
    })
}

const fn seat(viewer: Viewer) -> Seat {
    match viewer {
        Viewer::P1 => Seat::P1,
        Viewer::P2 => Seat::P2,
    }
}
