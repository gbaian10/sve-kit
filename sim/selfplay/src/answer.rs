//! A random answer inside the domain a wire question states.
//!
//! Only the question is read: an answer the stated constraints allow and the host still
//! refuses is a finding about the question, not about this player.

use alloc::collections::BTreeMap;
use rand::RngExt as _;
use rand::seq::{IndexedRandom as _, SliceRandom as _};
use rand_chacha::ChaCha8Rng;
use sve_battle_contract::question::{
    Bounds, Candidate, Distinct, EmptyDistribution, GroupAssignment, MainIntent, PartitionBucket,
    Question, QuestionShape, RoleSize, Selection, SelectionGroup, SelectionRole,
};
use sve_battle_contract::{Answer, BucketChoice, GroupChoice, Nat, OptionId, RoleChoice, Share};

/// Random attempts before the exhaustive search of a selection.
const SAMPLES: usize = 64;
/// Largest candidate count the exhaustive search covers.
const EXHAUSTIVE: usize = 16;

/// Why no answer was produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoAnswer {
    /// Every answer of the stated domain was tried: the question cannot be answered.
    Proven,
    /// Sampling found none and the domain is too large to enumerate.
    NotFound,
}

fn count(value: Nat) -> usize {
    usize::try_from(value.get()).unwrap_or(usize::MAX)
}

/// The inclusive range of sizes `bounds` allows among `available` items.
fn sizes(bounds: Bounds, available: usize) -> Option<(usize, usize)> {
    let minimum = count(bounds.min);
    let maximum = bounds.max.map_or(available, count).min(available);
    (minimum <= maximum).then_some((minimum, maximum))
}

fn shares_property(first: &Candidate, second: &Candidate, property: Distinct) -> bool {
    match property {
        Distinct::RulesName => first.rules_names.as_ref().is_some_and(|names| {
            second
                .rules_names
                .as_ref()
                .is_some_and(|others| names.iter().any(|name| others.contains(name)))
        }),
        Distinct::OriginalCost => {
            first.original_cost.is_some() && first.original_cost == second.original_cost
        }
        Distinct::Class => first.classes.as_ref().is_some_and(|classes| {
            second
                .classes
                .as_ref()
                .is_some_and(|others| classes.iter().any(|class| others.contains(class)))
        }),
    }
}

fn compatible(selection: &Selection, chosen: &[&Candidate], next: &Candidate) -> bool {
    selection.distinct.iter().all(|property| {
        chosen
            .iter()
            .all(|member| !shares_property(member, next, *property))
    })
}

fn cost_within(selection: &Selection, chosen: &[&Candidate]) -> bool {
    let total = chosen
        .iter()
        .filter_map(|member| member.original_cost)
        .fold(0_i64, |sum, cost| sum.saturating_add(cost.get()));
    let limits = selection.original_cost_total;
    limits.min.is_none_or(|minimum| total >= minimum.get())
        && limits.max.is_none_or(|maximum| total <= maximum.get())
}

/// Place the remaining candidates one by one, each in an eligible group with room, and accept
/// only when every group reached its minimum.
fn place(groups: &[SelectionGroup], rest: &[&Candidate], members: &mut [Vec<OptionId>]) -> bool {
    let missing = groups
        .iter()
        .zip(&*members)
        .map(|(group, placed)| count(group.bounds.min).saturating_sub(placed.len()))
        .fold(0_usize, usize::saturating_add);
    // Too few candidates are left to fill the minimums still open.
    if missing > rest.len() {
        return false;
    }
    let Some((candidate, later)) = rest.split_first() else {
        return true;
    };
    for (index, group) in groups.iter().enumerate() {
        let full = group.bounds.max.is_some_and(|maximum| {
            members
                .get(index)
                .is_none_or(|placed| placed.len() >= count(maximum))
        });
        if full || !group.eligible.contains(&candidate.id) {
            continue;
        }
        if let Some(placed) = members.get_mut(index) {
            placed.push(candidate.id.clone());
        }
        if place(groups, later, members) {
            return true;
        }
        if let Some(placed) = members.get_mut(index) {
            placed.pop();
        }
    }
    false
}

/// A disjoint assignment of every chosen candidate to an eligible group within each group's
/// bounds, found by exhaustive backtracking: selections are small.
fn assign_groups(groups: &[SelectionGroup], chosen: &[&Candidate]) -> Option<Vec<Vec<OptionId>>> {
    let mut members = vec![Vec::new(); groups.len()];
    place(groups, chosen, &mut members).then_some(members)
}

/// Sizes for every role that partition `total` selected candidates.
fn role_sizes(roles: &[SelectionRole], total: usize, rng: &mut ChaCha8Rng) -> Option<Vec<usize>> {
    if roles.is_empty() {
        return Some(Vec::new());
    }
    let mut sizes = vec![0_usize; roles.len()];
    let mut left = total;
    let mut order: Vec<usize> = (0..roles.len()).collect();
    order.shuffle(rng);
    // Fixed minimums and conditional counts first, then spread what is left.
    for index in &order {
        let need = match roles.get(*index)?.size {
            RoleSize::Fixed { bounds } => count(bounds.min),
            RoleSize::IfNonempty { count: required } => {
                if total == 0 {
                    0
                } else {
                    count(required)
                }
            }
            RoleSize::Remainder => 0,
        };
        left = left.checked_sub(need)?;
        *sizes.get_mut(*index)? = need;
    }
    for index in &order {
        if let RoleSize::Fixed { bounds } = roles.get(*index)?.size {
            let size = sizes.get_mut(*index)?;
            let room = bounds.max.map_or(left, |maximum| {
                count(maximum).saturating_sub(*size).min(left)
            });
            let extra = rng.random_range(0..=room);
            *size = size.saturating_add(extra);
            left = left.saturating_sub(extra);
        }
    }
    let remainders: Vec<usize> = order
        .iter()
        .copied()
        .filter(|index| {
            roles
                .get(*index)
                .is_some_and(|role| role.size == RoleSize::Remainder)
        })
        .collect();
    if let Some(first) = remainders.first() {
        *sizes.get_mut(*first)? = left;
        left = 0;
    }
    // Leftover with every fixed role at a random size: push it into roles that still have room.
    for index in &order {
        if left == 0 {
            break;
        }
        if let RoleSize::Fixed { bounds } = roles.get(*index)?.size {
            let size = sizes.get_mut(*index)?;
            let room = bounds
                .max
                .map_or(left, |maximum| count(maximum).saturating_sub(*size))
                .min(left);
            *size = size.saturating_add(room);
            left = left.saturating_sub(room);
        }
    }
    (left == 0).then_some(sizes)
}

struct Selected {
    ids: Vec<OptionId>,
    groups: Option<Vec<GroupChoice>>,
    roles: Vec<RoleChoice>,
}

/// Check one candidate set against every stated constraint and complete its answer.
fn complete(
    selection: &Selection,
    chosen: &[&Candidate],
    rng: &mut ChaCha8Rng,
) -> Option<Selected> {
    let (minimum, maximum) = sizes(selection.bounds, selection.candidates.len())?;
    if chosen.len() < minimum || chosen.len() > maximum || !cost_within(selection, chosen) {
        return None;
    }
    for (index, member) in chosen.iter().enumerate() {
        if !compatible(selection, chosen.get(..index)?, member) {
            return None;
        }
    }
    let groups = if selection.groups.is_empty() {
        None
    } else {
        Some(assign_groups(&selection.groups, chosen)?)
    };
    let mut ids: Vec<OptionId> = chosen.iter().map(|member| member.id.clone()).collect();
    let sizes = role_sizes(&selection.roles, ids.len(), rng)?;
    ids.shuffle(rng);
    let mut rest = ids.iter();
    let roles = selection
        .roles
        .iter()
        .zip(sizes)
        .map(|(role, size)| RoleChoice {
            role: role.role,
            selected: rest.by_ref().take(size).cloned().collect(),
        })
        .collect();
    let groups = match (selection.group_assignment, groups) {
        (GroupAssignment::Explicit, Some(assigned)) => Some(
            selection
                .groups
                .iter()
                .zip(assigned)
                .map(|(group, selected)| GroupChoice {
                    group: group.group,
                    selected,
                })
                .collect(),
        ),
        (GroupAssignment::Explicit, None) => Some(Vec::new()),
        (GroupAssignment::Existence, _) => None,
    };
    Some(Selected { ids, groups, roles })
}

fn sample(selection: &Selection, rng: &mut ChaCha8Rng) -> Option<Selected> {
    let (minimum, maximum) = sizes(selection.bounds, selection.candidates.len())?;
    let target = rng.random_range(minimum..=maximum);
    let mut order: Vec<&Candidate> = selection.candidates.iter().collect();
    order.shuffle(rng);
    let mut chosen: Vec<&Candidate> = Vec::new();
    if selection.groups.is_empty() {
        for candidate in order {
            if chosen.len() == target {
                break;
            }
            if compatible(selection, &chosen, candidate) {
                chosen.push(candidate);
            }
        }
    } else {
        // Fill each group to a random size of its own, so group minimums are met by design.
        for group in &selection.groups {
            let eligible: Vec<&Candidate> = order
                .iter()
                .copied()
                .filter(|candidate| {
                    group.eligible.contains(&candidate.id)
                        && !chosen.iter().any(|member| member.id == candidate.id)
                })
                .collect();
            let Some((low, high)) = sizes(group.bounds, eligible.len()) else {
                continue;
            };
            let take = rng.random_range(low..=high);
            for candidate in eligible {
                if chosen.len() >= maximum
                    || chosen
                        .iter()
                        .filter(|member| group.eligible.contains(&member.id))
                        .count()
                        >= take
                {
                    break;
                }
                if compatible(selection, &chosen, candidate) {
                    chosen.push(candidate);
                }
            }
        }
    }
    complete(selection, &chosen, rng)
}

fn exhaustive(selection: &Selection, rng: &mut ChaCha8Rng) -> Option<Selected> {
    let candidates = &selection.candidates;
    let mut subsets: Vec<u32> =
        (0..1_u32.checked_shl(u32::try_from(candidates.len()).ok()?)?).collect();
    subsets.shuffle(rng);
    subsets.into_iter().find_map(|mask| {
        let chosen: Vec<&Candidate> = candidates
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                u32::try_from(*index)
                    .ok()
                    .and_then(|shift| mask.checked_shr(shift))
                    .is_some_and(|bits| bits & 1 == 1)
            })
            .map(|(_, candidate)| candidate)
            .collect();
        complete(selection, &chosen, rng)
    })
}

fn select(selection: &Selection, rng: &mut ChaCha8Rng) -> Result<Selected, NoAnswer> {
    if let Some(selected) = (0..SAMPLES).find_map(|_| sample(selection, rng)) {
        return Ok(selected);
    }
    if selection.candidates.len() > EXHAUSTIVE {
        return Err(NoAnswer::NotFound);
    }
    exhaustive(selection, rng).ok_or(NoAnswer::Proven)
}

fn partition(
    items: &[Candidate],
    buckets: &[PartitionBucket],
    rng: &mut ChaCha8Rng,
) -> Result<Answer, NoAnswer> {
    let mut order: Vec<&Candidate> = items.iter().collect();
    order.shuffle(rng);
    let mut placed: BTreeMap<Nat, Vec<OptionId>> = buckets
        .iter()
        .map(|bucket| (bucket.bucket, Vec::new()))
        .collect();
    let mut rest = order.into_iter();
    for bucket in buckets {
        let slot = placed.entry(bucket.bucket).or_default();
        slot.extend(
            rest.by_ref()
                .take(count(bucket.bounds.min))
                .map(|item| item.id.clone()),
        );
        if slot.len() < count(bucket.bounds.min) {
            return Err(NoAnswer::Proven);
        }
    }
    for item in rest {
        let open: Vec<&PartitionBucket> = buckets
            .iter()
            .filter(|bucket| {
                bucket.bounds.max.is_none_or(|maximum| {
                    placed.get(&bucket.bucket).map_or(0, Vec::len) < count(maximum)
                })
            })
            .collect();
        let bucket = open.choose(rng).ok_or(NoAnswer::Proven)?;
        placed
            .entry(bucket.bucket)
            .or_default()
            .push(item.id.clone());
    }
    Ok(Answer::Partition {
        buckets: buckets
            .iter()
            .map(|bucket| BucketChoice {
                bucket: bucket.bucket,
                items: placed.remove(&bucket.bucket).unwrap_or_default(),
            })
            .collect(),
    })
}

fn distribute(
    recipients: &[Candidate],
    total: Nat,
    share: Bounds,
    empty: EmptyDistribution,
    rng: &mut ChaCha8Rng,
) -> Result<Answer, NoAnswer> {
    if recipients.is_empty() {
        return match empty {
            EmptyDistribution::AllowWithoutTotal => Ok(Answer::Distribute { shares: Vec::new() }),
            EmptyDistribution::Reject => Err(NoAnswer::Proven),
        };
    }
    let minimum = share.min.get();
    let people = u64::try_from(recipients.len()).unwrap_or(u64::MAX);
    let mut left = total
        .get()
        .checked_sub(minimum.saturating_mul(people))
        .ok_or(NoAnswer::Proven)?;
    let room = share
        .max
        .map_or(left, |maximum| maximum.get().saturating_sub(minimum));
    let mut order: Vec<&Candidate> = recipients.iter().collect();
    order.shuffle(rng);
    let mut shares = Vec::new();
    for (index, recipient) in order.iter().enumerate() {
        let after =
            u64::try_from(order.len().saturating_sub(index).saturating_sub(1)).unwrap_or(u64::MAX);
        // Leave no more than the later recipients can still take.
        let least = left.saturating_sub(after.saturating_mul(room));
        let most = left.min(room);
        if least > most {
            return Err(NoAnswer::Proven);
        }
        let extra = rng.random_range(least..=most);
        left = left.saturating_sub(extra);
        shares.push(Share {
            option: recipient.id.clone(),
            amount: Nat::try_from(minimum.saturating_add(extra)).unwrap_or(Nat::MAX),
        });
    }
    Ok(Answer::Distribute { shares })
}

/// A random answer the question's stated domain allows.
///
/// # Errors
/// No answer satisfies the stated domain, or none was found in a domain too large to search.
#[expect(
    clippy::too_many_lines,
    reason = "One arm per wire shape keeps the whole answer domain in view."
)]
pub fn choose(question: &Question, rng: &mut ChaCha8Rng) -> Result<Answer, NoAnswer> {
    match &question.shape {
        QuestionShape::Select { selection } => {
            let selected = select(selection, rng)?;
            Ok(Answer::Select {
                selected: selected.ids,
                groups: selected.groups,
                roles: selected.roles,
            })
        }
        QuestionShape::Choose {
            options,
            bounds,
            count: limit,
            ..
        } => {
            let available = limit.map_or(options.len(), |limit| count(limit).min(options.len()));
            if available == 0 {
                return Ok(Answer::Choose { chosen: Vec::new() });
            }
            let (minimum, maximum) = sizes(*bounds, available).ok_or(NoAnswer::Proven)?;
            let take = rng.random_range(minimum..=maximum);
            let mut chosen: Vec<OptionId> =
                options.iter().map(|option| option.id.clone()).collect();
            chosen.shuffle(rng);
            chosen.truncate(take);
            Ok(Answer::Choose { chosen })
        }
        QuestionShape::Mulligan { hand } => {
            let redraw = rng.random();
            let mut bottom_order: Vec<OptionId> = if redraw {
                hand.iter().map(|card| card.id.clone()).collect()
            } else {
                Vec::new()
            };
            bottom_order.shuffle(rng);
            Ok(Answer::Mulligan {
                redraw,
                bottom_order,
            })
        }
        QuestionShape::ActionWindow { intents } => {
            let intent = intents.choose(rng).ok_or(NoAnswer::Proven)?;
            Ok(match intent {
                MainIntent::Attack { id, targets, .. } => Answer::ActionWindow {
                    intent: id.clone(),
                    target: Some(targets.choose(rng).ok_or(NoAnswer::Proven)?.id.clone()),
                },
                MainIntent::Play { id, .. }
                | MainIntent::Activate { id, .. }
                | MainIntent::Evolve { id, .. }
                | MainIntent::EndPhase { id } => Answer::ActionWindow {
                    intent: id.clone(),
                    target: None,
                },
            })
        }
        QuestionShape::Quick { options, .. } => Ok(Answer::Quick {
            option: options
                .get(rng.random_range(0..=options.len()))
                .map(|option| option.id.clone()),
        }),
        QuestionShape::EvolutionPayment { payments } => {
            let payment = payments.choose(rng).ok_or(NoAnswer::Proven)?;
            Ok(Answer::EvolutionPayment {
                pp: payment.pp,
                use_ep: payment.use_ep,
                super_evolution: payment.super_evolution,
            })
        }
        QuestionShape::ResourcePayment {
            materials,
            ep_allowed,
        } => Ok(Answer::ResourcePayment {
            materials: select(materials, rng)?.ids,
            use_ep: *ep_allowed && rng.random(),
        }),
        QuestionShape::Confirm { allowed, .. } => Ok(Answer::Confirm {
            accept: *allowed.choose(rng).ok_or(NoAnswer::Proven)?,
        }),
        QuestionShape::Integer { bounds, .. } => {
            let minimum = bounds.min.get();
            // An open-ended declaration has no finite domain; small values are the common case.
            let maximum = bounds.max.map_or_else(
                || minimum.saturating_add(u64::from(rng.random::<u64>().trailing_zeros())),
                Nat::get,
            );
            if minimum > maximum {
                return Err(NoAnswer::Proven);
            }
            Ok(Answer::Integer {
                value: Nat::try_from(rng.random_range(minimum..=maximum)).unwrap_or(Nat::MAX),
            })
        }
        QuestionShape::DiceReroll { allowed, .. } => Ok(Answer::DiceReroll {
            choice: *allowed.choose(rng).ok_or(NoAnswer::Proven)?,
        }),
        QuestionShape::Order { items, .. } => {
            let mut order: Vec<OptionId> = items.iter().map(|item| item.id.clone()).collect();
            order.shuffle(rng);
            Ok(Answer::Order { order })
        }
        QuestionShape::DriveTrigger {
            allowed,
            targets,
            target_bounds,
            ..
        } => {
            let mut answers = allowed.clone();
            answers.shuffle(rng);
            // Accepting may need a target that does not exist; another listed answer may not.
            answers
                .into_iter()
                .find_map(|accept| {
                    if !accept {
                        return Some(Answer::DriveTrigger {
                            accept,
                            target: None,
                        });
                    }
                    let (minimum, maximum) = sizes(*target_bounds, targets.len())?;
                    let target = (maximum > 0 && (minimum > 0 || rng.random()))
                        .then(|| targets.choose(rng).map(|target| target.id.clone()))
                        .flatten();
                    Some(Answer::DriveTrigger { accept, target })
                })
                .ok_or(NoAnswer::Proven)
        }
        QuestionShape::Partition { items, buckets } => partition(items, buckets, rng),
        QuestionShape::Distribute {
            recipients,
            total,
            share,
            empty,
        } => distribute(recipients, *total, *share, *empty, rng),
    }
}

#[cfg(test)]
mod tests;
