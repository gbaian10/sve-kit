//! Shared vocabulary for typed player input, observations and projected events.
//!
//! Authority setup hands control to this interface with `Game::advance_player_input`.
//! The observation module exposes authorized state; player submissions are separate.

pub mod observation;

mod closing;
mod opening;
mod pipeline;

pub use closing::{ClosingAnswer, ClosingQuestion};
pub use opening::{DeckConstruction, OpeningAnswer, OpeningQuestion, PlayerDeck};

pub use super::internal::{Answer, InputDomain};

use crate::ir::{Nat, PlayerId};
use core::fmt;

/// An authenticated player perspective supplied by the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Viewer {
    /// First player.
    P1,
    /// Second player.
    P2,
}

impl Viewer {
    /// The valid seat associated with this player perspective.
    #[must_use]
    pub const fn player(self) -> PlayerId {
        match self {
            Self::P1 => PlayerId { seat: 1 },
            Self::P2 => PlayerId { seat: 2 },
        }
    }
}

/// A viewer's opaque reference during one continuously identifiable period.
/// Issuance and revocation belong to the observation service; submissions must revalidate it.
///
/// ```
/// use sve_engine::game::player::ObjectHandle;
///
/// fn handle_text(handle: &ObjectHandle) -> &str {
///     handle.as_str()
/// }
/// ```
///
/// ```compile_fail,E0603
/// use sve_engine::game::player::ObjectHandle;
///
/// let forged = ObjectHandle("physical-object-id".into());
/// ```
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectHandle(pub(crate) String);

impl ObjectHandle {
    /// Opaque text for display or comparison, independent of authority identities.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// A question's rules purpose, independent of its answer shape or presentation time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QuestionPurpose {
    /// Parameters of a play, including a nested play during effect resolution.
    PlayParameter,
    /// A choice belonging to an effect's resolution.
    ResolutionChoice,
    /// A choice required by a rules procedure.
    RuleChoice,
    /// A fixed Quick window, including one whose only legal answer is Pass.
    QuickWindow,
    /// An active player's main-phase action window.
    MainAction,
}

/// Classification for the read-only player input result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InputKind {
    /// This viewer must answer the current question.
    Question,
    /// Another player must answer; only authorized public summaries may accompany it.
    Waiting,
    /// A normal rules result ended the game.
    Finished,
    /// Automatic progression stopped on an engine fault, without deciding a winner.
    EngineFault,
}

/// Classification for the player submission result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SubmissionKind {
    /// Accepted input advanced the procedure, possibly returning to target selection.
    Applied,
    /// Invalid actor, token or answer preserved the original question.
    Rejected,
    /// An illegal play returned to its origin and invalidated its question.
    Reverted,
    /// The existing rules procedure cancelled a pending occurrence.
    Cancelled,
    /// An engine failure preserved its rollback scope and stopped player progression.
    /// A rolled-back unsupported operation or damaged state remains an engine fault.
    EngineFault,
}

/// Opaque occurrence coordinate; it contains no execution or instruction identity.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct QuestionToken {
    epoch: u64,
    serial: u64,
    action: bool,
}

impl fmt::Debug for QuestionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QuestionToken").finish_non_exhaustive()
    }
}

/// Existing decision shapes or a separate action boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuestionShape {
    /// Serial opening procedure, using this viewer's current object capabilities.
    Opening(OpeningQuestion),
    /// A specific turn-closing rules procedure, distinct from Ward entry.
    Closing(ClosingQuestion),
    /// Token-scoped candidates from the existing typed decision procedure.
    Decision(InputDomain),
    /// Main-phase action, without general legal action enumeration.
    ActionWindow,
}

/// One authorized question, without the authority continuation or execution identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Question {
    /// Coordinate required by the answer command.
    pub token: QuestionToken,
    /// Authenticated seat permitted to answer.
    pub actor: Viewer,
    /// Rules role independent of the answer's shape.
    pub purpose: QuestionPurpose,
    /// Only candidates authorized for this actor.
    pub shape: QuestionShape,
}

/// Public information available while another seat answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Waiting {
    /// Seat currently deciding.
    pub actor: Viewer,
    /// Public die face and remaining rerolls, when the current decision is a reroll.
    pub dice: Option<(Nat, Nat)>,
}

/// Read-only player input state; no receipts, facts or continuation are returned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    /// This viewer is authorized to answer.
    Question(Box<Question>),
    /// Another seat is deciding; no private candidates accompany the summary.
    Waiting(Waiting),
    /// Normal rules termination; an absent winner is a draw.
    Finished {
        /// Winning seat, if the game was not drawn.
        winner: Option<Viewer>,
    },
    /// Player progression stopped; private diagnostic details remain with the authority.
    EngineFault,
}

impl Input {
    /// Stable classification shared by input consumers.
    #[must_use]
    pub const fn kind(&self) -> InputKind {
        match self {
            Self::Question(_) => InputKind::Question,
            Self::Waiting(_) => InputKind::Waiting,
            Self::Finished { .. } => InputKind::Finished,
            Self::EngineFault => InputKind::EngineFault,
        }
    }
}

/// An authenticated player's submission; surrender is independent of the current token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Answer the current opening procedure.
    Opening {
        /// Exact question returned to this seat.
        token: QuestionToken,
        /// Opening choice addressed by viewer capabilities.
        answer: OpeningAnswer,
    },
    /// Leave the current main phase, subject to existing attack obligations.
    EndPhase {
        /// Exact main-action question returned to this seat.
        token: QuestionToken,
    },
    /// Answer a turn-closing rules procedure using current viewer capabilities.
    Closing {
        /// Exact closing question returned to this seat.
        token: QuestionToken,
        /// Guard or hand-limit choice, never an entry Ward answer.
        answer: ClosingAnswer,
    },
    /// Respond only to the current decision occurrence.
    Answer {
        /// Exact question returned to this seat.
        token: QuestionToken,
        /// Existing typed payload, addressed only by question-scoped handles.
        answer: Answer,
    },
    /// Surrender even while another player is deciding.
    Concede,
}

/// Submission result and the submitting viewer's next input state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Submission {
    /// Preserves rejection, rollback, cancellation and engine-fault distinctions.
    pub kind: SubmissionKind,
    /// Already advanced to the next player input point.
    pub input: Input,
}
