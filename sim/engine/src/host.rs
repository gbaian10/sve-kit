//! Bounded two-seat play through the public player interface.
//! The driver owns transport; each strategy receives only its own authorized data.

use crate::game::Game;
use crate::game::player::observation::Observation;
use crate::game::player::{
    Command, Input, Question, QuestionToken, Submission, SubmissionKind, Viewer,
};
use crate::policy::{BasicPolicy, Choice, PolicyFailure};

/// Limits count actual submissions, including rejected ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostLimits {
    /// Maximum number of commands sent during this host's lifetime.
    pub max_submissions: u64,
    /// Additional attempts allowed after rejections of the same question.
    pub max_retries: u64,
}
impl Default for HostLimits {
    fn default() -> Self {
        Self {
            max_submissions: 10_000,
            max_retries: 1,
        }
    }
}

/// Command results stay separate even when the match later finishes normally.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostStats {
    /// All completed submissions, including faulting submissions.
    pub submissions: u64,
    /// Accepted commands.
    pub applied: u64,
    /// Commands rejected without advancing the question.
    pub rejected: u64,
    /// Illegal declarations rolled back by the native pipeline.
    pub reverted: u64,
    /// Occurrences cancelled by the native rules.
    pub cancelled: u64,
    /// Submissions that stopped on an engine fault.
    pub engine_faults: u64,
    /// Accepted commands in P1, P2 order.
    pub applied_by_seat: [u64; 2],
}
impl HostStats {
    fn record(&mut self, actor: Viewer, kind: SubmissionKind) -> Option<()> {
        self.submissions = self.submissions.checked_add(1)?;
        let count = match kind {
            SubmissionKind::Applied => &mut self.applied,
            SubmissionKind::Rejected => &mut self.rejected,
            SubmissionKind::Reverted => &mut self.reverted,
            SubmissionKind::Cancelled => &mut self.cancelled,
            SubmissionKind::EngineFault => &mut self.engine_faults,
        };
        *count = count.checked_add(1)?;
        if kind == SubmissionKind::Applied {
            let [first, second] = &mut self.applied_by_seat;
            let per_seat = match actor {
                Viewer::P1 => first,
                Viewer::P2 => second,
            };
            *per_seat = per_seat.checked_add(1)?;
        }
        Some(())
    }
}

/// Why the host stopped. Only Finished is a normal rules result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostOutcome {
    /// Native rules termination; no winner means a draw.
    Finished {
        /// Winning seat.
        winner: Option<Viewer>,
    },
    /// A player input, observation or submission reported an engine fault.
    EngineFault,
    /// A strategy could not construct a complete answer.
    PolicyFailure {
        /// Seat that was deciding.
        actor: Viewer,
        /// Strategy failure, without authority diagnostics.
        failure: PolicyFailure,
    },
    /// The same question exceeded its allowed rejected retries.
    RejectedLimit {
        /// Seat whose question was rejected.
        actor: Viewer,
    },
    /// The total command budget ran out before rules termination.
    StepLimit,
    /// Input routing or prepare/record pairing violated the host protocol.
    ProtocolError,
}

/// A stopped host's result and actual command counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostReport {
    /// Normal result or a distinct failure/limit.
    pub outcome: HostOutcome,
    /// Counts include the command that stopped the host.
    pub stats: HostStats,
}

#[derive(Debug)]
struct Attempt {
    actor: Viewer,
    token: QuestionToken,
    choice: Choice,
}

/// Two independent basic policies and one outstanding command at a time.
/// A stopped host is sticky: create another host to begin a separate run.
#[derive(Debug)]
pub struct PolicyHost {
    first: BasicPolicy,
    second: BasicPolicy,
    limits: HostLimits,
    stats: HostStats,
    pending: Option<Attempt>,
    rejected: Option<(Viewer, QuestionToken, u64)>,
    outcome: Option<HostOutcome>,
}

impl PolicyHost {
    /// Construct a fresh bounded host; policies never share private observations.
    #[must_use]
    pub fn new(limits: HostLimits) -> Self {
        Self {
            first: BasicPolicy::new(Viewer::P1),
            second: BasicPolicy::new(Viewer::P2),
            limits,
            stats: HostStats::default(),
            pending: None,
            rejected: None,
            outcome: None,
        }
    }

    /// Inspect command counts without issuing a command.
    #[must_use]
    pub const fn stats(&self) -> HostStats {
        self.stats
    }

    /// A report exists only after a normal finish, failure or limit.
    #[must_use]
    pub const fn report(&self) -> Option<HostReport> {
        match self.outcome {
            Some(outcome) => Some(HostReport {
                outcome,
                stats: self.stats,
            }),
            None => None,
        }
    }

    fn stop(&mut self, outcome: HostOutcome) -> HostOutcome {
        *self.outcome.get_or_insert(outcome)
    }

    const fn policy(&mut self, actor: Viewer) -> &mut BasicPolicy {
        match actor {
            Viewer::P1 => &mut self.first,
            Viewer::P2 => &mut self.second,
        }
    }

    /// Prepare exactly one command. Send it as this question's actor, then report
    /// its actual Submission before preparing another command.
    /// Tokens are transport coordinates, never passed to `BasicPolicy`.
    ///
    /// # Errors
    /// Returns the sticky stop reason for a limit, failed policy or invalid protocol.
    pub fn prepare(
        &mut self,
        observation: &Observation,
        question: &Question,
    ) -> Result<Command, HostOutcome> {
        if let Some(outcome) = self.outcome {
            return Err(outcome);
        }
        if self.pending.is_some() {
            return Err(self.stop(HostOutcome::ProtocolError));
        }
        if self.stats.submissions >= self.limits.max_submissions {
            return Err(self.stop(HostOutcome::StepLimit));
        }
        if observation.viewer != question.actor {
            return Err(self.stop(HostOutcome::ProtocolError));
        }
        let choice = match self
            .policy(question.actor)
            .choose(observation, &question.shape)
        {
            Ok(choice) => choice,
            Err(failure) => {
                return Err(self.stop(HostOutcome::PolicyFailure {
                    actor: question.actor,
                    failure,
                }));
            }
        };
        let command = command(&choice, question.token);
        self.pending = Some(Attempt {
            actor: question.actor,
            token: question.token,
            choice,
        });
        Ok(command)
    }

    /// Pair the public result with the outstanding command. Rejection permits a
    /// bounded fresh preparation; a rollback or cancellation is recorded and can continue.
    /// Returns a stop reason only when the host has stopped.
    pub fn record_submission(&mut self, submission: &Submission) -> Option<HostOutcome> {
        if self.outcome.is_some() {
            return self.outcome;
        }
        let Some(attempt) = self.pending.take() else {
            return Some(self.stop(HostOutcome::ProtocolError));
        };
        if self.stats.record(attempt.actor, submission.kind).is_none() {
            return Some(self.stop(HostOutcome::ProtocolError));
        }
        self.policy(attempt.actor)
            .record_submission(&attempt.choice, submission.kind);
        if submission.kind == SubmissionKind::EngineFault {
            return Some(self.stop(HostOutcome::EngineFault));
        }
        if let Some(outcome) = terminal(&submission.input) {
            return Some(self.stop(outcome));
        }
        if submission.kind != SubmissionKind::Rejected {
            self.rejected = None;
            return None;
        }
        let count = match self.rejected {
            Some((actor, token, count)) if actor == attempt.actor && token == attempt.token => {
                count.checked_add(1)
            }
            Some(_) | None => Some(1),
        };
        let Some(count) = count else {
            return Some(self.stop(HostOutcome::ProtocolError));
        };
        self.rejected = Some((attempt.actor, attempt.token, count));
        if count > self.limits.max_retries {
            return Some(self.stop(HostOutcome::RejectedLimit {
                actor: attempt.actor,
            }));
        }
        None
    }

    /// Drive an initialized typed Game using only `input`, `observe` and `submit_player`.
    /// Native terminal input takes precedence over a command limit. No authority setup,
    /// hidden state, diagnostic messages or search interface enters the policies.
    pub fn run(&mut self, game: &mut Game) -> HostReport {
        loop {
            if let Some(report) = self.report() {
                return report;
            }
            let (viewer, input) = match game.input(Viewer::P1) {
                Input::Waiting(waiting) => (waiting.actor, game.input(waiting.actor)),
                input @ (Input::Question(_) | Input::Finished { .. } | Input::EngineFault) => {
                    (Viewer::P1, input)
                }
            };
            if let Some(outcome) = terminal(&input) {
                self.stop(outcome);
                continue;
            }
            let Input::Question(question) = input else {
                self.stop(HostOutcome::ProtocolError);
                continue;
            };
            if question.actor != viewer {
                self.stop(HostOutcome::ProtocolError);
                continue;
            }
            let observation = match game.observe(viewer) {
                Ok(observation) => observation,
                Err(_error) => {
                    self.stop(HostOutcome::EngineFault);
                    continue;
                }
            };
            let command = match self.prepare(&observation, &question) {
                Ok(command) => command,
                Err(_outcome) => continue,
            };
            let submission = game.submit_player(viewer, command);
            let _outcome = self.record_submission(&submission);
        }
    }
}

const fn terminal(input: &Input) -> Option<HostOutcome> {
    match input {
        Input::Finished { winner } => Some(HostOutcome::Finished { winner: *winner }),
        Input::EngineFault => Some(HostOutcome::EngineFault),
        Input::Question(_) | Input::Waiting(_) => None,
    }
}

fn command(choice: &Choice, token: QuestionToken) -> Command {
    match choice {
        Choice::Opening(answer) => Command::Opening {
            token,
            answer: answer.clone(),
        },
        Choice::Closing(answer) => Command::Closing {
            token,
            answer: answer.clone(),
        },
        Choice::Evolution(answer) => Command::Evolution {
            token,
            answer: answer.clone(),
        },
        Choice::Decision(answer) => Command::Answer {
            token,
            answer: answer.clone(),
        },
        Choice::MainAction { intent, answer } => Command::MainAction {
            token,
            intent: intent.id.clone(),
            answer: answer.clone(),
        },
    }
}
