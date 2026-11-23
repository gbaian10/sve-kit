//! Enumerate alternative answers lazily so a successful witness need not explore the domain.
//! Update these domains alongside the self-play policy in `answers.rs`.

use crate::game::player::{
    Answer, ClosingAnswer, ClosingQuestion, Command, DamageShare, DecisionShape, DiscardKind,
    EvolutionAnswer, EvolutionQuestion, Question, QuestionShape, SearchAnswer, SelectionGroup,
    SelectionQuestion, SelectionRole, TargetRoleAssignment,
};
use crate::ir::{
    Cardinality, Nat, RoleCardinality, SearchReplacementAnswer, StepwiseAnswer, TriggerIcon,
};
use core::iter::{empty, from_fn, once, successors};

type Choices<'question, T> = Box<dyn Iterator<Item = T> + 'question>;

fn selections<T>(values: Vec<T>, bounds: Cardinality) -> impl Iterator<Item = Vec<T>>
where
    T: Copy,
{
    let min = usize::try_from(bounds.minimum.0).unwrap_or(usize::MAX);
    let max = bounds
        .maximum
        .and_then(|value| usize::try_from(value.0).ok())
        .unwrap_or(usize::MAX)
        .min(values.len());
    (min..=max).flat_map(move |size| {
        let values = values.clone();
        let mut current = Some((0..size).collect::<Vec<_>>());
        from_fn(move || {
            let indices = current.take()?;
            let selected = indices
                .iter()
                .filter_map(|&index| values.get(index).copied())
                .collect();
            let free = values.len().saturating_sub(size);
            if let Some((slot, &index)) = indices
                .iter()
                .enumerate()
                .rfind(|&(slot, index)| *index < free.saturating_add(slot))
            {
                let first = index.saturating_add(1);
                let mut next = indices;
                for (offset, next_index) in next.iter_mut().skip(slot).enumerate() {
                    *next_index = first.saturating_add(offset);
                }
                current = Some(next);
            }
            Some(selected)
        })
    })
}

fn permutations<T>(values: Vec<T>) -> Choices<'static, Vec<T>>
where
    T: Copy + 'static,
{
    if values.is_empty() {
        return Box::new(once(vec![]));
    }
    Box::new((0..values.len()).flat_map(move |index| {
        let mut rest = values.clone();
        let first = rest.remove(index);
        permutations(rest).map(move |mut ordered| {
            ordered.insert(0, first);
            ordered
        })
    }))
}

fn targets(question: &SelectionQuestion) -> impl Iterator<Item = Vec<u64>> {
    selections(
        question
            .members
            .iter()
            .map(|member| member.handle)
            .collect(),
        question.cardinality,
    )
}

fn roles(
    domain: &[SelectionRole],
    remaining: Vec<u64>,
    total: u64,
) -> Choices<'_, Vec<TargetRoleAssignment>> {
    let Some((first, rest)) = domain.split_first() else {
        return Box::new(remaining.is_empty().then(Vec::new).into_iter());
    };
    let bounds = match first.cardinality {
        RoleCardinality::Fixed { cardinality } => cardinality,
        RoleCardinality::IfNonempty { count } => exact(if total == 0 { Nat(0) } else { count }),
        RoleCardinality::Remainder {} => Cardinality {
            minimum: Nat(0),
            maximum: Some(Nat(total)),
        },
    };
    Box::new(
        selections(remaining.clone(), bounds).flat_map(move |assigned| {
            let rest_targets = remaining
                .iter()
                .copied()
                .filter(|id| !assigned.contains(id))
                .collect();
            roles(rest, rest_targets, total).map(move |mut assignments| {
                assignments.insert(
                    0,
                    TargetRoleAssignment {
                        role: first.role,
                        targets: assigned.clone(),
                    },
                );
                assignments
            })
        }),
    )
}

fn grouped_search(groups: &[SelectionGroup], used: Vec<u64>) -> Choices<'_, Vec<Vec<u64>>> {
    let Some((first, rest)) = groups.split_first() else {
        return Box::new(once(vec![]));
    };
    let eligible = first
        .handles
        .iter()
        .copied()
        .filter(|id| !used.contains(id))
        .collect();
    Box::new(
        selections(eligible, first.cardinality).flat_map(move |chosen| {
            let mut all_used = used.clone();
            all_used.extend(&chosen);
            grouped_search(rest, all_used).map(move |mut assignments| {
                assignments.insert(0, chosen.clone());
                assignments
            })
        }),
    )
}

fn allocations(count: usize, extra: u64) -> Choices<'static, Vec<u64>> {
    match count {
        0 => Box::new(once(vec![])),
        1 => Box::new(once(vec![extra])),
        _ => Box::new((0..=extra).flat_map(move |first| {
            allocations(count.saturating_sub(1), extra.saturating_sub(first)).map(
                move |mut amounts| {
                    amounts.insert(0, first);
                    amounts
                },
            )
        })),
    }
}

const fn exact(value: Nat) -> Cardinality {
    Cardinality {
        minimum: value,
        maximum: Some(value),
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "The match covers each native decision domain."
)]
fn decisions(shape: &DecisionShape) -> Choices<'_, Answer> {
    match shape {
        DecisionShape::DiceReroll { .. } | DecisionShape::May => {
            Box::new([0, 1].into_iter().map(Nat).map(Answer::Integer))
        }
        DecisionShape::Abilities(options) => {
            Box::new(options.iter().map(|option| Answer::Ability(option.handle)))
        }
        DecisionShape::ResourceEntryPayment {
            materials,
            ep_allowed,
        } => Box::new(targets(materials).flat_map(move |materials| {
            [false, true]
                .into_iter()
                .filter(move |ep| !ep || *ep_allowed)
                .map(move |use_ep| Answer::ResourceEntryPayment {
                    materials: materials.clone(),
                    use_ep,
                })
        })),
        DecisionShape::WardEntry { .. } => {
            Box::new([false, true].into_iter().map(Answer::WardEntry))
        }
        DecisionShape::Quick { options, .. } => Box::new(
            once(Answer::Quick(None)).chain(
                options
                    .iter()
                    .map(|option| Answer::Quick(Some(option.handle))),
            ),
        ),
        DecisionShape::OptionalCost | DecisionShape::ResolutionPayment => {
            Box::new([true, false].into_iter().map(Answer::OptionalCost))
        }
        DecisionShape::BeforeDraw => Box::new(
            [StepwiseAnswer::Stop, StepwiseAnswer::Continue]
                .into_iter()
                .map(Answer::Stepwise),
        ),
        DecisionShape::CostMaterials(selection)
        | DecisionShape::Discard {
            kind: DiscardKind::Cost,
            cards: selection,
        } => Box::new(targets(selection).map(Answer::CostMaterials)),
        DecisionShape::Discard {
            kind: DiscardKind::Effect,
            cards,
        } => Box::new(targets(cards).map(Answer::Targets)),
        DecisionShape::ExchangeRoutes(routes) => {
            Box::new(routes.iter().map(|route| Answer::Integer(route.index)))
        }
        DecisionShape::Pending(options) => {
            Box::new(options.iter().map(|option| Answer::Pending(option.handle)))
        }
        DecisionShape::TriggerQuota(quota) => Box::new(
            quota
                .candidates
                .iter()
                .map(|option| Answer::TriggerQuota(option.handle)),
        ),
        DecisionShape::Integer { bounds, .. } => Box::new(
            successors(Some(bounds.minimum.0), |number| number.checked_add(1))
                .take_while(|number| bounds.maximum.is_none_or(|limit| *number <= limit.0))
                .map(Nat)
                .map(Answer::Integer),
        ),
        DecisionShape::DeclareName(names) => {
            Box::new(names.iter().cloned().map(Answer::DeclareName))
        }
        DecisionShape::Order(order) => Box::new(
            permutations(order.cards.iter().map(|card| card.handle).collect()).map(Answer::Order),
        ),
        DecisionShape::DamageReplacements(options) => Box::new(
            permutations(options.iter().map(|option| option.handle).collect()).map(Answer::Order),
        ),
        DecisionShape::DriveTrigger { icon, targets } => {
            let accepted: Choices<'_, Answer> = match icon {
                TriggerIcon::Front => Box::new(empty()),
                TriggerIcon::Draw | TriggerIcon::Heal => Box::new(once(Answer::DriveTrigger {
                    accept: true,
                    target: None,
                })),
                TriggerIcon::Critical | TriggerIcon::Stand => {
                    Box::new(targets.iter().map(|card| Answer::DriveTrigger {
                        accept: true,
                        target: Some(card.handle),
                    }))
                }
            };
            Box::new(
                once(Answer::DriveTrigger {
                    accept: false,
                    target: None,
                })
                .chain(accepted),
            )
        }
        DecisionShape::Search(question) => match &question.groups {
            Some(groups) => Box::new(
                grouped_search(groups, vec![])
                    .map(SearchAnswer::Grouped)
                    .map(Answer::Search),
            ),
            None => Box::new(
                selections(
                    question.cards.iter().map(|card| card.handle).collect(),
                    question.cardinality,
                )
                .map(SearchAnswer::Ungrouped)
                .map(Answer::Search),
            ),
        },
        DecisionShape::SearchPartition(question) => {
            let sets = selections(
                question
                    .selection
                    .cards
                    .iter()
                    .map(|card| card.handle)
                    .collect(),
                question.selection.cardinality,
            );
            if question.facts.as_ref().is_some_and(|facts| facts.ordered) {
                Box::new(sets.flat_map(permutations).map(Answer::SearchPartition))
            } else {
                Box::new(sets.map(Answer::SearchPartition))
            }
        }
        DecisionShape::MovementCapacity(question) => Box::new(
            selections(
                question
                    .selection
                    .cards
                    .iter()
                    .map(|card| card.handle)
                    .collect(),
                question.selection.cardinality,
            )
            .map(Answer::SearchPartition),
        ),
        DecisionShape::Distribution(question) => {
            let count = u64::try_from(question.members.len()).unwrap_or(u64::MAX);
            let Some(extra) = question
                .total
                .0
                .checked_sub(question.minimum.0.saturating_mul(count))
            else {
                return Box::new(empty());
            };
            Box::new(allocations(question.members.len(), extra).map(|amounts| {
                Answer::Distribution(
                    question
                        .members
                        .iter()
                        .zip(amounts)
                        .map(|(member, amount)| DamageShare {
                            handle: member.handle,
                            amount: Nat(question.minimum.0.saturating_add(amount)),
                        })
                        .collect(),
                )
            }))
        }
        DecisionShape::SearchReplacement(_) => Box::new(
            [
                SearchReplacementAnswer::Original,
                SearchReplacementAnswer::Replacement,
            ]
            .into_iter()
            .map(Answer::SearchReplacement),
        ),
        DecisionShape::Targets(selection) if selection.roles.is_empty() => {
            Box::new(targets(selection).map(Answer::Targets))
        }
        DecisionShape::Targets(selection) => Box::new(targets(selection).flat_map(|targets| {
            let total = u64::try_from(targets.len()).unwrap_or(u64::MAX);
            roles(&selection.roles, targets.clone(), total).map(move |roles| {
                Answer::TargetsWithRoles {
                    targets: targets.clone(),
                    roles,
                }
            })
        })),
        DecisionShape::TokenCapacity(question) => Box::new(
            (0..u64::try_from(question.choices.len()).unwrap_or(u64::MAX))
                .map(Answer::TokenCapacity),
        ),
        DecisionShape::Modes {
            modes,
            cardinality,
            count,
        }
        | DecisionShape::ResolutionAlternative {
            modes,
            cardinality,
            count,
        } => Box::new(
            selections(
                modes.clone(),
                Cardinality {
                    minimum: cardinality.minimum,
                    maximum: Some(
                        cardinality
                            .maximum
                            .map_or(*count, |max| Nat(max.0.min(count.0))),
                    ),
                },
            )
            .map(Answer::Modes),
        ),
    }
}

pub(super) fn alternatives(question: &Question) -> Choices<'_, Command> {
    let token = question.token;
    match &question.shape {
        QuestionShape::Decision(decision) => {
            Box::new(decisions(decision).map(move |answer| Command::Answer { token, answer }))
        }
        QuestionShape::Evolution(evolution) => {
            let answers: Choices<'_, EvolutionAnswer> = match evolution {
                EvolutionQuestion::Form(forms) => {
                    Box::new(forms.iter().map(|form| EvolutionAnswer::Form(form.id)))
                }
                EvolutionQuestion::Materials(domain) => {
                    Box::new(targets(domain).map(EvolutionAnswer::Materials))
                }
                EvolutionQuestion::Discard(domain) => {
                    Box::new(targets(domain).map(EvolutionAnswer::Discard))
                }
                EvolutionQuestion::Payment(payment) => {
                    Box::new([false, true].into_iter().flat_map(move |ep| {
                        let pp = payment.total_pp.saturating_sub(i64::from(ep));
                        [false, true]
                            .into_iter()
                            .filter(move |&super_evolution| {
                                payment.is_valid(pp, ep, super_evolution)
                            })
                            .map(move |super_evolution| EvolutionAnswer::Payment {
                                pp,
                                ep,
                                super_evolution,
                            })
                    }))
                }
            };
            Box::new(answers.map(move |answer| Command::Evolution { token, answer }))
        }
        QuestionShape::Closing(closing) => {
            let answers: Choices<'_, ClosingAnswer> = match closing {
                ClosingQuestion::GuardAct(cards) => Box::new(
                    selections(
                        cards.iter().map(|card| &card.handle).collect(),
                        Cardinality {
                            minimum: Nat(0),
                            maximum: None,
                        },
                    )
                    .map(|handles| ClosingAnswer::GuardAct(handles.into_iter().cloned().collect())),
                ),
                ClosingQuestion::Discard { cards, count } => Box::new(
                    selections(
                        cards.iter().map(|card| &card.handle).collect(),
                        exact(Nat(*count)),
                    )
                    .map(|handles| ClosingAnswer::Discard(handles.into_iter().cloned().collect())),
                ),
            };
            Box::new(answers.map(move |answer| Command::Closing { token, answer }))
        }
        QuestionShape::Opening(_) | QuestionShape::ActionWindow(_) => Box::new(empty()),
    }
}
