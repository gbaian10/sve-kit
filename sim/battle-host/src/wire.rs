//! Temporary channel messages until the generated battle contract replaces them.
//!
//! Only the routing, receipt and rejection semantics are final; payloads are deliberately
//! minimal and use format 0 so no consumer mistakes them for the contract's format 1.

use serde::{Deserialize, Serialize};
use sve_engine::game::player::{QuestionPurpose, QuestionShape, Viewer};

/// Format number of these temporary messages; the contract starts at 1.
pub const TEMPORARY_FORMAT: u8 = 0;

/// Largest integer that survives a JavaScript number unchanged.
pub const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

/// A seat on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Seat {
    /// First seat.
    P1,
    /// Second seat.
    P2,
}

impl From<Viewer> for Seat {
    fn from(viewer: Viewer) -> Self {
        match viewer {
            Viewer::P1 => Self::P1,
            Viewer::P2 => Self::P2,
        }
    }
}

impl From<Seat> for Viewer {
    fn from(seat: Seat) -> Self {
        match seat {
            Seat::P1 => Self::P1,
            Seat::P2 => Self::P2,
        }
    }
}

/// One command from a seat's channel.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Submit {
    /// Pairs the receipt with this command inside one local session.
    pub request_id: String,
    /// The rules command.
    pub command: SubmitCommand,
}

/// Rules commands a seat may send.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SubmitCommand {
    /// Answer the question the host issued to this seat.
    Answer {
        /// Host-issued question id.
        question_id: String,
        /// Temporary answer payload.
        answer: TemporaryAnswer,
    },
    /// Surrender through the native entry point.
    Concede,
}

/// Until the contract's typed answers exist, a seat can only delegate the current
/// question to the basic policy; the host still validates the question binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TemporaryAnswer {
    /// The basic policy answers on behalf of this seat.
    Policy,
}

/// Message the host sends on one seat's channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outgoing {
    /// This seat's state after a visible change, or a requested full baseline.
    Update(Update),
    /// The result of this seat's command, sent before any update it caused.
    Receipt(Receipt),
    /// The host cannot continue safely; interaction stops.
    HostError(HostError),
}

/// Temporary update: input state only, without the contract's observation and events.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Update {
    /// Always [`TEMPORARY_FORMAT`].
    pub format: u8,
    /// Seat this update belongs to.
    pub viewer: Seat,
    /// Counts only this seat's visible changes.
    pub update_seq: u64,
    /// A full baseline replaces all previous seat state on the receiver.
    pub baseline: bool,
    /// Current input state.
    pub input: InputState,
}

/// Read-only input state for one seat.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InputState {
    /// This seat must answer.
    Question {
        /// Host-issued id, valid until this question is accepted, rolled back or replaced.
        id: String,
        /// Rules purpose.
        purpose: Purpose,
        /// Coarse shape family; the contract carries the full typed question.
        shape: ShapeFamily,
    },
    /// The other seat is deciding.
    Waiting {
        /// Deciding seat.
        actor: Seat,
    },
    /// Normal rules termination; no winner is a draw.
    Finished {
        /// Winning seat.
        winner: Option<Seat>,
    },
    /// Native rules execution stopped; the authority decides whether surrender remains.
    EngineFault {
        /// Whether a concede command can still be accepted.
        can_concede: bool,
    },
}

/// Rules purpose of a question.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    /// Parameters of a play.
    PlayParameter,
    /// A choice inside an effect's resolution.
    ResolutionChoice,
    /// A choice required by a rules procedure.
    RuleChoice,
    /// A Quick window.
    QuickWindow,
    /// The main-phase action window.
    MainAction,
}

impl From<QuestionPurpose> for Purpose {
    fn from(purpose: QuestionPurpose) -> Self {
        match purpose {
            QuestionPurpose::PlayParameter => Self::PlayParameter,
            QuestionPurpose::ResolutionChoice => Self::ResolutionChoice,
            QuestionPurpose::RuleChoice => Self::RuleChoice,
            QuestionPurpose::QuickWindow => Self::QuickWindow,
            QuestionPurpose::MainAction => Self::MainAction,
        }
    }
}

/// Native question family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeFamily {
    /// Normal evolution parameters.
    Evolution,
    /// Opening procedure.
    Opening,
    /// Turn-closing procedure.
    Closing,
    /// Existing typed decision.
    Decision,
    /// Main-phase intents.
    ActionWindow,
}

impl From<&QuestionShape> for ShapeFamily {
    fn from(shape: &QuestionShape) -> Self {
        match shape {
            QuestionShape::Evolution(_) => Self::Evolution,
            QuestionShape::Opening(_) => Self::Opening,
            QuestionShape::Closing(_) => Self::Closing,
            QuestionShape::Decision(_) => Self::Decision,
            QuestionShape::ActionWindow(_) => Self::ActionWindow,
        }
    }
}

/// Result of one command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Receipt {
    /// The command's request id.
    pub request_id: String,
    /// Native or host classification.
    pub result: ReceiptResult,
    /// Present exactly when the command was rejected.
    pub rejection: Option<Rejection>,
}

/// Receipt classification; native outcomes stay distinct.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptResult {
    /// Accepted and applied.
    Applied,
    /// Not executed; the original question stays answerable.
    Rejected,
    /// An illegal play returned to its origin.
    Reverted,
    /// The rules cancelled a pending occurrence.
    Cancelled,
    /// Native rules execution faulted.
    EngineFault,
}

/// Every invalid seat, question, option or answer has the same public reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rejection {
    /// Wrong seat, stale, unknown or repeated question, or an invalid answer.
    InvalidSubmission,
}

/// Host failure; it never carries the raw payload, secrets or diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct HostError {
    /// Always [`TEMPORARY_FORMAT`].
    pub format: u8,
    /// Failure class.
    pub reason: HostErrorReason,
}

/// Host failure classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostErrorReason {
    /// A malformed message or format mismatch.
    Format,
    /// A number outside the safe integer range.
    Range,
    /// Required state or a question could not be projected.
    Projection,
    /// A command or search budget ran out.
    Budget,
}
