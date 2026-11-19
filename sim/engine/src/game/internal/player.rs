//! Authority-only adapters keep player tokens and results free of execution coordinates.

use super::{
    Answer, CardPlay, DecisionToken, DecisionView, Execution, Fact, Game, PlayerId, Result, Wait,
    invalid,
};
use crate::game::player::{QuestionPurpose, SubmissionKind};
use crate::ir::DecisionKind;

mod candidates;
pub(super) use candidates::CandidateLinks;
mod projection;

pub(super) const fn purpose(kind: &DecisionKind) -> QuestionPurpose {
    match kind {
        DecisionKind::PlayParameter { .. } => QuestionPurpose::PlayParameter,
        DecisionKind::ResolutionChoice { .. } => QuestionPurpose::ResolutionChoice,
        DecisionKind::RuleChoice { .. } => QuestionPurpose::RuleChoice,
        DecisionKind::QuickWindow { .. } => QuestionPurpose::QuickWindow,
    }
}

impl Wait {
    const fn purpose(&self) -> QuestionPurpose {
        match self {
            Self::Targets(wait) | Self::ResolutionTargets(wait) => purpose(&wait.decision.kind),
            Self::RecipientTargets(wait) => wait.purpose,
            Self::Distribution(wait) => wait.purpose,
            Self::Integer { purpose, .. } => *purpose,
            Self::ResourceEntryPayment(_)
            | Self::CostDiscard(_)
            | Self::OptionalCost
            | Self::CostMaterials(_)
            | Self::ExchangeRoute { .. }
            | Self::MaterialParameter(_)
            | Self::Modes(_) => QuestionPurpose::PlayParameter,
            Self::WardEntry { .. }
            | Self::TriggerQuota { .. }
            | Self::TokenCapacity(_)
            | Self::MovementCapacity(_)
            | Self::DamageReplacements(_)
            | Self::Drive(_)
            | Self::SearchReplacement(_) => QuestionPurpose::RuleChoice,
            Self::Dice(_)
            | Self::Ability(_)
            | Self::Discard(_)
            | Self::ResolutionPayment { .. }
            | Self::Alternative(_)
            | Self::May { .. }
            | Self::Order(_)
            | Self::Search(_)
            | Self::SearchPartition(_)
            | Self::TokenAllocation(_)
            | Self::TransformTemplates(_)
            | Self::Draw { .. } => QuestionPurpose::ResolutionChoice,
        }
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Answer validation marks the existing execution's origin handoff."
)]
impl Execution {
    pub(super) const fn answer_origin(&mut self, validated: &mut bool) -> Option<Box<Game>> {
        *validated = true;
        self.origin.take()
    }

    /// Whether this play itself has been played (step 7).
    fn played(&self) -> bool {
        self.facts.iter().any(|fact| {
            matches!(
                fact,
                Fact::CardPlayed { execution, .. } | Fact::AbilityPlayed { execution, .. }
                    if *execution == self.id
            )
        })
    }

    /// Whether this play and every play enclosing it have been played (step 7).
    /// Before that, payment, capacity or a declined head cost can still undo a layer by rules.
    fn established(&self) -> bool {
        self.played()
            && self
                .nested
                .as_ref()
                .is_none_or(|nested| nested.parent.established())
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "The player adapter shares existing authority dispatch and rollback."
)]
impl Game {
    /// A real input stop inside an established play that keeps its origin: from here only an
    /// internal fault can discard what has completed. Procedures that are not plays, and
    /// suspensions without an origin, never qualify.
    pub(in crate::game) fn provisional_stop(&self) -> bool {
        self.player_fault.is_none()
            && self.state.game["ended"] != true
            && self.typed.as_ref().is_some_and(|execution| {
                execution.waiting.is_some() && execution.origin.is_some() && execution.established()
            })
    }

    /// The journal position at which the suspended card or ability play began, while it has
    /// not been played yet: what it recorded since is its declaration and payment so far,
    /// not a result. A procedure that is not a play never has one.
    pub(in crate::game) fn unplayed_since(&self) -> Option<u64> {
        self.typed
            .as_ref()
            .filter(|execution| {
                (execution.ability.is_some() || execution.field_entry) && !execution.played()
            })
            .map(|execution| execution.began)
    }

    /// The origin and controller of a play its controller started itself and has not played yet
    /// (step 7). Until then the play may still be withdrawn or reverted, so other seats keep
    /// seeing its origin. Automatic abilities and plays nested in a resolution are excluded:
    /// they belong to a procedure already under way.
    pub(in crate::game) fn concealed_declaration(&self) -> Option<(&Self, PlayerId)> {
        let execution = self.typed.as_ref()?;
        if !(execution.ability.is_some() || execution.field_entry)
            || execution.nested.is_some()
            || execution.trigger_occurrence.is_some()
            || execution.played()
        {
            return None;
        }
        let origin = execution.origin.as_deref()?;
        Some((origin, execution.context.controller))
    }

    /// Whether the waiting question is a parameter of its own concealed declaration.
    pub(in crate::game) fn withdrawable_declaration(&self) -> bool {
        self.concealed_declaration()
            .zip(
                self.typed
                    .as_ref()
                    .and_then(|execution| execution.waiting.as_ref()),
            )
            .is_some_and(|((_, controller), (question, wait))| {
                wait.purpose() == QuestionPurpose::PlayParameter && question.actor == controller
            })
    }

    /// Return the withdrawn play to its origin through the same restoration an illegal play uses.
    pub(in crate::game) fn withdraw_declaration(&mut self) -> Result<()> {
        if !self.withdrawable_declaration() {
            return Err(invalid("no withdrawable declaration"));
        }
        let published = self.published_knowledge();
        let concealed = self
            .concealed_declaration()
            .map(|(_, controller)| controller);
        let origin = self
            .typed
            .as_mut()
            .and_then(|execution| execution.origin.take())
            .ok_or_else(|| invalid("withdrawn play lost its origin"))?;
        self.restore_play_origin(*origin, published, concealed)
    }

    pub(in crate::game) fn player_decision(&self) -> Option<(DecisionView, QuestionPurpose)> {
        if let Some((question, wait)) = self
            .typed
            .as_ref()
            .and_then(|execution| execution.waiting.as_ref())
        {
            return Some((question.clone(), wait.purpose()));
        }
        if let Some((question, _)) = &self.card_runtime.question {
            return Some((question.clone(), QuestionPurpose::RuleChoice));
        }
        self.card_runtime
            .quick
            .as_ref()
            .map(|question| (question.view.clone(), QuestionPurpose::QuickWindow))
    }

    pub(in crate::game) fn answer_player_decision(
        &mut self,
        actor: PlayerId,
        token: DecisionToken,
        answer: Answer,
        validated: &mut bool,
    ) -> Result<SubmissionKind> {
        if self.card_runtime.quick.is_some()
            || self.card_runtime.question.is_some()
            || self
                .typed
                .as_ref()
                .is_some_and(|execution| execution.ability.is_some() || execution.field_entry)
        {
            self.answer_internal_card_validated(actor, token, answer, validated)
                .map(|result| match result {
                    CardPlay::Cancelled => SubmissionKind::Cancelled,
                    _ => SubmissionKind::Applied,
                })
        } else {
            self.answer_internal_step(actor, token, answer, validated)
                .map(|_progress| SubmissionKind::Applied)
        }
    }
}
