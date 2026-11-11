//! A tiny fixture policy that derives complete answers from public constraints only.

use super::*;
use alloc::collections::BTreeSet;
use sve_engine::game::internal::SearchAnswer;
use sve_engine::game::player::{SearchQuestion, SelectionGroup, SelectionQuestion};

fn within(bounds: Cardinality, count: usize) -> bool {
    let count = u64::try_from(count).unwrap();
    count >= bounds.minimum.0 && bounds.maximum.is_none_or(|max| count <= max.0)
}

fn assign(groups: &[SelectionGroup], selected: &[u64], rows: &mut [Vec<u64>]) -> bool {
    let Some((handle, tail)) = selected.split_first() else {
        return groups
            .iter()
            .zip(rows)
            .all(|(group, row)| within(group.cardinality, row.len()));
    };
    for index in 0..groups.len() {
        if groups[index].handles.contains(handle) {
            rows[index].push(*handle);
            if assign(groups, tail, rows) {
                return true;
            }
            rows[index].pop();
        }
    }
    false
}

fn subsets(handles: &[u64]) -> impl DoubleEndedIterator<Item = Vec<u64>> + '_ {
    assert!(
        handles.len() <= 16,
        "Fixture enumeration is deliberately bounded"
    );
    (0..1_usize
        .checked_shl(u32::try_from(handles.len()).unwrap())
        .unwrap())
        .map(|mask| {
            handles
                .iter()
                .enumerate()
                .filter_map(move |(index, handle)| {
                    (mask & 1_usize.checked_shl(u32::try_from(index).unwrap()).unwrap() != 0)
                        .then_some(*handle)
                })
                .collect()
        })
}

pub(crate) fn choose(question: &SelectionQuestion) -> Vec<u64> {
    let handles = question
        .members
        .iter()
        .map(|member| member.handle)
        .collect::<Vec<_>>();
    subsets(&handles)
        .rev()
        .find(|selected| {
            if !within(question.cardinality, selected.len()) {
                return false;
            }
            let mut names = BTreeSet::new();
            let mut costs = BTreeSet::new();
            let mut total = 0_i64;
            for member in question
                .members
                .iter()
                .filter(|member| selected.contains(&member.handle))
            {
                if question.distinct_names
                    && !member
                        .rules_names
                        .as_ref()
                        .unwrap()
                        .iter()
                        .all(|name| names.insert(name))
                {
                    return false;
                }
                if let Some(cost) = member.original_cost {
                    if question.distinct_original_costs && !costs.insert(cost) {
                        return false;
                    }
                    total = total.checked_add(cost).unwrap();
                }
            }
            question
                .minimum_original_cost
                .is_none_or(|min| total >= min)
                && question
                    .maximum_original_cost
                    .is_none_or(|max| total <= max)
                && assign(
                    &question.groups,
                    selected,
                    &mut vec![vec![]; question.groups.len()],
                )
        })
        .unwrap()
}

pub(crate) fn search(question: &SearchQuestion) -> Answer {
    let handles = question
        .cards
        .iter()
        .map(|card| card.handle)
        .collect::<Vec<_>>();
    for selected in subsets(&handles).rev() {
        if !within(question.cardinality, selected.len()) {
            continue;
        }
        let mut names = BTreeSet::new();
        let mut total = 0_i64;
        let mut valid = true;
        for card in question
            .cards
            .iter()
            .filter(|card| selected.contains(&card.handle))
        {
            if question.distinct_names
                && !card
                    .rules_names
                    .as_ref()
                    .unwrap()
                    .iter()
                    .all(|name| names.insert(name))
            {
                valid = false;
            }
            if let Some(cost) = card.original_cost {
                total = total.checked_add(cost).unwrap();
            }
        }
        if !valid
            || question
                .maximum_original_cost
                .is_some_and(|max| total > max)
        {
            continue;
        }
        let answer = if let Some(groups) = &question.groups {
            let mut rows = vec![vec![]; groups.len()];
            if !assign(groups, &selected, &mut rows) {
                continue;
            }
            SearchAnswer::Grouped(rows)
        } else {
            SearchAnswer::Ungrouped(selected)
        };
        return Answer::Search(answer);
    }
    panic!("Public search constraints must describe an answer");
}

pub(super) fn search_selected(question: &SearchQuestion, selected: &[u64]) -> Answer {
    let result = question.groups.as_ref().map_or_else(
        || SearchAnswer::Ungrouped(selected.to_vec()),
        |groups| {
            let mut rows = vec![vec![]; groups.len()];
            assert!(
                assign(groups, selected, &mut rows),
                "The chosen set must admit a disjoint group assignment"
            );
            SearchAnswer::Grouped(rows)
        },
    );
    Answer::Search(result)
}
