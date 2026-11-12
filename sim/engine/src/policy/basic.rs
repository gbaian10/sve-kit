use super::{PolicyFailure, decision};
use crate::game::player::observation::{Observation, TurnObservation};
use crate::game::player::{
    Answer, ClosingAnswer, ClosingQuestion, EvolutionAnswer, EvolutionPayment, EvolutionQuestion,
    MainActionAnswer, MainActions, MainIntent, MainIntentKind, OpeningAnswer, OpeningQuestion,
    QuestionShape, SubmissionKind, Viewer,
};
use crate::ir::{Cardinality, Nat};

/// A proposed player answer. Opaque capabilities are copied only for transport;
/// the caller binds the current question token when constructing its Command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    /// One opening procedure answer.
    Opening(OpeningAnswer),
    /// One serial normal-evolution answer.
    Evolution(EvolutionAnswer),
    /// One turn-closing answer.
    Closing(ClosingAnswer),
    /// One typed resolution or declaration answer.
    Decision(Answer),
    /// The chosen public intent and its complete immediate parameters.
    MainAction {
        /// Original option, including its unchanged opaque capability.
        intent: MainIntent,
        /// Target or no parameters as advertised by this intent.
        answer: MainActionAnswer,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MainKind {
    Play,
    Evolve,
    Activate,
    Attack,
    End,
}
impl MainKind {
    const fn of(kind: &MainIntentKind) -> Self {
        match kind {
            MainIntentKind::Play { .. } => Self::Play,
            MainIntentKind::Evolve { .. } => Self::Evolve,
            MainIntentKind::Activate { .. } => Self::Activate,
            MainIntentKind::Attack { .. } => Self::Attack,
            MainIntentKind::EndPhase => Self::End,
        }
    }
    const fn limited(self) -> bool {
        matches!(self, Self::Play | Self::Evolve | Self::Activate)
    }
}

/// One seat's conservative deterministic strategy, with a separate budget for each of
/// Play, Evolve and Activate per public turn. Attack remains governed by native permissions.
#[derive(Clone, Debug)]
pub struct BasicPolicy {
    viewer: Viewer,
    turn: Option<TurnObservation>,
    spent: Vec<MainKind>,
}

impl BasicPolicy {
    /// Keep one instance per seat; never share a seat's feedback with another policy.
    #[must_use]
    pub const fn new(viewer: Viewer) -> Self {
        Self {
            viewer,
            turn: None,
            spent: Vec::new(),
        }
    }

    /// Choose from authorized data. Repeated queries do not spend action budgets.
    /// Pair each actual submission with `record_submission` before choosing again.
    ///
    /// # Errors
    /// Rejects a mismatched viewer, malformed questions or an unavailable complete answer.
    pub fn choose(
        &mut self,
        observation: &Observation,
        question: &QuestionShape,
    ) -> Result<Choice, PolicyFailure> {
        if observation.viewer != self.viewer {
            return Err(PolicyFailure::InvalidQuestion);
        }
        if self.turn.is_none_or(|previous| {
            previous.active != observation.turn.active
                || previous.elapsed_turns != observation.turn.elapsed_turns
        }) {
            self.spent.clear();
        }
        self.turn = Some(observation.turn);
        Ok(match question {
            QuestionShape::Decision(question) => Choice::Decision(decision::answer(question)?),
            QuestionShape::Opening(question) => Choice::Opening(match question {
                OpeningQuestion::StartAmulet(cards) => OpeningAnswer::StartAmulet(
                    cards
                        .first()
                        .ok_or(PolicyFailure::NoSolution)?
                        .handle
                        .clone(),
                ),
                OpeningQuestion::FirstPlayer => OpeningAnswer::FirstPlayer(self.viewer),
                OpeningQuestion::Mulligan(_) => OpeningAnswer::Keep,
            }),
            QuestionShape::Evolution(question) => Choice::Evolution(match question {
                EvolutionQuestion::Form(forms) => {
                    EvolutionAnswer::Form(forms.first().ok_or(PolicyFailure::NoSolution)?.id)
                }
                EvolutionQuestion::Materials(question) => {
                    EvolutionAnswer::Materials(decision::materials(question)?)
                }
                EvolutionQuestion::Discard(question) => {
                    EvolutionAnswer::Discard(decision::materials(question)?)
                }
                EvolutionQuestion::Payment(payment) => payment_answer(*payment)?,
            }),
            QuestionShape::Closing(question) => Choice::Closing(match question {
                ClosingQuestion::GuardAct(cards) => {
                    ClosingAnswer::GuardAct(cards.iter().map(|card| card.handle.clone()).collect())
                }
                ClosingQuestion::Discard { cards, count } => {
                    let count = decision::count(
                        Cardinality {
                            minimum: Nat(*count),
                            maximum: Some(Nat(*count)),
                        },
                        cards.len(),
                    )?;
                    ClosingAnswer::Discard(
                        cards
                            .iter()
                            .take(count)
                            .map(|card| card.handle.clone())
                            .collect(),
                    )
                }
            }),
            QuestionShape::ActionWindow(actions) => {
                if observation.turn.active != Some(self.viewer) {
                    return Err(PolicyFailure::InvalidQuestion);
                }
                self.main(actions)?
            }
        })
    }

    /// Immediately record the result for the exact Choice that was submitted.
    /// Accepted, reverted and cancelled declarations spend that class's budget;
    /// rejected submissions and engine faults do not. Non-main choices have no budget.
    pub fn record_submission(&mut self, choice: &Choice, result: SubmissionKind) {
        if !matches!(
            result,
            SubmissionKind::Applied | SubmissionKind::Reverted | SubmissionKind::Cancelled
        ) {
            return;
        }
        if let Choice::MainAction { intent, .. } = choice {
            let kind = MainKind::of(&intent.kind);
            if kind.limited() && !self.spent.contains(&kind) {
                self.spent.push(kind);
            }
        }
    }

    fn main(&self, actions: &MainActions) -> Result<Choice, PolicyFailure> {
        for kind in [
            MainKind::Play,
            MainKind::Evolve,
            MainKind::Activate,
            MainKind::Attack,
            MainKind::End,
        ] {
            if self.spent.contains(&kind) {
                continue;
            }
            for intent in &actions.intents {
                if MainKind::of(&intent.kind) != kind {
                    continue;
                }
                let answer = if let MainIntentKind::Attack { targets, .. } = &intent.kind {
                    let Some(target) = targets.first() else {
                        continue;
                    };
                    MainActionAnswer::Attack(target.clone())
                } else {
                    MainActionAnswer::None
                };
                return Ok(Choice::MainAction {
                    intent: intent.clone(),
                    answer,
                });
            }
        }
        Err(PolicyFailure::NoSolution)
    }
}

fn payment_answer(payment: EvolutionPayment) -> Result<EvolutionAnswer, PolicyFailure> {
    if payment.total_pp < 0 || payment.available_pp < 0 {
        return Err(PolicyFailure::InvalidQuestion);
    }
    let ep = payment.total_pp > payment.available_pp && payment.ep_allowed && payment.total_pp > 0;
    let pp = payment
        .total_pp
        .checked_sub(i64::from(ep))
        .ok_or(PolicyFailure::NoSolution)?;
    if pp > payment.available_pp {
        return Err(PolicyFailure::NoSolution);
    }
    Ok(EvolutionAnswer::Payment {
        pp,
        ep,
        super_evolution: payment.super_allowed,
    })
}
