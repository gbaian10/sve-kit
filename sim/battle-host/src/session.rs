//! One local authority with one ordered channel per player seat.
//!
//! Commands are processed one at a time. Host-level validation happens before any native
//! call, so a rejected command cannot touch rules state or randomness.

use crate::wire::{
    HostError, HostErrorReason, InputState, MAX_SAFE_INTEGER, Outgoing, Receipt, ReceiptResult,
    Rejection, Seat, Submit, SubmitCommand, TEMPORARY_FORMAT, TemporaryAnswer, Update,
};
use core::fmt;
use sve_engine::game::Game;
use sve_engine::game::player::observation::Observation;
use sve_engine::game::player::{Command, Input, QuestionToken, Submission, SubmissionKind, Viewer};
use sve_engine::host::{HostLimits, HostOutcome, PolicyHost};
use sve_engine::policy::PolicyFailure;

/// Who answers a seat's questions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatControl {
    /// A player channel; the seat receives updates and sends commands.
    Player,
    /// The basic policy answers inside the host; the seat has no channel.
    Policy,
}

/// One outgoing message and the seat channel it belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delivery {
    /// Destination channel.
    pub seat: Seat,
    /// Message for that channel only.
    pub message: Outgoing,
}

#[derive(Debug)]
struct Issued {
    id: String,
    token: QuestionToken,
}

#[derive(Debug, PartialEq, Eq)]
struct Seen {
    question: Option<String>,
    input: Input,
    observation: Observation,
}

#[derive(Debug, Default)]
struct Channel {
    update_seq: u64,
    issued: u64,
    current: Option<Issued>,
    seen: Option<Seen>,
}

/// A local match: the authority, its policies and both seat channels.
pub struct LocalSession {
    game: Game,
    control: [SeatControl; 2],
    policy: PolicyHost,
    channels: [Channel; 2],
    stopped: Option<HostErrorReason>,
}

impl fmt::Debug for LocalSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The authority state is private; only host bookkeeping is shown.
        f.debug_struct("LocalSession")
            .field("control", &self.control)
            .field("channels", &self.channels)
            .field("stopped", &self.stopped)
            .finish_non_exhaustive()
    }
}

const SEATS: [Viewer; 2] = [Viewer::P1, Viewer::P2];

impl LocalSession {
    /// Bind an initialized game whose player input is ready.
    /// `limits` bounds every policy submission, including delegated player answers.
    #[must_use]
    pub fn new(game: Game, control: [SeatControl; 2], limits: HostLimits) -> Self {
        Self {
            game,
            control,
            policy: PolicyHost::new(limits),
            channels: [Channel::default(), Channel::default()],
            stopped: None,
        }
    }

    /// Read-only authority access for native comparisons; never exposed to a seat.
    #[must_use]
    pub const fn game(&self) -> &Game {
        &self.game
    }

    /// Whether a host error has stopped interaction.
    #[must_use]
    pub const fn stopped(&self) -> Option<HostErrorReason> {
        self.stopped
    }

    /// Let policy seats reach the first player stop, then send each player seat its baseline.
    pub fn start(&mut self) -> Vec<Delivery> {
        let mut out = Vec::new();
        if let Some(reason) = self.stopped {
            // Repeat the existing error; a stopped host never advances policies again.
            self.stop(reason, &mut out);
            return out;
        }
        if let Err(reason) = self.advance_policies() {
            self.stop(reason, &mut out);
            return out;
        }
        self.publish(Viewer::P1, true, &mut out);
        out
    }

    /// Resend this seat's complete current state, e.g. after a hot-seat switch.
    /// This is a query: it never advances rules, randomness or the update sequence.
    pub fn baseline(&mut self, seat: Seat) -> Vec<Delivery> {
        let mut out = Vec::new();
        if let Some(reason) = self.stopped {
            out.push(host_error(seat, reason));
            return out;
        }
        let viewer = Viewer::from(seat);
        if self.control_of(viewer) == SeatControl::Player {
            match self.refresh(viewer, true) {
                Ok(update) => out.extend(update.map(|update| Delivery {
                    seat,
                    message: Outgoing::Update(update),
                })),
                Err(reason) => self.stop(reason, &mut out),
            }
        }
        out
    }

    /// Parse and process one command; malformed messages stop interaction.
    pub fn submit_json(&mut self, seat: Seat, message: &str) -> Vec<Delivery> {
        match serde_json::from_str::<Submit>(message) {
            Ok(submit) => self.submit(seat, submit),
            Err(_error) => {
                let mut out = Vec::new();
                if let Some(reason) = self.stopped {
                    out.push(host_error(seat, reason));
                } else {
                    self.stop(HostErrorReason::Format, &mut out);
                }
                out
            }
        }
    }

    /// Process one command. The receipt precedes every update the command caused.
    pub fn submit(&mut self, seat: Seat, submit: Submit) -> Vec<Delivery> {
        let mut out = Vec::new();
        if let Some(reason) = self.stopped {
            out.push(host_error(seat, reason));
            return out;
        }
        let viewer = Viewer::from(seat);
        let kind = if self.control_of(viewer) == SeatControl::Policy {
            Ok(None)
        } else {
            match submit.command {
                SubmitCommand::Concede => {
                    Ok(Some(self.game.submit_player(viewer, Command::Concede).kind))
                }
                SubmitCommand::Answer {
                    question_id,
                    answer: TemporaryAnswer::Policy,
                } => self.delegate(viewer, &question_id),
            }
        };
        let kind = match kind {
            Ok(Some(kind)) => kind,
            Ok(None) => {
                out.push(receipt(seat, submit.request_id, None));
                return out;
            }
            Err(reason) => {
                self.stop(reason, &mut out);
                return out;
            }
        };
        out.push(receipt(seat, submit.request_id, Some(kind)));
        if kind != SubmissionKind::Rejected {
            // Accepted, rolled back or cancelled questions never revive, even with an equal token.
            self.channel(viewer).current = None;
        }
        if let Err(reason) = self.advance_policies() {
            self.stop(reason, &mut out);
            return out;
        }
        self.publish(viewer, false, &mut out);
        out
    }

    /// `None` when the id does not name this seat's current question.
    fn delegate(
        &mut self,
        viewer: Viewer,
        question_id: &str,
    ) -> Result<Option<SubmissionKind>, HostErrorReason> {
        let Input::Question(question) = self.game.input(viewer) else {
            return Ok(None);
        };
        let [first, second] = &self.channels;
        let channel = match viewer {
            Viewer::P1 => first,
            Viewer::P2 => second,
        };
        if !channel
            .current
            .as_ref()
            .is_some_and(|issued| issued.id == question_id && issued.token == question.token)
        {
            return Ok(None);
        }
        let observation = self
            .game
            .observe(viewer)
            .map_err(|_error| HostErrorReason::Projection)?;
        let command = self
            .policy
            .prepare(&observation, &question)
            .map_err(stop_reason)?;
        let submission = self.game.submit_player(viewer, command);
        self.record(&submission)?;
        Ok(Some(submission.kind))
    }

    /// Answer every consecutive policy-seat question.
    fn advance_policies(&mut self) -> Result<(), HostErrorReason> {
        loop {
            let viewer = match self.game.input(Viewer::P1) {
                Input::Question(_) => Viewer::P1,
                Input::Waiting(waiting) => waiting.actor,
                Input::Finished { .. } | Input::EngineFault => return Ok(()),
            };
            if self.control_of(viewer) == SeatControl::Player {
                return Ok(());
            }
            let Input::Question(question) = self.game.input(viewer) else {
                return Err(HostErrorReason::Projection);
            };
            let observation = self
                .game
                .observe(viewer)
                .map_err(|_error| HostErrorReason::Projection)?;
            let command = self
                .policy
                .prepare(&observation, &question)
                .map_err(stop_reason)?;
            let submission = self.game.submit_player(viewer, command);
            self.record(&submission)?;
        }
    }

    fn record(&mut self, submission: &Submission) -> Result<(), HostErrorReason> {
        match self.policy.record_submission(submission) {
            None | Some(HostOutcome::Finished { .. } | HostOutcome::EngineFault) => Ok(()),
            Some(outcome) => Err(stop_reason(outcome)),
        }
    }

    /// Updates for every player seat whose visible state changed, `first` seat first.
    fn publish(&mut self, first: Viewer, force: bool, out: &mut Vec<Delivery>) {
        let order = if first == Viewer::P1 {
            SEATS
        } else {
            [Viewer::P2, Viewer::P1]
        };
        let mut updates = Vec::new();
        for viewer in order {
            if self.control_of(viewer) == SeatControl::Policy {
                continue;
            }
            match self.refresh(viewer, force) {
                Ok(update) => updates.extend(update.map(|update| Delivery {
                    seat: viewer.into(),
                    message: Outgoing::Update(update),
                })),
                Err(reason) => {
                    // No partial update set: interaction stops instead.
                    self.stop(reason, out);
                    return;
                }
            }
        }
        out.extend(updates);
    }

    /// The seat's update if its visible state changed, or always when `force` is set.
    fn refresh(&mut self, viewer: Viewer, force: bool) -> Result<Option<Update>, HostErrorReason> {
        let input = self.game.input(viewer);
        let observation = self
            .game
            .observe(viewer)
            .map_err(|_error| HostErrorReason::Projection)?;
        let channel = self.channel(viewer);
        let question = match &input {
            Input::Question(question) => Some(match &channel.current {
                Some(issued) if issued.token == question.token => issued.id.clone(),
                Some(_) | None => {
                    let next = safe_increment(channel.issued)?;
                    channel.issued = next;
                    // The seat prefix keeps one seat's stale id from naming the other seat's question.
                    let id = format!("{}-q{next}", seat_name(viewer));
                    channel.current = Some(Issued {
                        id: id.clone(),
                        token: question.token,
                    });
                    id
                }
            }),
            Input::Waiting(_) | Input::Finished { .. } | Input::EngineFault => {
                channel.current = None;
                None
            }
        };
        let seen = Seen {
            question,
            input,
            observation,
        };
        if channel.seen.as_ref() == Some(&seen) && !force {
            return Ok(None);
        }
        if channel.seen.as_ref() != Some(&seen) {
            channel.update_seq = safe_increment(channel.update_seq)?;
        }
        let state = input_state(&seen.input, seen.question.as_deref());
        channel.seen = Some(seen);
        Ok(Some(Update {
            format: TEMPORARY_FORMAT,
            viewer: viewer.into(),
            update_seq: channel.update_seq,
            baseline: force,
            input: state,
        }))
    }

    fn stop(&mut self, reason: HostErrorReason, out: &mut Vec<Delivery>) {
        self.stopped = Some(reason);
        for viewer in SEATS {
            if self.control_of(viewer) == SeatControl::Player {
                out.push(host_error(viewer.into(), reason));
            }
        }
    }

    const fn control_of(&self, viewer: Viewer) -> SeatControl {
        let [first, second] = self.control;
        match viewer {
            Viewer::P1 => first,
            Viewer::P2 => second,
        }
    }

    const fn channel(&mut self, viewer: Viewer) -> &mut Channel {
        let [first, second] = &mut self.channels;
        match viewer {
            Viewer::P1 => first,
            Viewer::P2 => second,
        }
    }
}

fn input_state(input: &Input, question: Option<&str>) -> InputState {
    match (input, question) {
        (Input::Question(native), Some(id)) => InputState::Question {
            id: id.to_owned(),
            purpose: native.purpose.into(),
            shape: (&native.shape).into(),
        },
        (Input::Waiting(waiting), _) => InputState::Waiting {
            actor: waiting.actor.into(),
        },
        (Input::Finished { winner }, _) => InputState::Finished {
            winner: winner.map(Seat::from),
        },
        // Native surrender clears a fault, so the authority still accepts it here.
        // A question always has an issued id; the unreachable pairing is reported the same way.
        (Input::EngineFault | Input::Question(_), _) => {
            InputState::EngineFault { can_concede: true }
        }
    }
}

const fn seat_name(viewer: Viewer) -> &'static str {
    match viewer {
        Viewer::P1 => "p1",
        Viewer::P2 => "p2",
    }
}

fn safe_increment(value: u64) -> Result<u64, HostErrorReason> {
    value
        .checked_add(1)
        .filter(|next| *next <= MAX_SAFE_INTEGER)
        .ok_or(HostErrorReason::Range)
}

const fn stop_reason(outcome: HostOutcome) -> HostErrorReason {
    match outcome {
        HostOutcome::StepLimit
        | HostOutcome::RejectedLimit { .. }
        | HostOutcome::PolicyFailure {
            failure: PolicyFailure::SearchExhausted,
            ..
        } => HostErrorReason::Budget,
        HostOutcome::Finished { .. }
        | HostOutcome::EngineFault
        | HostOutcome::ProtocolError
        | HostOutcome::PolicyFailure { .. } => HostErrorReason::Projection,
    }
}

fn receipt(seat: Seat, request_id: String, kind: Option<SubmissionKind>) -> Delivery {
    let result = match kind {
        None | Some(SubmissionKind::Rejected) => ReceiptResult::Rejected,
        Some(SubmissionKind::Applied) => ReceiptResult::Applied,
        Some(SubmissionKind::Reverted) => ReceiptResult::Reverted,
        Some(SubmissionKind::Cancelled) => ReceiptResult::Cancelled,
        Some(SubmissionKind::EngineFault) => ReceiptResult::EngineFault,
    };
    Delivery {
        seat,
        message: Outgoing::Receipt(Receipt {
            request_id,
            result,
            rejection: (result == ReceiptResult::Rejected).then_some(Rejection::InvalidSubmission),
        }),
    }
}

const fn host_error(seat: Seat, reason: HostErrorReason) -> Delivery {
    Delivery {
        seat,
        message: Outgoing::HostError(HostError {
            format: TEMPORARY_FORMAT,
            reason,
        }),
    }
}
