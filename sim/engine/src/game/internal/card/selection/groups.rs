//! Frozen membership gives validation and affordability the same disjoint partition.

use crate::game::internal::{Result, contains, invalid, unsupported};
use crate::ir::{Cardinality, Nat, ObjectRef};
use alloc::collections::BTreeSet;
use core::array::from_fn;

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct GroupConstraints {
    pub(in crate::game::internal) members: Vec<Vec<ObjectRef>>,
    pub(in crate::game::internal) bounds: Vec<Cardinality>,
}

impl GroupConstraints {
    pub(in crate::game::internal) fn accepts(&self, members: &[ObjectRef]) -> bool {
        let mut unique = BTreeSet::new();
        if self.members.len() != self.bounds.len()
            || !members
                .iter()
                .all(|member| unique.insert((member.id, member.generation)))
        {
            return false;
        }
        if let ([group], [bounds]) = (self.members.as_slice(), self.bounds.as_slice()) {
            return u64::try_from(members.len()).is_ok_and(|size| contains(*bounds, Nat(size)))
                && members.iter().all(|member| group.contains(member));
        }
        self.assign(members, 0, &mut vec![0; self.members.len()])
    }

    fn assign(&self, members: &[ObjectRef], index: usize, counts: &mut [u64]) -> bool {
        if index == members.len() {
            return self.bounds.iter().enumerate().all(|(group, bound)| {
                bound.minimum.0 <= counts[group]
                    && bound
                        .maximum
                        .is_none_or(|maximum| counts[group] <= maximum.0)
            });
        }
        for (group, candidates) in self.members.iter().enumerate() {
            if !candidates.contains(&members[index])
                || self.bounds[group]
                    .maximum
                    .is_some_and(|maximum| counts[group] >= maximum.0)
            {
                continue;
            }
            counts[group] = counts[group].saturating_add(1);
            if self.assign(members, index.saturating_add(1), counts) {
                return true;
            }
            counts[group] = counts[group].saturating_sub(1);
        }
        false
    }

    pub(in crate::game::internal) fn capacity(&self) -> Result<u64> {
        let candidates = self.members.iter().flatten().copied().collect::<Vec<_>>();
        self.assignments(&candidates)?
            .into_iter()
            .enumerate()
            .filter(|(mask, choice)| choice.is_some() && self.complete(*mask))
            .map(|(_, choice)| {
                u64::try_from(choice.map_or(0, |members| members.len())).map_err(invalid)
            })
            .try_fold(0, |maximum, count| Ok(maximum.max(count?)))
    }

    pub(in crate::game::internal) fn witness(
        &self,
        candidates: &[ObjectRef],
        count: Cardinality,
    ) -> Result<Option<Vec<ObjectRef>>> {
        if let ([group], [bounds]) = (self.members.as_slice(), self.bounds.as_slice()) {
            let least = usize::try_from(count.minimum.0.max(bounds.minimum.0)).map_err(invalid)?;
            let mut unique = BTreeSet::new();
            let members = candidates
                .iter()
                .copied()
                .filter(|member| {
                    group.contains(member) && unique.insert((member.id, member.generation))
                })
                .take(least)
                .collect::<Vec<_>>();
            return Ok(
                (contains(count, Nat(u64::try_from(members.len()).map_err(invalid)?))
                    && self.accepts(&members))
                .then_some(members),
            );
        }
        Ok(self
            .assignments(candidates)?
            .into_iter()
            .enumerate()
            .filter(|(mask, _)| self.complete(*mask))
            .filter_map(|(_, choice)| choice)
            .filter(|members| {
                u64::try_from(members.len()).is_ok_and(|size| contains(count, Nat(size)))
            })
            .min_by_key(Vec::len))
    }

    fn complete(&self, mask: usize) -> bool {
        self.bounds
            .iter()
            .enumerate()
            .all(|(group, bound)| bound.minimum == Nat(0) || mask & (1 << group) != 0)
    }

    fn assignments(&self, candidates: &[ObjectRef]) -> Result<[Option<Vec<ObjectRef>>; 8]> {
        if !matches!(self.members.len(), 2 | 3)
            || self.bounds.len() != self.members.len()
            || self
                .bounds
                .iter()
                .any(|bound| bound.maximum != Some(Nat(1)) || bound.minimum.0 > 1)
        {
            return Err(unsupported("group capacity bounds"));
        }
        let mut reachable: [Option<Vec<ObjectRef>>; 8] = from_fn(|_| None);
        reachable[0] = Some(Vec::new());
        let mut seen = BTreeSet::new();
        for member in candidates {
            if !seen.insert((member.id, member.generation)) {
                continue;
            }
            // Read only the prior states so one physical member never fills two groups.
            let previous = reachable.clone();
            for (mask, choice) in previous.iter().enumerate() {
                let Some(choice) = choice else {
                    continue;
                };
                for (group, eligible) in self.members.iter().enumerate() {
                    let next = mask | (1 << group);
                    if mask & (1 << group) == 0
                        && eligible.contains(member)
                        && reachable[next].is_none()
                    {
                        let mut chosen = choice.clone();
                        chosen.push(*member);
                        reachable[next] = Some(chosen);
                    }
                }
            }
        }
        Ok(reachable)
    }
}

#[cfg(test)]
mod tests;
