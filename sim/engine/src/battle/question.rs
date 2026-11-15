//! Only authority-owned questions can bind a wire domain to native capabilities.

use super::ProjectionError;
use crate::game::player::{
    self, ObjectHandle as NativeHandle, OpeningAnswer, OpeningQuestion, QuestionToken, Viewer,
};
use alloc::collections::{BTreeMap, BTreeSet};
use core::{fmt, result};
use sve_battle_contract::{
    Answer, Nat, OptionId, QuestionId, Seat, Subject, Zone,
    observation::{BattleObservation, ProcessingStage},
    question as wire,
};

type Result<T> = result::Result<T, ProjectionError>;

/// A rejected answer binding, separate from an unavailable board or question.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("answer does not match the projected question")]
pub struct AnswerError;

/// One immutable question and board, with its native answer capabilities kept private.
///
/// The host issues a fresh question ID for this occurrence in this battle. It
/// authenticates the viewer before decoding, then submits the returned command
/// to the same Game, which still checks token freshness and game legality.
/// Currently supports first-player choice and whole-hand mulligan only.
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
    ) -> Result<Self> {
        if native.purpose != player::QuestionPurpose::RuleChoice
            || observation.viewer != seat(native.actor)
            || observation.processing.len() != 1
        {
            return Err(ProjectionError::Unavailable);
        }
        let processing = observation
            .processing
            .first()
            .filter(|context| context.stage == ProcessingStage::Opening)
            .cloned()
            .ok_or(ProjectionError::Unavailable)?;
        let (usage, shape, binding) = match native.shape {
            player::QuestionShape::Opening(OpeningQuestion::FirstPlayer) => first_player(id),
            player::QuestionShape::Opening(OpeningQuestion::Mulligan(cards)) => {
                mulligan(id, native.actor, &cards, &observation)?
            }
            player::QuestionShape::Opening(OpeningQuestion::StartAmulet(_))
            | player::QuestionShape::Evolution(_)
            | player::QuestionShape::Closing(_)
            | player::QuestionShape::Decision(_)
            | player::QuestionShape::ActionWindow(_) => return Err(ProjectionError::Unavailable),
        };
        let question = wire::Question {
            id: id.clone(),
            actor: seat(native.actor),
            purpose: wire::QuestionPurpose::RuleChoice,
            usage,
            processing,
            shape,
            auto_answer: wire::AutoAnswer::Unknown,
            assistance: wire::NoAssistance,
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
        let answer = match &self.binding {
            Binding::FirstPlayer(choices) => {
                let Answer::Choose { chosen } = answer else {
                    return Err(AnswerError);
                };
                if chosen.len() != 1 {
                    return Err(AnswerError);
                }
                OpeningAnswer::FirstPlayer(
                    chosen
                        .first()
                        .and_then(|choice| choices.get(choice))
                        .copied()
                        .ok_or(AnswerError)?,
                )
            }
            Binding::Mulligan(cards) => {
                let Answer::Mulligan {
                    redraw,
                    bottom_order,
                } = answer
                else {
                    return Err(AnswerError);
                };
                mulligan_answer(cards, *redraw, bottom_order)?
            }
        };
        Ok(player::Command::Opening {
            token: self.token,
            answer,
        })
    }
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
    let mut coordinates = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut candidates = Vec::new();
    for (index, card) in cards.iter().enumerate() {
        if !seen.insert(&card.handle) {
            return Err(ProjectionError::Unavailable);
        }
        let member = hand
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
            controller: Some(seat(card.properties.controller)),
            origin: Some(wire::Origin {
                player: seat(actor),
                zone: Zone::Hand,
                position: member.position,
            }),
            rules_names: None,
            original_cost: None,
            classes: None,
        });
        coordinates.insert(option_id, card.handle.clone());
    }
    Ok((
        wire::QuestionUse::Mulligan,
        wire::QuestionShape::Mulligan { hand: candidates },
        Binding::Mulligan(coordinates),
    ))
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

fn option(question: &QuestionId, index: usize) -> OptionId {
    OptionId::new(format!("{}:c{index}", question.as_str()))
}
const fn seat(viewer: Viewer) -> Seat {
    match viewer {
        Viewer::P1 => Seat::P1,
        Viewer::P2 => Seat::P2,
    }
}
