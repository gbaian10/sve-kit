use super::PolicyFailure;
use crate::game::player::{
    DamageShare, DistributionQuestion, SearchAnswer, SearchQuestion, SelectionGroup,
    SelectionQuestion, SelectionRole, TargetRoleAssignment,
};
use crate::ir::{Cardinality, Nat, RoleCardinality};
use alloc::collections::BTreeSet;
use core::mem;

/// A complete selected set and, when requested, every role's disjoint subset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionAnswer {
    /// Original question coordinates in candidate order.
    pub targets: Vec<u64>,
    /// Empty exactly when the question has no roles; otherwise includes empty roles.
    pub roles: Vec<TargetRoleAssignment>,
}

/// Choose a complete legal selection using only frozen public constraints.
/// Prefers the smallest nonempty answer, then candidate order; empty is the fallback.
///
/// # Errors
/// Returns malformed-question, exhaustive-no-solution or search-budget errors separately.
pub fn select(question: &SelectionQuestion) -> Result<SelectionAnswer, PolicyFailure> {
    let needs_cost = question.distinct_original_costs
        || question.minimum_original_cost.is_some()
        || question.maximum_original_cost.is_some();
    let candidates = question
        .members
        .iter()
        .map(|member| {
            let cost = question
                .original_costs
                .as_ref()
                .and_then(|costs| costs.get(&member.handle))
                .copied();
            if needs_cost && (cost.is_none() || cost != member.original_cost) {
                return Err(PolicyFailure::InvalidQuestion);
            }
            Ok(Candidate {
                handle: member.handle,
                names: member.rules_names.as_deref(),
                cost: if needs_cost { cost } else { None },
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if question.groups.is_empty() {
        return Err(PolicyFailure::InvalidQuestion);
    }
    let problem = Problem {
        candidates,
        bounds: question.cardinality,
        groups: &question.groups,
        roles: &question.roles,
        distinct_names: question.distinct_names,
        distinct_costs: question.distinct_original_costs,
        floor: question.minimum_original_cost,
        cap: question.maximum_original_cost,
    };
    let solution = problem.solve()?;
    Ok(SelectionAnswer {
        targets: solution.targets,
        roles: solution.roles,
    })
}

/// Select all search groups together, preserving their printed order and empty rows.
///
/// # Errors
/// Returns malformed-question, exhaustive-no-solution or search-budget errors separately.
pub fn search(question: &SearchQuestion) -> Result<SearchAnswer, PolicyFailure> {
    if question.groups.as_ref().is_some_and(Vec::is_empty) {
        return Err(PolicyFailure::InvalidQuestion);
    }
    let problem = Problem {
        candidates: question
            .cards
            .iter()
            .map(|card| Candidate {
                handle: card.handle,
                names: card.rules_names.as_deref(),
                cost: card.original_cost,
            })
            .collect(),
        bounds: question.cardinality,
        groups: question.groups.as_deref().unwrap_or_default(),
        roles: &[],
        distinct_names: question.distinct_names,
        distinct_costs: false,
        floor: None,
        cap: question.maximum_original_cost,
    };
    let solution = problem.solve()?;
    Ok(if question.groups.is_some() {
        SearchAnswer::Grouped(solution.groups)
    } else {
        SearchAnswer::Ungrouped(solution.targets)
    })
}

/// Give every recipient its minimum, then the first recipient the remaining amount.
/// Empty membership skips allocation regardless of total, as in the native procedure.
///
/// # Errors
/// Duplicate coordinates are malformed; an insufficient total is infeasible.
pub fn distribute(question: &DistributionQuestion) -> Result<Vec<DamageShare>, PolicyFailure> {
    let mut seen = BTreeSet::new();
    if question
        .members
        .iter()
        .any(|member| !seen.insert(member.handle))
    {
        return Err(PolicyFailure::InvalidQuestion);
    }
    let count =
        u64::try_from(question.members.len()).map_err(|_error| PolicyFailure::InvalidQuestion)?;
    if count == 0 {
        return Ok(Vec::new());
    }
    let required = question
        .minimum
        .0
        .checked_mul(count)
        .ok_or(PolicyFailure::NoSolution)?;
    let remainder = question
        .total
        .0
        .checked_sub(required)
        .ok_or(PolicyFailure::NoSolution)?;
    question
        .members
        .iter()
        .enumerate()
        .map(|(index, member)| {
            let amount = if index == 0 {
                question
                    .minimum
                    .0
                    .checked_add(remainder)
                    .ok_or(PolicyFailure::NoSolution)?
            } else {
                question.minimum.0
            };
            Ok(DamageShare {
                handle: member.handle,
                amount: Nat(amount),
            })
        })
        .collect()
}

struct Candidate<'question> {
    handle: u64,
    names: Option<&'question [String]>,
    cost: Option<i64>,
}

struct Problem<'question> {
    candidates: Vec<Candidate<'question>>,
    bounds: Cardinality,
    groups: &'question [SelectionGroup],
    roles: &'question [SelectionRole],
    distinct_names: bool,
    distinct_costs: bool,
    floor: Option<i64>,
    cap: Option<i64>,
}

struct Solution {
    targets: Vec<u64>,
    groups: Vec<Vec<u64>>,
    roles: Vec<TargetRoleAssignment>,
}

struct Budget(u64);
impl Budget {
    fn spend(&mut self) -> Result<(), PolicyFailure> {
        self.0 = self
            .0
            .checked_sub(1)
            .ok_or(PolicyFailure::SearchExhausted)?;
        Ok(())
    }
}

fn valid_bounds(bounds: Cardinality) -> bool {
    bounds
        .maximum
        .is_none_or(|maximum| bounds.minimum.0 <= maximum.0)
}

fn limits(bounds: Cardinality, count: usize) -> Result<(usize, usize), PolicyFailure> {
    let minimum = usize::try_from(bounds.minimum.0).map_err(|_error| PolicyFailure::NoSolution)?;
    let maximum = bounds.maximum.map_or(count, |value| {
        usize::try_from(value.0).unwrap_or(usize::MAX).min(count)
    });
    Ok((minimum, maximum))
}

impl Problem<'_> {
    fn validate(&self) -> Result<(), PolicyFailure> {
        let mut handles = BTreeSet::new();
        if !valid_bounds(self.bounds)
            || self
                .floor
                .zip(self.cap)
                .is_some_and(|(floor, cap)| floor > cap)
            || self.candidates.iter().any(|candidate| {
                !handles.insert(candidate.handle)
                    || (self.distinct_names && candidate.names.is_none())
                    || ((self.floor.is_some() || self.cap.is_some() || self.distinct_costs)
                        && candidate.cost.is_none())
            })
        {
            return Err(PolicyFailure::InvalidQuestion);
        }
        for group in self.groups {
            let mut seen = BTreeSet::new();
            if !valid_bounds(group.cardinality)
                || group
                    .handles
                    .iter()
                    .any(|handle| !handles.contains(handle) || !seen.insert(*handle))
            {
                return Err(PolicyFailure::InvalidQuestion);
            }
        }
        let mut roles = BTreeSet::new();
        let mut remainder = false;
        for role in self.roles {
            let valid = match role.cardinality {
                RoleCardinality::Fixed { cardinality } => valid_bounds(cardinality),
                RoleCardinality::IfNonempty { count } => count == Nat(1),
                RoleCardinality::Remainder {} => !mem::replace(&mut remainder, true),
            };
            if !roles.insert(role.role.0) || !valid {
                return Err(PolicyFailure::InvalidQuestion);
            }
        }
        Ok(())
    }

    fn solve(&self) -> Result<Solution, PolicyFailure> {
        self.validate()?;
        let (minimum, maximum) = limits(self.bounds, self.candidates.len())?;
        let mut budget = Budget(100_000);
        for count in minimum.max(1)..=maximum {
            if let Some(solution) = self.with_count(count, &mut budget)? {
                return Ok(solution);
            }
        }
        if minimum == 0
            && let Some(solution) = self.with_count(0, &mut budget)?
        {
            return Ok(solution);
        }
        Err(PolicyFailure::NoSolution)
    }

    fn with_count(
        &self,
        count: usize,
        budget: &mut Budget,
    ) -> Result<Option<Solution>, PolicyFailure> {
        let Some(role_counts) = role_counts(self.roles, count) else {
            return Ok(None);
        };
        let mut group_minimum = 0_usize;
        let mut group_maximum = 0_usize;
        for group in self.groups {
            let (minimum, maximum) = limits(group.cardinality, group.handles.len())?;
            if minimum > maximum {
                return Ok(None);
            }
            group_minimum = group_minimum.saturating_add(minimum);
            group_maximum = group_maximum.saturating_add(maximum);
        }
        if !self.groups.is_empty() && (count < group_minimum || count > group_maximum) {
            return Ok(None);
        }
        let mut selected = Vec::with_capacity(count);
        let mut cursor = 0;
        // Iterative backtracking keeps large candidate domains off the call stack.
        loop {
            budget.spend()?;
            if selected.len() == count
                && let Some(solution) = self.complete(&selected, &role_counts, budget)?
            {
                return Ok(Some(solution));
            }
            if selected.len() < count && self.reachable(&selected, cursor, count) {
                let Some(candidate) = self.candidates.get(cursor) else {
                    return Ok(None);
                };
                if self.compatible(&selected, candidate) {
                    selected.push(cursor);
                }
                cursor = cursor.saturating_add(1);
                continue;
            }
            let Some(previous) = selected.pop() else {
                return Ok(None);
            };
            cursor = previous.saturating_add(1);
        }
    }

    fn eligible(&self, candidate: &Candidate<'_>) -> bool {
        self.groups.is_empty()
            || self
                .groups
                .iter()
                .any(|group| group.handles.contains(&candidate.handle))
    }

    fn compatible(&self, selected: &[usize], candidate: &Candidate<'_>) -> bool {
        self.eligible(candidate)
            && selected.iter().all(|index| {
                self.candidates.get(*index).is_some_and(|other| {
                    (!self.distinct_costs || candidate.cost != other.cost)
                        && (!self.distinct_names
                            || !candidate
                                .names
                                .unwrap_or_default()
                                .iter()
                                .any(|name| other.names.is_some_and(|names| names.contains(name))))
                })
            })
    }

    fn reachable(&self, selected: &[usize], cursor: usize, count: usize) -> bool {
        let needed = count.saturating_sub(selected.len());
        let remaining: Vec<_> = self
            .candidates
            .get(cursor..)
            .unwrap_or_default()
            .iter()
            .filter(|candidate| self.eligible(candidate))
            .collect();
        if remaining.len() < needed {
            return false;
        }
        if self.floor.is_none() && self.cap.is_none() {
            return true;
        }
        let sum: i128 = selected
            .iter()
            .filter_map(|index| self.candidates.get(*index))
            .filter_map(|candidate| candidate.cost)
            .map(i128::from)
            .sum();
        let mut costs: Vec<_> = remaining
            .iter()
            .filter_map(|candidate| candidate.cost)
            .collect();
        costs.sort_unstable();
        let smallest: i128 = costs.iter().take(needed).copied().map(i128::from).sum();
        let largest: i128 = costs
            .iter()
            .rev()
            .take(needed)
            .copied()
            .map(i128::from)
            .sum();
        self.floor
            .is_none_or(|floor| sum.saturating_add(largest) >= i128::from(floor))
            && self
                .cap
                .is_none_or(|cap| sum.saturating_add(smallest) <= i128::from(cap))
    }

    fn complete(
        &self,
        selected: &[usize],
        counts: &[usize],
        budget: &mut Budget,
    ) -> Result<Option<Solution>, PolicyFailure> {
        let candidates: Vec<_> = selected
            .iter()
            .filter_map(|index| self.candidates.get(*index))
            .collect();
        if self.floor.is_some() || self.cap.is_some() {
            let sum = candidates
                .iter()
                .try_fold(0_i64, |sum, candidate| sum.checked_add(candidate.cost?));
            let Some(sum) = sum else {
                return Ok(None);
            };
            if self.floor.is_some_and(|floor| sum < floor) || self.cap.is_some_and(|cap| sum > cap)
            {
                return Ok(None);
            }
        }
        let targets: Vec<_> = candidates
            .iter()
            .map(|candidate| candidate.handle)
            .collect();
        let Some(groups) = assign_groups(&targets, self.groups, budget)? else {
            return Ok(None);
        };
        let mut remaining = targets.as_slice();
        let mut roles = Vec::new();
        for (role, count) in self.roles.iter().zip(counts) {
            let Some((assigned, rest)) = remaining.split_at_checked(*count) else {
                return Err(PolicyFailure::InvalidQuestion);
            };
            roles.push(TargetRoleAssignment {
                role: role.role,
                targets: assigned.to_vec(),
            });
            remaining = rest;
        }
        Ok(Some(Solution {
            targets,
            groups,
            roles,
        }))
    }
}

fn role_counts(roles: &[SelectionRole], count: usize) -> Option<Vec<usize>> {
    if roles.is_empty() {
        return Some(Vec::new());
    }
    let mut ranges = Vec::new();
    let mut required = 0_usize;
    for role in roles {
        let (minimum, maximum) = match role.cardinality {
            RoleCardinality::Fixed { cardinality } => limits(cardinality, count).ok()?,
            RoleCardinality::IfNonempty { .. } => {
                let nonempty_count = usize::from(count > 0);
                (nonempty_count, nonempty_count)
            }
            RoleCardinality::Remainder {} => (0, count),
        };
        if minimum > maximum {
            return None;
        }
        required = required.checked_add(minimum)?;
        ranges.push((minimum, maximum));
    }
    let mut left = count.checked_sub(required)?;
    let counts = ranges
        .into_iter()
        .map(|(minimum, maximum)| {
            let extra = left.min(maximum.saturating_sub(minimum));
            left = left.saturating_sub(extra);
            minimum.saturating_add(extra)
        })
        .collect();
    (left == 0).then_some(counts)
}

fn assign_groups(
    targets: &[u64],
    groups: &[SelectionGroup],
    budget: &mut Budget,
) -> Result<Option<Vec<Vec<u64>>>, PolicyFailure> {
    if groups.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let mut rows = vec![Vec::new(); groups.len()];
    let mut assigned = Vec::<usize>::new();
    let mut next_group = 0;
    loop {
        budget.spend()?;
        let index = assigned.len();
        if index == targets.len() {
            if groups.iter().zip(&rows).all(|(group, row)| {
                u64::try_from(row.len()).is_ok_and(|count| count >= group.cardinality.minimum.0)
            }) {
                return Ok(Some(rows));
            }
        } else {
            let handle = targets.get(index).ok_or(PolicyFailure::InvalidQuestion)?;
            let choice =
                groups
                    .iter()
                    .zip(&rows)
                    .enumerate()
                    .skip(next_group)
                    .find(|(_, (group, row))| {
                        group.handles.contains(handle)
                            && group.cardinality.maximum.is_none_or(|maximum| {
                                u64::try_from(row.len()).is_ok_and(|count| count < maximum.0)
                            })
                    });
            if let Some((group, _)) = choice {
                rows.get_mut(group)
                    .ok_or(PolicyFailure::InvalidQuestion)?
                    .push(*handle);
                assigned.push(group);
                next_group = 0;
                continue;
            }
        }
        let Some(previous) = assigned.pop() else {
            return Ok(None);
        };
        rows.get_mut(previous)
            .ok_or(PolicyFailure::InvalidQuestion)?
            .pop();
        next_group = previous.saturating_add(1);
    }
}
