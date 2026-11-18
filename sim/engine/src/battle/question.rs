//! Only authority-owned questions can bind a wire domain to native capabilities.

use super::{BattleReferences, ProjectionError, SelectionProjection};
use crate::catalog::Catalog;
use crate::game::player::{
    self, ClosingAnswer, ClosingQuestion, EvolutionAnswer, EvolutionQuestion,
    ObjectHandle as NativeHandle, OpeningAnswer, OpeningQuestion, QuestionToken, Viewer,
};
use alloc::collections::BTreeMap;
use core::{fmt, result};
use sve_battle_contract::{
    Answer, OptionId, QuestionId,
    observation::{BattleObservation, ProcessingContext, ProcessingStage},
    question as wire,
};

use closing::{end_guard, hand_limit};
use common::{seat, selected_cards};
use evolution::{evolution_form, evolution_payment, evolution_payment_answer};
use opening::{first_player, mulligan, mulligan_answer, start_amulet};

type Result<T> = result::Result<T, ProjectionError>;

mod closing;
mod common;
mod decision;
mod evolution;
mod flow;
mod main_actions;
mod opening;

/// A rejected answer binding, separate from an unavailable board or question.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("answer does not match the projected question")]
pub struct AnswerError;

/// One immutable question and board, with its native answer capabilities kept private.
///
/// The host issues a fresh question ID for this occurrence in this battle. It
/// authenticates the viewer before decoding, then submits the returned command
/// to the same Game, which still checks token freshness and game legality.
/// Supports all admitted player questions, including main-phase intents.
///
/// ```compile_fail,E0624
/// use sve_engine::battle::QuestionProjection;
/// let _ = QuestionProjection::project;
/// ```
#[derive(Clone)]
pub struct QuestionProjection {
    observation: BattleObservation,
    question: wire::Question,
    answer: BoundAnswer,
}

#[derive(Clone)]
pub(super) struct BoundAnswer {
    id: QuestionId,
    actor: Viewer,
    token: QuestionToken,
    binding: Binding,
}

#[derive(Clone)]
enum Binding {
    Decision(decision::Binding),
    Main(main_actions::Binding),
    Flow(flow::Binding),
    FirstPlayer(BTreeMap<OptionId, Viewer>),
    Mulligan(BTreeMap<OptionId, NativeHandle>),
    StartAmulet(BTreeMap<OptionId, NativeHandle>),
    EndGuard(BTreeMap<OptionId, NativeHandle>),
    EvolutionPayment(Vec<wire::EvolutionPayment>),
    EvolutionForm(BTreeMap<OptionId, player::EvolutionFormId>),
    EvolutionDiscard(SelectionProjection),
    EvolutionMaterials(SelectionProjection),
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
        current: Option<usize>,
        public_domain: bool,
        catalog: &Catalog,
        references: &BattleReferences,
    ) -> Result<Self> {
        let processing = if let Some(index) = current {
            observation
                .processing
                .get(index)
                .cloned()
                .ok_or(ProjectionError::Unavailable)?
        } else {
            // The question authorizes its actor, but cannot reveal a hidden frame's existence or source.
            ProcessingContext {
                stage: None,
                actor: Some(seat(native.actor)),
                controller: None,
                source: None,
                subject: None,
                rule: None,
                procedure: None,
                declared: vec![],
                payments: vec![],
            }
        };
        let purpose = question_purpose(&native, &observation, &processing)?;
        let context = decision::Context {
            id,
            actor: native.actor,
            purpose: native.purpose,
            observation: &observation,
            catalog,
            references,
        };
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
            player::QuestionShape::Evolution(EvolutionQuestion::Materials(domain)) => {
                evolution::materials(&domain, context)?
            }
            player::QuestionShape::Evolution(EvolutionQuestion::Discard(domain)) => {
                evolution::discard(id, native.actor, &domain, &observation)?
            }
            player::QuestionShape::Decision(
                domain @ (player::DecisionShape::Quick { .. }
                | player::DecisionShape::Pending(_)
                | player::DecisionShape::TriggerQuota(_)),
            ) => flow::project(
                &domain,
                context,
                processing.stage.ok_or(ProjectionError::Unavailable)?,
                catalog,
                references,
            )?,
            player::QuestionShape::Decision(domain) => {
                let (usage, shape, binding) = decision::project(&domain, Some(context))?;
                (usage, shape, Binding::Decision(binding))
            }
            player::QuestionShape::ActionWindow(actions) => {
                main_actions::project(&actions, context)?
            }
        };
        let auto_answer = auto_answer(usage, &shape, public_domain);
        let question = wire::Question {
            id: id.clone(),
            actor: seat(native.actor),
            purpose,
            usage,
            processing,
            shape,
            auto_answer,
            assistance: wire::NoAssistance,
            // Without a verified RuleRef closure, [] would falsely assert known-empty.
            rules: None,
        };
        Ok(Self {
            observation,
            question,
            answer: BoundAnswer {
                id: id.clone(),
                actor: native.actor,
                token: native.token,
                binding,
            },
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
        self.answer.answer(viewer, id, answer)
    }

    pub(super) fn into_parts(self) -> (BattleObservation, wire::Question, BoundAnswer) {
        (self.observation, self.question, self.answer)
    }
}

fn auto_answer(
    usage: wire::QuestionUse,
    shape: &wire::QuestionShape,
    public_domain: bool,
) -> wire::AutoAnswer {
    let answer = match (usage, shape) {
        (wire::QuestionUse::FirstPlayer, _) => return wire::AutoAnswer::NotUnique,
        (_, wire::QuestionShape::Quick { options, .. }) if !options.is_empty() => {
            return wire::AutoAnswer::NotUnique;
        }
        (_, wire::QuestionShape::Quick { .. }) if public_domain => Answer::Quick { option: None },
        (wire::QuestionUse::DeclareNumber, wire::QuestionShape::Integer { bounds, .. })
            if public_domain =>
        {
            // The declaration producer supplies literal bounds.
            if bounds.max != Some(bounds.min) {
                return wire::AutoAnswer::NotUnique;
            }
            Answer::Integer { value: bounds.min }
        }
        _ => return wire::AutoAnswer::Unknown,
    };
    wire::AutoAnswer::Unique {
        answer,
        public: true,
    }
}

impl BoundAnswer {
    pub(super) fn answer(
        &self,
        viewer: Viewer,
        id: &QuestionId,
        answer: &Answer,
    ) -> result::Result<player::Command, AnswerError> {
        if viewer != self.actor || id != &self.id {
            return Err(AnswerError);
        }
        Ok(match &self.binding {
            Binding::Decision(binding) => binding.answer(self.token, answer)?,
            Binding::Main(binding) => binding.answer(self.token, answer)?,
            Binding::Flow(binding) => binding.answer(self.token, answer)?,
            Binding::EvolutionDiscard(projection) => player::Command::Evolution {
                token: self.token,
                answer: evolution::discard_answer(projection, answer)?,
            },
            Binding::EvolutionMaterials(projection) => player::Command::Evolution {
                token: self.token,
                answer: evolution::materials_answer(projection, answer)?,
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
                let selected = chosen
                    .first()
                    .and_then(|choice| choices.get(choice))
                    .copied()
                    .ok_or(AnswerError)?;
                player::Command::Opening {
                    token: self.token,
                    answer: OpeningAnswer::FirstPlayer(selected),
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
        })
    }
}

fn question_purpose(
    native: &player::Question,
    observation: &BattleObservation,
    processing: &ProcessingContext,
) -> Result<wire::QuestionPurpose> {
    let stage = processing.stage;
    if processing.actor != Some(seat(native.actor)) {
        return Err(ProjectionError::Unavailable);
    }
    let (native_purpose, purpose, expected_stage) = match &native.shape {
        player::QuestionShape::Opening(_) => (
            player::QuestionPurpose::RuleChoice,
            wire::QuestionPurpose::RuleChoice,
            Some(ProcessingStage::Opening),
        ),
        player::QuestionShape::Closing(_) => (
            player::QuestionPurpose::RuleChoice,
            wire::QuestionPurpose::RuleChoice,
            Some(ProcessingStage::Closing),
        ),
        player::QuestionShape::Evolution(_) => (
            player::QuestionPurpose::PlayParameter,
            wire::QuestionPurpose::PlayParameter,
            Some(ProcessingStage::Declare),
        ),
        player::QuestionShape::Decision(
            player::DecisionShape::Quick { .. }
            | player::DecisionShape::Pending(_)
            | player::DecisionShape::TriggerQuota(_),
        ) => {
            let (native_purpose, purpose, recorded_stage) =
                flow::context(native.purpose, processing)?;
            (native_purpose, purpose, Some(recorded_stage))
        }
        player::QuestionShape::Decision(_) => {
            (native.purpose, flow::purpose(native.purpose), stage)
        }
        player::QuestionShape::ActionWindow(_) => (
            player::QuestionPurpose::MainAction,
            wire::QuestionPurpose::MainAction,
            Some(ProcessingStage::Declare),
        ),
    };
    if native.purpose != native_purpose
        || observation.viewer != seat(native.actor)
        || stage != expected_stage
    {
        return Err(ProjectionError::Unavailable);
    }
    Ok(purpose)
}
