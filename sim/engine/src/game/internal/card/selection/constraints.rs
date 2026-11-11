use crate::game::internal::{Result, contains, invalid};
use crate::ir::{Cardinality, Nat};
use alloc::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::game::internal) enum PropertyKey<'property> {
    Name(&'property str),
    OriginalCost(i64),
}

#[derive(Debug, Default)]
pub(in crate::game::internal) struct DistinctProperties<'property> {
    seen: BTreeSet<PropertyKey<'property>>,
}

impl<'property> DistinctProperties<'property> {
    pub(in crate::game::internal) fn insert_member(
        &mut self,
        keys: impl IntoIterator<Item = PropertyKey<'property>>,
    ) -> bool {
        let own: BTreeSet<_> = keys.into_iter().collect();
        if !self.seen.is_disjoint(&own) {
            return false;
        }
        self.seen.extend(own);
        true
    }
}

pub(in crate::game::internal) fn check_count(
    bounds: Cardinality,
    count: usize,
    message: &'static str,
) -> Result<()> {
    if !contains(bounds, Nat(u64::try_from(count).map_err(invalid)?)) {
        return Err(invalid(message));
    }
    Ok(())
}

pub(in crate::game::internal) fn unique_handles<T>(
    handles: &[u64],
    duplicate: &'static str,
    mut resolve: impl FnMut(u64) -> Result<T>,
) -> Result<Vec<T>> {
    let mut seen = BTreeSet::new();
    handles
        .iter()
        .map(|&handle| {
            if !seen.insert(handle) {
                return Err(invalid(duplicate));
            }
            resolve(handle)
        })
        .collect()
}

pub(in crate::game::internal) fn within_original_cost_cap(
    costs: impl IntoIterator<Item = Result<i64>>,
    limit: i64,
    overflow: &'static str,
) -> Result<bool> {
    let total = costs.into_iter().try_fold(0_i64, |sum, cost| {
        sum.checked_add(cost?).ok_or_else(|| invalid(overflow))
    })?;
    Ok(total <= limit)
}
