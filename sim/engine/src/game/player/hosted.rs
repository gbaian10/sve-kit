use super::{
    Command, DecisionShape, Input, Question, QuestionShape, Submission, SubmissionKind, Viewer,
    Waiting,
};
use crate::battle::{BattleReferences, EventFeed, FeedUpdate, ProjectionError};
use crate::game::Game;
use std::sync::OnceLock;
use sve_battle_contract::presentation::Notice;

#[derive(Default)]
pub(in crate::game) struct InputCache(Option<OnceLock<[Input; 2]>>);

impl Clone for InputCache {
    fn clone(&self) -> Self {
        // Rule probes and rollback targets are free to mutate independently.
        Self::default()
    }
}

impl InputCache {
    pub(super) const fn enabled(&self) -> bool {
        self.0.is_some()
    }

    pub(super) fn prepared(question: Question) -> Self {
        Self(Some(OnceLock::from(Self::split(Box::new(question)))))
    }

    pub(super) fn read(&self, game: &Game, viewer: Viewer) -> Input {
        let Some(stop) = &self.0 else {
            return game.compute_player_input(Some(viewer));
        };
        let [first, second] = stop.get_or_init(|| Self::read_stop(game));
        match viewer {
            Viewer::P1 => first,
            Viewer::P2 => second,
        }
        .clone()
    }

    fn read_stop(game: &Game) -> [Input; 2] {
        match game.compute_player_input(None) {
            Input::Question(question) => Self::split(question),
            Input::Finished { winner } => [Input::Finished { winner }, Input::Finished { winner }],
            // A private question can fail while its public waiting summary is still readable.
            Input::EngineFault | Input::Waiting(_) => {
                [Viewer::P1, Viewer::P2].map(|viewer| game.compute_player_input(Some(viewer)))
            }
        }
    }

    fn split(question: Box<Question>) -> [Input; 2] {
        // Match the public waiting summary in compute_player_input.
        let dice = match question.shape {
            QuestionShape::Decision(DecisionShape::DiceReroll { value, remaining }) => {
                Some((value, remaining))
            }
            QuestionShape::ActionWindow(_)
            | QuestionShape::Closing(_)
            | QuestionShape::Decision(_)
            | QuestionShape::Evolution(_)
            | QuestionShape::Opening(_) => None,
        };
        let actor = question.actor;
        let waiting = Input::Waiting(Waiting { actor, dice });
        let input = Input::Question(question);
        match actor {
            Viewer::P1 => [input, waiting],
            Viewer::P2 => [waiting, input],
        }
    }

    pub(super) fn after_submission(mut self, kind: SubmissionKind) -> Self {
        if kind != SubmissionKind::Rejected
            && let Some(stop) = &mut self.0
        {
            *stop = OnceLock::new();
        }
        self
    }
}

/// A player-controlled authority with inputs shared between submissions and projections.
#[derive(Debug)]
pub struct HostedGame(Game);

impl HostedGame {
    /// Take ownership after authority setup. The host can advance rules only by submitting commands.
    #[must_use]
    pub fn new(mut game: Game) -> Self {
        game.player_inputs = InputCache(Some(OnceLock::new()));
        Self(game)
    }

    /// Read the current authority without changing its rules state.
    #[must_use]
    pub const fn game(&self) -> &Game {
        &self.0
    }

    /// Submit an authenticated command and prepare the next input.
    #[must_use]
    pub fn submit(&mut self, viewer: Viewer, command: Command) -> Submission {
        self.0.submit_player(viewer, command)
    }

    /// Transfer short-lived disclosures to this seat's channel.
    /// # Errors
    /// Refuses unavailable references or disclosures.
    pub fn take_battle_notices(
        &mut self,
        viewer: Viewer,
        references: &BattleReferences,
    ) -> Result<Vec<Notice>, ProjectionError> {
        self.0.take_battle_notices(viewer, references)
    }

    /// Record the delivered state and collect the events for this update.
    pub fn deliver_events(
        &mut self,
        feed: &mut EventFeed,
        references: &BattleReferences,
        baseline: bool,
    ) -> FeedUpdate {
        if baseline {
            feed.baseline(&mut self.0)
        } else {
            feed.deliver(&mut self.0, references)
        }
    }
}
