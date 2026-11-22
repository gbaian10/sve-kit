use super::{choice_count, sample};
use crate::game::player::{
    Answer, ClosingAnswer, ClosingQuestion, Command, DamageShare, DecisionShape, DiscardKind,
    EvolutionAnswer, EvolutionQuestion, MainActionAnswer, MainIntentKind, OpeningAnswer,
    OpeningQuestion, Question, QuestionShape, SearchAnswer, SearchQuestion, SelectionQuestion,
    Viewer,
};
use crate::ir::{Cardinality, Nat, SearchReplacementAnswer, StepwiseAnswer, TriggerIcon};
use crate::policy::{SelectionAnswer, search, select};
use rand::RngExt as _;
use rand::seq::SliceRandom as _;
use rand_chacha::ChaCha8Rng;

fn subset<T>(mut values: Vec<T>, count: usize, rng: &mut ChaCha8Rng) -> Vec<T> {
    values.shuffle(rng);
    values.truncate(count);
    values
}

fn chosen_selection(
    source: &SelectionQuestion,
    rng: &mut ChaCha8Rng,
) -> Result<SelectionAnswer, &'static str> {
    let mut question = source.clone();
    question.members.shuffle(rng);
    let count = choice_count(source.cardinality, question.members.len(), rng)?;
    let count = Nat(u64::try_from(count).map_err(|_error| "choice count overflow")?);
    question.cardinality = Cardinality {
        minimum: count,
        maximum: Some(count),
    };
    select(&question)
        .or_else(|_| {
            question.cardinality = source.cardinality;
            select(&question)
        })
        .map_err(|_error| "selection solver found no answer")
}

fn chosen_search(
    source: &SearchQuestion,
    rng: &mut ChaCha8Rng,
) -> Result<SearchAnswer, &'static str> {
    let mut question = source.clone();
    question.cards.shuffle(rng);
    let count = choice_count(source.cardinality, question.cards.len(), rng)?;
    let count = Nat(u64::try_from(count).map_err(|_error| "choice count overflow")?);
    question.cardinality = Cardinality {
        minimum: count,
        maximum: Some(count),
    };
    search(&question)
        .or_else(|_| {
            question.cardinality = source.cardinality;
            search(&question)
        })
        .map_err(|_error| "search solver found no answer")
}

#[expect(
    clippy::too_many_lines,
    reason = "All public question types are mapped to their native commands."
)]
pub(in crate::game) fn choose(
    question: &Question,
    rng: &mut ChaCha8Rng,
) -> Result<(Command, String), &'static str> {
    let token = question.token;
    let selected = match &question.shape {
        QuestionShape::Opening(opening) => {
            let (answer, context) = match opening {
                OpeningQuestion::StartAmulet(cards) => {
                    let card = sample(cards, rng)?;
                    (
                        OpeningAnswer::StartAmulet(card.handle.clone()),
                        format!("start amulet {}", card.physical.card),
                    )
                }
                OpeningQuestion::FirstPlayer => (
                    OpeningAnswer::FirstPlayer(if rng.random() { Viewer::P1 } else { Viewer::P2 }),
                    "first player".into(),
                ),
                OpeningQuestion::Mulligan(cards) => {
                    let mut handles: Vec<_> =
                        cards.iter().map(|card| card.handle.clone()).collect();
                    (
                        if rng.random() {
                            handles.shuffle(rng);
                            OpeningAnswer::Mulligan(handles)
                        } else {
                            OpeningAnswer::Keep
                        },
                        "mulligan".into(),
                    )
                }
            };
            (Command::Opening { token, answer }, context)
        }
        QuestionShape::Closing(closing) => {
            let (answer, context) = match closing {
                ClosingQuestion::GuardAct(cards) => (
                    ClosingAnswer::GuardAct(
                        cards
                            .iter()
                            .filter(|_| rng.random())
                            .map(|card| card.handle.clone())
                            .collect(),
                    ),
                    "guard act",
                ),
                ClosingQuestion::Discard { cards, count } => (
                    ClosingAnswer::Discard(subset(
                        cards.iter().map(|card| card.handle.clone()).collect(),
                        usize::try_from(*count).map_err(|_error| "discard count overflow")?,
                        rng,
                    )),
                    "hand limit discard",
                ),
            };
            (Command::Closing { token, answer }, context.into())
        }
        QuestionShape::Evolution(evolution) => {
            let (answer, context) = match evolution {
                EvolutionQuestion::Form(forms) => {
                    let form = sample(forms, rng)?;
                    (
                        EvolutionAnswer::Form(form.id),
                        format!("evolution form {:?}", form.card.physical.card),
                    )
                }
                EvolutionQuestion::Materials(materials) => (
                    EvolutionAnswer::Materials(chosen_selection(materials, rng)?.targets),
                    "evolution materials".into(),
                ),
                EvolutionQuestion::Discard(cards) => (
                    EvolutionAnswer::Discard(chosen_selection(cards, rng)?.targets),
                    "evolution discard".into(),
                ),
                EvolutionQuestion::Payment(payment) => {
                    let mut payments = Vec::new();
                    for ep in [false, true] {
                        for super_evolution in [false, true] {
                            let pp = payment.total_pp.saturating_sub(i64::from(ep));
                            if payment.is_valid(pp, ep, super_evolution) {
                                payments.push(EvolutionAnswer::Payment {
                                    pp,
                                    ep,
                                    super_evolution,
                                });
                            }
                        }
                    }
                    (sample(&payments, rng)?.clone(), "evolution payment".into())
                }
            };
            (Command::Evolution { token, answer }, context)
        }
        QuestionShape::ActionWindow(actions) => {
            let intent = sample(&actions.intents, rng)?;
            let (answer, context) = match &intent.kind {
                MainIntentKind::Attack { attacker, targets } => (
                    MainActionAnswer::Attack(sample(targets, rng)?.clone()),
                    format!("attack {:?}", attacker.physical.card),
                ),
                MainIntentKind::Evolve { holder, line, .. } => (
                    MainActionAnswer::None,
                    format!("evolve {:?} line {line}", holder.physical.card),
                ),
                MainIntentKind::Activate { holder, line, .. } => (
                    MainActionAnswer::None,
                    format!("activate {:?} line {line}", holder.physical.card),
                ),
                MainIntentKind::Play { card } => (
                    MainActionAnswer::None,
                    format!("play {}", card.physical.card),
                ),
                MainIntentKind::EndPhase => (MainActionAnswer::None, "end main".into()),
            };
            (
                Command::MainAction {
                    token,
                    intent: intent.id.clone(),
                    answer,
                },
                context,
            )
        }
        QuestionShape::Decision(shape) => {
            let (answer, context) = decision(shape, rng)?;
            (Command::Answer { token, answer }, context)
        }
    };
    Ok(selected)
}

#[expect(
    clippy::too_many_lines,
    reason = "The exhaustive match follows the native typed question variants."
)]
fn decision(
    question: &DecisionShape,
    rng: &mut ChaCha8Rng,
) -> Result<(Answer, String), &'static str> {
    let (answer, context) = match question {
        DecisionShape::DiceReroll { .. } => (
            Answer::Integer(Nat(rng.random_range(0..=1))),
            "dice reroll".into(),
        ),
        DecisionShape::Abilities(options) => {
            let ability = sample(options, rng)?;
            (
                Answer::Ability(ability.handle),
                format!(
                    "ability {} line {}",
                    ability.source_card, ability.source_line
                ),
            )
        }
        DecisionShape::ResourceEntryPayment {
            materials,
            ep_allowed,
        } => (
            Answer::ResourceEntryPayment {
                materials: chosen_selection(materials, rng)?.targets,
                use_ep: *ep_allowed && rng.random(),
            },
            "resource entry payment".into(),
        ),
        DecisionShape::WardEntry { card, .. } => (
            Answer::WardEntry(rng.random()),
            format!("ward entry {card}"),
        ),
        DecisionShape::Quick { options, .. } => {
            let index = rng.random_range(0..=options.len());
            let option = options.get(index);
            (
                Answer::Quick(option.map(|choice| choice.handle)),
                option.map_or_else(
                    || "quick pass".into(),
                    |choice| format!("quick {} line {:?}", choice.card, choice.line),
                ),
            )
        }
        DecisionShape::OptionalCost => (Answer::OptionalCost(rng.random()), "optional cost".into()),
        DecisionShape::ResolutionPayment => (
            Answer::OptionalCost(rng.random()),
            "resolution payment".into(),
        ),
        DecisionShape::May => (Answer::Integer(Nat(rng.random_range(0..=1))), "may".into()),
        DecisionShape::BeforeDraw => (
            Answer::Stepwise(if rng.random() {
                StepwiseAnswer::Continue
            } else {
                StepwiseAnswer::Stop
            }),
            "before draw".into(),
        ),
        DecisionShape::CostMaterials(question)
        | DecisionShape::Discard {
            kind: DiscardKind::Cost,
            cards: question,
        } => (
            Answer::CostMaterials(chosen_selection(question, rng)?.targets),
            "cost materials".into(),
        ),
        DecisionShape::Discard {
            kind: DiscardKind::Effect,
            cards,
        } => (
            Answer::Targets(chosen_selection(cards, rng)?.targets),
            "effect discard".into(),
        ),
        DecisionShape::ExchangeRoutes(routes) => (
            Answer::Integer(sample(routes, rng)?.index),
            "exchange route".into(),
        ),
        DecisionShape::Pending(options) => {
            let pending = sample(options, rng)?;
            (
                Answer::Pending(pending.handle),
                format!("pending {} line {}", pending.card, pending.line),
            )
        }
        DecisionShape::TriggerQuota(quota) => {
            let pending = sample(&quota.candidates, rng)?;
            (
                Answer::TriggerQuota(pending.handle),
                format!("trigger quota {} line {}", pending.card, pending.line),
            )
        }
        DecisionShape::Integer { bounds, .. } => {
            // An open-ended declaration has no finite options to enumerate.
            let maximum = bounds.maximum.unwrap_or_else(|| {
                Nat(bounds
                    .minimum
                    .0
                    .saturating_add(u64::from(rng.random::<u64>().trailing_zeros())))
            });
            if bounds.minimum.0 > maximum.0 {
                return Err("infeasible integer range");
            }
            (
                Answer::Integer(Nat(rng.random_range(bounds.minimum.0..=maximum.0))),
                "integer".into(),
            )
        }
        DecisionShape::DeclareName(names) => (
            Answer::DeclareName(sample(names, rng)?.clone()),
            "declare name".into(),
        ),
        DecisionShape::Order(order) => {
            let mut handles: Vec<_> = order.cards.iter().map(|card| card.handle).collect();
            handles.shuffle(rng);
            (Answer::Order(handles), "order cards".into())
        }
        DecisionShape::DamageReplacements(options) => {
            let mut handles: Vec<_> = options.iter().map(|option| option.handle).collect();
            handles.shuffle(rng);
            (Answer::Order(handles), "damage replacements".into())
        }
        DecisionShape::DriveTrigger { icon, targets } => {
            let (accept, target) = match icon {
                TriggerIcon::Front => (false, None),
                TriggerIcon::Draw | TriggerIcon::Heal => (rng.random(), None),
                TriggerIcon::Critical | TriggerIcon::Stand => {
                    let index = rng.random_range(0..=targets.len());
                    let target = targets.get(index).map(|card| card.handle);
                    (target.is_some(), target)
                }
            };
            (
                Answer::DriveTrigger { accept, target },
                format!("drive {icon:?}"),
            )
        }
        DecisionShape::Search(question) => (
            Answer::Search(chosen_search(question, rng)?),
            "search".into(),
        ),
        DecisionShape::SearchPartition(question) => {
            let choices = &question.selection;
            let count = choice_count(choices.cardinality, choices.cards.len(), rng)?;
            (
                Answer::SearchPartition(subset(
                    choices.cards.iter().map(|card| card.handle).collect(),
                    count,
                    rng,
                )),
                "search partition".into(),
            )
        }
        DecisionShape::MovementCapacity(question) => {
            let choices = &question.selection;
            let count = choice_count(choices.cardinality, choices.cards.len(), rng)?;
            (
                Answer::SearchPartition(subset(
                    choices.cards.iter().map(|card| card.handle).collect(),
                    count,
                    rng,
                )),
                "movement capacity".into(),
            )
        }
        DecisionShape::Distribution(question) => {
            let mut remainder = question
                .total
                .0
                .checked_sub(
                    question.minimum.0.saturating_mul(
                        u64::try_from(question.members.len())
                            .map_err(|_error| "distribution too large")?,
                    ),
                )
                .ok_or("infeasible distribution")?;
            let mut members: Vec<_> = question.members.iter().collect();
            members.shuffle(rng);
            let shares = members
                .iter()
                .enumerate()
                .map(|(index, member)| {
                    let extra = if index.saturating_add(1) == members.len() {
                        remainder
                    } else {
                        rng.random_range(0..=remainder)
                    };
                    remainder = remainder.saturating_sub(extra);
                    DamageShare {
                        handle: member.handle,
                        amount: Nat(question.minimum.0.saturating_add(extra)),
                    }
                })
                .collect();
            (Answer::Distribution(shares), "distribution".into())
        }
        DecisionShape::SearchReplacement(_) => (
            Answer::SearchReplacement(if rng.random() {
                SearchReplacementAnswer::Original
            } else {
                SearchReplacementAnswer::Replacement
            }),
            "search replacement".into(),
        ),
        DecisionShape::Targets(question) => {
            let selected = chosen_selection(question, rng)?;
            (
                if question.roles.is_empty() {
                    Answer::Targets(selected.targets)
                } else {
                    Answer::TargetsWithRoles {
                        targets: selected.targets,
                        roles: selected.roles,
                    }
                },
                "targets".into(),
            )
        }
        DecisionShape::TokenCapacity(question) => {
            if question.choices.is_empty() {
                return Err("empty token capacity");
            }
            (
                Answer::TokenCapacity(
                    u64::try_from(rng.random_range(0..question.choices.len()))
                        .map_err(|_error| "token capacity overflow")?,
                ),
                "token capacity".into(),
            )
        }
        DecisionShape::Modes {
            modes,
            cardinality,
            count,
        }
        | DecisionShape::ResolutionAlternative {
            modes,
            cardinality,
            count,
        } => {
            let limit = usize::try_from(count.0)
                .unwrap_or(usize::MAX)
                .min(modes.len());
            let take = if limit == 0 {
                0
            } else {
                choice_count(*cardinality, limit, rng)?
            };
            let mut chosen = subset(modes.clone(), take, rng);
            chosen.sort_by_key(|id| modes.iter().position(|original| original == id));
            (Answer::Modes(chosen), "modes".into())
        }
    };
    Ok((answer, context))
}
