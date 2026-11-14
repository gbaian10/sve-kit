//! Channel messages. Seats, ids, numbers, receipts and host errors are the battle contract's
//! (`sve-battle-contract`); the update and submit payloads stay temporary (format 0) until the
//! contract's question, answer and update types replace them.

use serde::{Deserialize, Serialize};
pub use sve_battle_contract::{
    HostError, HostErrorReason, Nat, QuestionId, Receipt, ReceiptResult, Rejection, RequestId, Seat,
};
use sve_engine::game::player::{QuestionPurpose, QuestionShape, Viewer};

/// Format number of the temporary update; the contract's own messages use format 1.
pub const TEMPORARY_FORMAT: u8 = 0;

/// The seat named on the wire, if any.
#[must_use]
pub fn seat_named(name: &str) -> Option<Seat> {
    match name {
        "p1" => Some(Seat::P1),
        "p2" => Some(Seat::P2),
        _ => None,
    }
}

/// The wire seat of a native viewer.
#[must_use]
pub const fn seat_of(viewer: Viewer) -> Seat {
    match viewer {
        Viewer::P1 => Seat::P1,
        Viewer::P2 => Seat::P2,
    }
}

/// The native viewer of a wire seat.
#[must_use]
pub const fn viewer_of(seat: Seat) -> Viewer {
    match seat {
        Seat::P1 => Viewer::P1,
        Seat::P2 => Viewer::P2,
    }
}

/// One command from a seat's channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Submit {
    /// Pairs the receipt with this command inside one local session.
    pub request_id: RequestId,
    /// The rules command.
    pub command: SubmitCommand,
}

/// Rules commands a seat may send.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SubmitCommand {
    /// Answer the question the host issued to this seat.
    Answer {
        /// Host-issued question id.
        question_id: QuestionId,
        /// Temporary answer payload.
        answer: TemporaryAnswer,
    },
    /// Surrender through the native entry point.
    Concede,
}

/// Until the contract's typed answers exist, a seat can only delegate the current
/// question to the basic policy; the host still validates the question binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    pub update_seq: Nat,
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
        id: QuestionId,
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
