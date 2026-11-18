//! One local authority with one ordered channel per player seat.
//!
//! Commands are processed one at a time. Each answer is decoded by the projection the seat was
//! last shown, so a stale, foreign or malformed answer is rejected before any native call and
//! cannot touch rules state or randomness.

use crate::wire::{
    BattleInput, BattleUpdate, ClientMessage, HostError, HostErrorReason, HostMessage, Identity,
    Nat, QuestionId, Ready, Receipt, ReceiptResult, RequestId, Seat, Submit, seat_named, seat_of,
    viewer_of,
};
use core::fmt;
use serde::Serialize;
use sve_battle_contract::observation::BattleObservation;
use sve_battle_contract::{Format, NoAdvice};
use sve_engine::battle::{BattleReferences, EventFeed, InputProjection, ProjectionError};
use sve_engine::game::Game;
use sve_engine::game::player::{Input, QuestionToken, Submission, SubmissionKind, Viewer};
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Delivery {
    /// Destination channel.
    pub seat: Seat,
    /// Message for that channel only.
    pub message: HostMessage,
}

#[derive(Debug)]
struct Issued {
    id: QuestionId,
    token: QuestionToken,
}

/// The visible state a seat was last sent; events are compared separately.
#[derive(Debug, PartialEq, Eq)]
struct Seen {
    observation: BattleObservation,
    input: BattleInput,
}

struct Channel {
    update_seq: Nat,
    issued: Nat,
    current: Option<Issued>,
    seen: Option<Seen>,
    /// Decodes answers to exactly what the seat was last shown.
    shown: Option<InputProjection>,
    feed: EventFeed,
}

impl Channel {
    const fn new(viewer: Viewer) -> Self {
        Self {
            update_seq: Nat::ZERO,
            issued: Nat::ZERO,
            current: None,
            seen: None,
            shown: None,
            feed: EventFeed::new(viewer),
        }
    }
}

impl fmt::Debug for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The answer capability and event cursors stay private.
        f.debug_struct("Channel")
            .field("update_seq", &self.update_seq)
            .field("issued", &self.issued)
            .field("current", &self.current)
            .finish_non_exhaustive()
    }
}

/// A local match: the authority, its references, its policies and both seat channels.
pub struct LocalSession {
    game: Game,
    references: BattleReferences,
    identity: Identity,
    ready: bool,
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
    /// Bind an initialized game whose player input is ready to the references of its snapshot
    /// and the content identity each seat is told in `Ready`.
    /// `limits` bounds the policy seats' submissions.
    #[must_use]
    pub fn new(
        game: Game,
        references: BattleReferences,
        identity: Identity,
        control: [SeatControl; 2],
        limits: HostLimits,
    ) -> Self {
        Self {
            game,
            references,
            identity,
            ready: false,
            control,
            policy: PolicyHost::new(limits),
            channels: [Channel::new(Viewer::P1), Channel::new(Viewer::P2)],
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

    /// Tell each player seat the game's identity once, let policy seats reach the first player
    /// stop, then send each player seat its baseline.
    pub fn start(&mut self) -> Vec<Delivery> {
        let mut out = Vec::new();
        if let Some(reason) = self.stopped {
            // Repeat the existing error; a stopped host never advances policies again.
            self.stop(reason, &mut out);
            return out;
        }
        if !self.ready {
            self.ready = true;
            for viewer in SEATS {
                if self.control_of(viewer) == SeatControl::Player {
                    out.push(Delivery {
                        seat: seat_of(viewer),
                        message: HostMessage::Ready(Ready {
                            format: Format,
                            viewer: seat_of(viewer),
                            identity: self.identity.clone(),
                        }),
                    });
                }
            }
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
        let viewer = viewer_of(seat);
        if self.control_of(viewer) == SeatControl::Player {
            match self.refresh(viewer, true) {
                Ok(update) => out.extend(update.map(|update| Delivery {
                    seat,
                    message: HostMessage::Update(Box::new(update)),
                })),
                Err(reason) => self.stop(reason, &mut out),
            }
        }
        out
    }

    /// Process a command routed by seat name, as the Worker passes it. An unknown seat name is a
    /// malformed message: interaction stops for every player seat.
    pub fn submit_named(&mut self, seat: &str, message: &str) -> Vec<Delivery> {
        match seat_named(seat) {
            Some(seat) => self.submit_json(seat, message),
            None => self.malformed(),
        }
    }

    /// [`Self::baseline`] routed by seat name; an unknown name stops interaction.
    pub fn baseline_named(&mut self, seat: &str) -> Vec<Delivery> {
        match seat_named(seat) {
            Some(seat) => self.baseline(seat),
            None => self.malformed(),
        }
    }

    fn malformed(&mut self) -> Vec<Delivery> {
        let mut out = Vec::new();
        let reason = self.stopped.unwrap_or(HostErrorReason::Format);
        self.stop(reason, &mut out);
        out
    }

    /// Parse and process one client message; malformed messages stop interaction.
    /// Numbers outside the safe integer range stop with `range`, other malformed input with `format`.
    pub fn submit_json(&mut self, seat: Seat, message: &str) -> Vec<Delivery> {
        match sve_battle_contract::decode::<ClientMessage>(message) {
            Ok(ClientMessage::Submit(submit)) => self.submit(seat, &submit),
            Err(error) => {
                let mut out = Vec::new();
                if let Some(reason) = self.stopped {
                    out.push(host_error(seat, reason));
                } else {
                    self.stop(error.reason, &mut out);
                }
                out
            }
        }
    }

    /// Process one command. The receipt precedes every update the command caused.
    pub fn submit(&mut self, seat: Seat, submit: &Submit) -> Vec<Delivery> {
        let mut out = Vec::new();
        if let Some(reason) = self.stopped {
            out.push(host_error(seat, reason));
            return out;
        }
        let viewer = viewer_of(seat);
        let request_id = submit.request_id.clone();
        // Only what this seat was shown can decode its command; a policy seat was shown nothing.
        let decoded = self
            .channel(viewer)
            .shown
            .as_ref()
            .and_then(|shown| shown.command(viewer, &submit.command).ok());
        let Some(command) = decoded else {
            out.push(receipt(seat, request_id, SubmissionKind::Rejected));
            return out;
        };
        let kind = self.game.submit_player(viewer, command).kind;
        out.push(receipt(seat, request_id, kind));
        if kind == SubmissionKind::Rejected {
            return out;
        }
        // Accepted, rolled back or cancelled questions never revive, even with an equal token.
        self.channel(viewer).current = None;
        if let Err(reason) = self.advance_policies() {
            self.stop(reason, &mut out);
            return out;
        }
        self.publish(viewer, false, &mut out);
        out
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
                    seat: seat_of(viewer),
                    message: HostMessage::Update(Box::new(update)),
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

    /// The seat's update if its board or input changed or it has events it has not received,
    /// or always when `force` is set.
    ///
    /// Events are taken only on this delivery path: taking them records that the seat was
    /// handed this stop, which must not happen at a stop the seat is never sent. New committed
    /// batches and a first event gap are themselves a reason to deliver.
    /// A forced update is a baseline: it replays no earlier events.
    fn refresh(
        &mut self,
        viewer: Viewer,
        force: bool,
    ) -> Result<Option<BattleUpdate>, HostErrorReason> {
        let id = self.question_id(viewer)?;
        let [first, second] = &mut self.channels;
        let channel = match viewer {
            Viewer::P1 => first,
            Viewer::P2 => second,
        };
        let projection = self
            .game
            .battle_input(viewer, &id, &self.references)
            .map_err(projection_reason)?;
        let seen = Seen {
            observation: projection.observation().clone(),
            input: projection.input().clone(),
        };
        channel.shown = Some(projection);
        let changed = channel.seen.as_ref() != Some(&seen);
        let news = !force && channel.feed.has_news(&self.game);
        if !(force || changed || news) {
            return Ok(None);
        }
        if changed || news {
            channel.update_seq = safe_increment(channel.update_seq)?;
        }
        let feed = if force {
            channel.feed.baseline(&mut self.game)
        } else {
            channel.feed.deliver(&mut self.game, &self.references)
        };
        let update = BattleUpdate {
            format: Format,
            viewer: seat_of(viewer),
            update_seq: channel.update_seq,
            observation: seen.observation.clone(),
            input: seen.input.clone(),
            events: feed.events,
            presentation: feed.presentation,
            notices: Vec::new(),
            advice: NoAdvice,
        };
        channel.seen = Some(seen);
        Ok(Some(update))
    }

    /// The host-issued id of this seat's current question: kept while the native occurrence
    /// stays the same, fresh for every new occurrence. Only a question reads it.
    fn question_id(&mut self, viewer: Viewer) -> Result<QuestionId, HostErrorReason> {
        let input = self.game.input(viewer);
        let channel = self.channel(viewer);
        let Input::Question(question) = input else {
            channel.current = None;
            return Ok(QuestionId::new(format!("{}-q0", seat_name(viewer))));
        };
        if let Some(issued) = &channel.current
            && issued.token == question.token
        {
            return Ok(issued.id.clone());
        }
        let next = safe_increment(channel.issued)?;
        channel.issued = next;
        // The seat prefix keeps one seat's stale id from naming the other seat's question.
        let id = QuestionId::new(format!("{}-q{}", seat_name(viewer), next.get()));
        channel.current = Some(Issued {
            id: id.clone(),
            token: question.token,
        });
        Ok(id)
    }

    fn stop(&mut self, reason: HostErrorReason, out: &mut Vec<Delivery>) {
        self.stopped = Some(reason);
        for viewer in SEATS {
            if self.control_of(viewer) == SeatControl::Player {
                out.push(host_error(seat_of(viewer), reason));
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

const fn seat_name(viewer: Viewer) -> &'static str {
    match viewer {
        Viewer::P1 => "p1",
        Viewer::P2 => "p2",
    }
}

fn safe_increment(value: Nat) -> Result<Nat, HostErrorReason> {
    value
        .get()
        .checked_add(1)
        .and_then(|next| Nat::try_from(next).ok())
        .ok_or(HostErrorReason::Range)
}

const fn projection_reason(error: ProjectionError) -> HostErrorReason {
    error.host_error().reason
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

const fn receipt(seat: Seat, request_id: RequestId, kind: SubmissionKind) -> Delivery {
    let result = match kind {
        SubmissionKind::Rejected => ReceiptResult::Rejected,
        SubmissionKind::Applied => ReceiptResult::Applied,
        SubmissionKind::Reverted => ReceiptResult::Reverted,
        SubmissionKind::Cancelled => ReceiptResult::Cancelled,
        SubmissionKind::EngineFault => ReceiptResult::EngineFault,
    };
    Delivery {
        seat,
        // The contract pairs `rejected` with `invalid_submission` and nothing else.
        message: HostMessage::Receipt(Receipt::new(request_id, result)),
    }
}

const fn host_error(seat: Seat, reason: HostErrorReason) -> Delivery {
    Delivery {
        seat,
        message: HostMessage::HostError(HostError::new(reason)),
    }
}
