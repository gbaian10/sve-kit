//! Only authority-owned questions can bind a wire domain to native capabilities.

use super::{BattleReferences, ProjectionError, SelectionProjection};
use crate::game::player::{
    self, ClosingAnswer, ClosingQuestion, EvolutionAnswer, EvolutionQuestion,
    ObjectHandle as NativeHandle, OpeningAnswer, OpeningQuestion, QuestionToken, Viewer,
};
use crate::{catalog::Catalog, game::player::observation::CardFace};
use alloc::collections::{BTreeMap, BTreeSet};
use core::{fmt, result};
use sve_battle_contract::{
    Answer, Int, Nat, OptionId, QuestionId, Seat, Subject, Zone,
    observation::{BattleObservation, ProcessingStage, ZoneView},
    question as wire,
};

type Result<T> = result::Result<T, ProjectionError>;
type CardCandidates = (Vec<wire::Candidate>, BTreeMap<OptionId, NativeHandle>);

mod evolution;

#[cfg(test)]
mod tests;

/// A rejected answer binding, separate from an unavailable board or question.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("answer does not match the projected question")]
pub struct AnswerError;

/// One immutable question and board, with its native answer capabilities kept private.
///
/// The host issues a fresh question ID for this occurrence in this battle. It
/// authenticates the viewer before decoding, then submits the returned command
/// to the same Game, which still checks token freshness and game legality.
/// Supports opening, closing, and normal evolution form, discard and payment questions.
///
/// ```compile_fail,E0624
/// use sve_engine::battle::QuestionProjection;
/// let _ = QuestionProjection::project;
/// ```
#[derive(Clone)]
pub struct QuestionProjection {
    observation: BattleObservation,
    question: wire::Question,
    actor: Viewer,
    token: QuestionToken,
    binding: Binding,
}

#[derive(Clone)]
enum Binding {
    FirstPlayer(BTreeMap<OptionId, Viewer>),
    Mulligan(BTreeMap<OptionId, NativeHandle>),
    StartAmulet(BTreeMap<OptionId, NativeHandle>),
    EndGuard(BTreeMap<OptionId, NativeHandle>),
    EvolutionPayment(Vec<wire::EvolutionPayment>),
    EvolutionForm(BTreeMap<OptionId, player::EvolutionFormId>),
    EvolutionDiscard(SelectionProjection),
    HandLimit {
        cards: BTreeMap<OptionId, NativeHandle>,
        count: usize,
    },
}

impl fmt::Debug for QuestionProjection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QuestionProjection").finish_non_exhaustive()
    }
}

impl QuestionProjection {
    pub(crate) fn project(
        id: &QuestionId,
        native: player::Question,
        observation: BattleObservation,
        catalog: &Catalog,
        references: &BattleReferences,
    ) -> Result<Self> {
        let (native_purpose, purpose, expected_stage) = match &native.shape {
            player::QuestionShape::Opening(_) => (
                player::QuestionPurpose::RuleChoice,
                wire::QuestionPurpose::RuleChoice,
                ProcessingStage::Opening,
            ),
            player::QuestionShape::Closing(_) => (
                player::QuestionPurpose::RuleChoice,
                wire::QuestionPurpose::RuleChoice,
                ProcessingStage::Closing,
            ),
            player::QuestionShape::Evolution(_) => (
                player::QuestionPurpose::PlayParameter,
                wire::QuestionPurpose::PlayParameter,
                ProcessingStage::Declare,
            ),
            player::QuestionShape::Decision(_) | player::QuestionShape::ActionWindow(_) => {
                return Err(ProjectionError::Unavailable);
            }
        };
        if native.purpose != native_purpose
            || observation.viewer != seat(native.actor)
            || observation.processing.len() != 1
        {
            return Err(ProjectionError::Unavailable);
        }
        let processing = observation
            .processing
            .first()
            .filter(|context| context.stage == expected_stage)
            .cloned()
            .ok_or(ProjectionError::Unavailable)?;
        let (usage, shape, binding) = match native.shape {
            player::QuestionShape::Opening(OpeningQuestion::FirstPlayer) => first_player(id),
            player::QuestionShape::Opening(OpeningQuestion::Mulligan(cards)) => {
                mulligan(id, native.actor, &cards, &observation)?
            }
            player::QuestionShape::Opening(OpeningQuestion::StartAmulet(cards)) => {
                start_amulet(id, native.actor, &cards, &observation)?
            }
            player::QuestionShape::Closing(ClosingQuestion::Discard { cards, count }) => {
                hand_limit(id, native.actor, &cards, count, &observation)?
            }
            player::QuestionShape::Closing(ClosingQuestion::GuardAct(cards)) => {
                end_guard(id, native.actor, &cards, &observation)?
            }
            player::QuestionShape::Evolution(EvolutionQuestion::Payment(quote)) => {
                evolution_payment(quote)?
            }
            player::QuestionShape::Evolution(EvolutionQuestion::Form(forms)) => {
                evolution_form(id, native.actor, &forms, &observation, catalog, references)?
            }
            player::QuestionShape::Evolution(EvolutionQuestion::Discard(domain)) => {
                evolution::discard(id, native.actor, &domain, &observation)?
            }
            player::QuestionShape::Evolution(EvolutionQuestion::Materials(_))
            | player::QuestionShape::Decision(_)
            | player::QuestionShape::ActionWindow(_) => return Err(ProjectionError::Unavailable),
        };
        let question = wire::Question {
            id: id.clone(),
            actor: seat(native.actor),
            purpose,
            usage,
            processing,
            shape,
            auto_answer: wire::AutoAnswer::Unknown,
            assistance: wire::NoAssistance,
            // Development-only until real RuleRefs resolve the known-empty/unknown gap.
            rules: vec![],
        };
        Ok(Self {
            observation,
            question,
            actor: native.actor,
            token: native.token,
            binding,
        })
    }

    /// Complete board from the same authority read and object scope as this question.
    #[must_use]
    pub const fn observation(&self) -> &BattleObservation {
        &self.observation
    }

    /// The only serializable question; no native token or private lookup is exposed.
    #[must_use]
    pub const fn question(&self) -> &wire::Question {
        &self.question
    }

    /// Decode an answer from the authenticated seat and the exact issued question.
    /// Native submission remains responsible for freshness and game legality.
    ///
    /// # Errors
    /// Rejects another seat, question, shape, foreign or repeated options, and
    /// any redraw order that is not a permutation of the complete hand.
    pub fn answer(
        &self,
        viewer: Viewer,
        id: &QuestionId,
        answer: &Answer,
    ) -> result::Result<player::Command, AnswerError> {
        if viewer != self.actor || id != &self.question.id {
            return Err(AnswerError);
        }
        let command = match &self.binding {
            Binding::EvolutionDiscard(projection) => player::Command::Evolution {
                token: self.token,
                answer: evolution::discard_answer(projection, answer)?,
            },
            Binding::EvolutionForm(forms) => {
                let Answer::Choose { chosen } = answer else {
                    return Err(AnswerError);
                };
                if chosen.len() != 1 {
                    return Err(AnswerError);
                }
                let form = chosen
                    .first()
                    .and_then(|choice| forms.get(choice))
                    .copied()
                    .ok_or(AnswerError)?;
                player::Command::Evolution {
                    token: self.token,
                    answer: EvolutionAnswer::Form(form),
                }
            }
            Binding::EvolutionPayment(payments) => player::Command::Evolution {
                token: self.token,
                answer: evolution_payment_answer(payments, answer)?,
            },
            Binding::HandLimit { cards, count } => player::Command::Closing {
                token: self.token,
                answer: ClosingAnswer::Discard(selected_cards(cards, Some(*count), answer)?),
            },
            Binding::EndGuard(cards) => player::Command::Closing {
                token: self.token,
                answer: ClosingAnswer::GuardAct(selected_cards(cards, None, answer)?),
            },
            Binding::FirstPlayer(choices) => {
                let Answer::Choose { chosen } = answer else {
                    return Err(AnswerError);
                };
                if chosen.len() != 1 {
                    return Err(AnswerError);
                }
                let decoded = OpeningAnswer::FirstPlayer(
                    chosen
                        .first()
                        .and_then(|choice| choices.get(choice))
                        .copied()
                        .ok_or(AnswerError)?,
                );
                player::Command::Opening {
                    token: self.token,
                    answer: decoded,
                }
            }
            Binding::Mulligan(cards) => {
                let Answer::Mulligan {
                    redraw,
                    bottom_order,
                } = answer
                else {
                    return Err(AnswerError);
                };
                player::Command::Opening {
                    token: self.token,
                    answer: mulligan_answer(cards, *redraw, bottom_order)?,
                }
            }
            Binding::StartAmulet(cards) => {
                let Answer::Select {
                    selected,
                    groups,
                    roles,
                } = answer
                else {
                    return Err(AnswerError);
                };
                if selected.len() != 1 || groups.is_some() || !roles.is_empty() {
                    return Err(AnswerError);
                }
                let decoded = OpeningAnswer::StartAmulet(
                    selected
                        .first()
                        .and_then(|choice| cards.get(choice))
                        .cloned()
                        .ok_or(AnswerError)?,
                );
                player::Command::Opening {
                    token: self.token,
                    answer: decoded,
                }
            }
        };
        Ok(command)
    }
}

fn evolution_form(
    id: &QuestionId,
    actor: Viewer,
    forms: &[player::EvolutionForm],
    observation: &BattleObservation,
    catalog: &Catalog,
    references: &BattleReferences,
) -> Result<(wire::QuestionUse, wire::QuestionShape, Binding)> {
    if forms.is_empty() {
        return Err(ProjectionError::Unavailable);
    }
    let deck = &observation
        .players
        .iter()
        .find(|player| player.seat == seat(actor))
        .ok_or(ProjectionError::Unavailable)?
        .zones
        .evolve_deck;
    let mut seen = BTreeSet::new();
    let mut choices = Vec::new();
    let mut coordinates = BTreeMap::new();
    for (index, form) in forms.iter().enumerate() {
        if !seen.insert((&form.card.handle, form.face)) {
            return Err(ProjectionError::Unavailable);
        }
        let member = deck
            .cards
            .iter()
            .find(|member| member.object.as_str() == form.card.handle.as_str())
            .ok_or(ProjectionError::Unavailable)?;
        let object = observation
            .objects
            .get(&member.object)
            .ok_or(ProjectionError::Unavailable)?;
        // The native form authorizes a destination, which may differ from its observed current face.
        let destination = references.resolve(
            catalog,
            &CardFace {
                card: form.card.properties.card.clone(),
                face: form.face,
            },
        )?;
        if destination.printing_id != object.card.printing_id {
            return Err(ProjectionError::Unavailable);
        }
        let option_id = option(id, index);
        // The destination CardRef lets clients read its name from the pinned snapshot.
        choices.push(wire::ChoiceOption {
            id: option_id.clone(),
            value: wire::ChoiceValue::EvolutionForm {
                object: member.object.clone(),
                face: destination,
            },
            cost: None,
        });
        coordinates.insert(option_id, form.id);
    }
    Ok((
        wire::QuestionUse::EvolutionForm,
        wire::QuestionShape::Choose {
            options: choices,
            bounds: wire::Bounds {
                min: Nat::from(1_u32),
                max: Some(Nat::from(1_u32)),
            },
            count: None,
            earlier: vec![],
            capacity: None,
        },
        Binding::EvolutionForm(coordinates),
    ))
}

fn evolution_payment(
    quote: player::EvolutionPayment,
) -> Result<(wire::QuestionUse, wire::QuestionShape, Binding)> {
    if quote.total_pp < 0 || quote.available_pp < 0 {
        return Err(ProjectionError::Unavailable);
    }
    let mut payments = Vec::new();
    // One optional EP replaces exactly one PP; changing that rule also changes this candidate template.
    for use_ep in [false, true] {
        let pp = quote
            .total_pp
            .checked_sub(i64::from(use_ep))
            .ok_or(ProjectionError::Unavailable)?;
        for super_evolution in [false, true] {
            if !quote.is_valid(pp, use_ep, super_evolution) {
                continue;
            }
            payments.push(wire::EvolutionPayment {
                pp: Int::try_from(pp)?,
                use_ep,
                super_evolution,
            });
        }
    }
    if payments.is_empty() {
        return Err(ProjectionError::Unavailable);
    }
    Ok((
        wire::QuestionUse::EvolutionPayment,
        wire::QuestionShape::EvolutionPayment {
            payments: payments.clone(),
        },
        Binding::EvolutionPayment(payments),
    ))
}

fn evolution_payment_answer(
    payments: &[wire::EvolutionPayment],
    answer: &Answer,
) -> result::Result<EvolutionAnswer, AnswerError> {
    let Answer::EvolutionPayment {
        pp,
        use_ep,
        super_evolution,
    } = answer
    else {
        return Err(AnswerError);
    };
    let chosen = wire::EvolutionPayment {
        pp: *pp,
        use_ep: *use_ep,
        super_evolution: *super_evolution,
    };
    if !payments.contains(&chosen) {
        return Err(AnswerError);
    }
    Ok(EvolutionAnswer::Payment {
        pp: pp.get(),
        ep: *use_ep,
        super_evolution: *super_evolution,
    })
}

fn first_player(id: &QuestionId) -> (wire::QuestionUse, wire::QuestionShape, Binding) {
    let choices = [Viewer::P1, Viewer::P2]
        .into_iter()
        .enumerate()
        .map(|(index, viewer)| (option(id, index), viewer))
        .collect::<BTreeMap<_, _>>();
    let options = choices
        .iter()
        .map(|(choice, viewer)| wire::ChoiceOption {
            id: choice.clone(),
            value: wire::ChoiceValue::Seat {
                player: seat(*viewer),
            },
            cost: None,
        })
        .collect();
    (
        wire::QuestionUse::FirstPlayer,
        wire::QuestionShape::Choose {
            options,
            bounds: wire::Bounds {
                min: Nat::from(1_u32),
                max: Some(Nat::from(1_u32)),
            },
            count: None,
            earlier: vec![],
            capacity: None,
        },
        Binding::FirstPlayer(choices),
    )
}

fn mulligan(
    id: &QuestionId,
    actor: Viewer,
    cards: &[player::observation::CardObservation],
    observation: &BattleObservation,
) -> Result<(wire::QuestionUse, wire::QuestionShape, Binding)> {
    let (candidates, coordinates) = hand_candidates(id, actor, cards, observation)?;
    Ok((
        wire::QuestionUse::Mulligan,
        wire::QuestionShape::Mulligan { hand: candidates },
        Binding::Mulligan(coordinates),
    ))
}

fn hand_candidates(
    id: &QuestionId,
    actor: Viewer,
    cards: &[player::observation::CardObservation],
    observation: &BattleObservation,
) -> Result<CardCandidates> {
    let hand = &observation
        .players
        .iter()
        .find(|player| player.seat == seat(actor))
        .ok_or(ProjectionError::Unavailable)?
        .zones
        .hand;
    if u64::try_from(cards.len()).ok() != Some(hand.count.get()) || cards.len() != hand.cards.len()
    {
        return Err(ProjectionError::Unavailable);
    }
    card_candidates(id, actor, cards, observation, Zone::Hand, hand)
}

fn hand_limit(
    id: &QuestionId,
    actor: Viewer,
    cards: &[player::observation::CardObservation],
    count: u64,
    observation: &BattleObservation,
) -> Result<(wire::QuestionUse, wire::QuestionShape, Binding)> {
    // Native closing skips a hand within its limit; zero or counts beyond the hand are inconsistent.
    let required = usize::try_from(count)
        .ok()
        .filter(|required| *required > 0 && *required <= cards.len())
        .ok_or(ProjectionError::Unavailable)?;
    let count = Nat::try_from(count)?;
    let (candidates, coordinates) = hand_candidates(id, actor, cards, observation)?;
    Ok((
        wire::QuestionUse::HandLimitDiscard,
        plain_selection(
            candidates,
            wire::Bounds {
                min: count,
                max: Some(count),
            },
        ),
        Binding::HandLimit {
            cards: coordinates,
            count: required,
        },
    ))
}

fn end_guard(
    id: &QuestionId,
    actor: Viewer,
    cards: &[player::observation::CardObservation],
    observation: &BattleObservation,
) -> Result<(wire::QuestionUse, wire::QuestionShape, Binding)> {
    // Native closing skips this question without standing guards; an empty domain is inconsistent.
    if cards.is_empty() {
        return Err(ProjectionError::Unavailable);
    }
    let field = &observation
        .players
        .iter()
        .find(|player| player.seat == seat(actor))
        .ok_or(ProjectionError::Unavailable)?
        .zones
        .field;
    let (candidates, coordinates) =
        card_candidates(id, actor, cards, observation, Zone::Field, field)?;
    Ok((
        wire::QuestionUse::EndGuard,
        plain_selection(
            candidates,
            wire::Bounds {
                min: Nat::from(0_u32),
                max: Some(Nat::try_from(cards.len())?),
            },
        ),
        Binding::EndGuard(coordinates),
    ))
}

fn start_amulet(
    id: &QuestionId,
    actor: Viewer,
    cards: &[player::observation::CardObservation],
    observation: &BattleObservation,
) -> Result<(wire::QuestionUse, wire::QuestionShape, Binding)> {
    // Native opening skips this question without eligible cards; an empty domain is inconsistent.
    if cards.is_empty() {
        return Err(ProjectionError::Unavailable);
    }
    let deck = &observation
        .players
        .iter()
        .find(|player| player.seat == seat(actor))
        .ok_or(ProjectionError::Unavailable)?
        .zones
        .deck;
    let (candidates, coordinates) =
        card_candidates(id, actor, cards, observation, Zone::Deck, deck)?;
    Ok((
        wire::QuestionUse::StartAmulet,
        plain_selection(
            candidates,
            wire::Bounds {
                min: Nat::from(1_u32),
                max: Some(Nat::from(1_u32)),
            },
        ),
        Binding::StartAmulet(coordinates),
    ))
}

const fn plain_selection(
    candidates: Vec<wire::Candidate>,
    bounds: wire::Bounds,
) -> wire::QuestionShape {
    wire::QuestionShape::Select {
        selection: wire::Selection {
            candidates,
            shown: vec![],
            bounds,
            groups: vec![],
            group_assignment: wire::GroupAssignment::Existence,
            roles: vec![],
            distinct: vec![],
            original_cost_total: wire::CostTotal {
                min: None,
                max: None,
            },
            capacity: None,
        },
    }
}

fn card_candidates(
    id: &QuestionId,
    actor: Viewer,
    cards: &[player::observation::CardObservation],
    observation: &BattleObservation,
    zone: Zone,
    members: &ZoneView,
) -> Result<CardCandidates> {
    let mut coordinates = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut candidates = Vec::new();
    for (index, card) in cards.iter().enumerate() {
        if !seen.insert(&card.handle) {
            return Err(ProjectionError::Unavailable);
        }
        let member = members
            .cards
            .iter()
            .find(|member| member.object.as_str() == card.handle.as_str())
            .ok_or(ProjectionError::Unavailable)?;
        let object = observation
            .objects
            .get(&member.object)
            .ok_or(ProjectionError::Unavailable)?;
        let option_id = option(id, index);
        candidates.push(wire::Candidate {
            id: option_id.clone(),
            subject: Some(Subject::Card {
                card: object.effective_face.clone(),
                object: Some(member.object.clone()),
            }),
            controller: Some(object.controller),
            origin: Some(wire::Origin {
                player: seat(actor),
                zone,
                position: member.position,
            }),
            rules_names: None,
            original_cost: None,
            classes: None,
        });
        coordinates.insert(option_id, card.handle.clone());
    }
    Ok((candidates, coordinates))
}

fn mulligan_answer(
    cards: &BTreeMap<OptionId, NativeHandle>,
    redraw: bool,
    bottom_order: &[OptionId],
) -> result::Result<OpeningAnswer, AnswerError> {
    if !redraw {
        return if bottom_order.is_empty() {
            Ok(OpeningAnswer::Keep)
        } else {
            Err(AnswerError)
        };
    }
    if bottom_order.len() != cards.len() {
        return Err(AnswerError);
    }
    let mut seen = BTreeSet::new();
    let order = bottom_order
        .iter()
        .map(|id| {
            if !seen.insert(id) {
                return Err(AnswerError);
            }
            cards.get(id).cloned().ok_or(AnswerError)
        })
        .collect::<result::Result<Vec<_>, _>>()?;
    Ok(OpeningAnswer::Mulligan(order))
}

fn selected_cards(
    cards: &BTreeMap<OptionId, NativeHandle>,
    exact: Option<usize>,
    answer: &Answer,
) -> result::Result<Vec<NativeHandle>, AnswerError> {
    let Answer::Select {
        selected,
        groups,
        roles,
    } = answer
    else {
        return Err(AnswerError);
    };
    if exact.is_some_and(|required| selected.len() != required)
        || groups.is_some()
        || !roles.is_empty()
    {
        return Err(AnswerError);
    }
    let mut seen = BTreeSet::new();
    let handles = selected
        .iter()
        .map(|choice| {
            if !seen.insert(choice) {
                return Err(AnswerError);
            }
            cards.get(choice).cloned().ok_or(AnswerError)
        })
        .collect::<result::Result<Vec<_>, _>>()?;
    Ok(handles)
}

fn option(question: &QuestionId, index: usize) -> OptionId {
    OptionId::new(format!("{}:c{index}", question.as_str()))
}
const fn seat(viewer: Viewer) -> Seat {
    match viewer {
        Viewer::P1 => Seat::P1,
        Viewer::P2 => Seat::P2,
    }
}
