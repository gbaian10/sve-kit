//! In-process authority records for typed games.

use super::Game;
use super::player::events::{EventError, PlayerEventBatch};
use super::player::observation::Observation;
use super::player::{
    Command, Input, PlayerDeck, StrictStartError, Submission, SubmissionKind, Viewer,
};
use crate::catalog::SelectedEffects;
use crate::{EngineFailure, Result, invalid};
use alloc::collections::{BTreeMap, BTreeSet};
use core::{convert::identity, fmt, result};
#[cfg(feature = "runner")]
use serde_json::Value;

#[derive(Clone, Default)]
pub(super) struct Coordinates {
    pub(super) node: Option<String>,
    pub(super) handle_line: Option<usize>,
    #[cfg(feature = "runner")]
    pub(super) executions: BTreeMap<u64, (String, Option<Value>)>,
}

/// One immutable position in a match's record. Use only with the issuing record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReplayNode(usize);

impl ReplayNode {
    /// The position at initialization, before any player submission.
    pub const ROOT: Self = Self(0);
}

#[derive(Clone)]
enum Start {
    Decks {
        decks: [PlayerDeck; 2],
        first_chooser: Viewer,
    },
    #[cfg(feature = "runner")]
    Position {
        setup: Value,
        facts: Value,
        random: Value,
    },
}

impl Start {
    fn game(&self, selected: &SelectedEffects, seed: &str) -> Result<Game> {
        match self {
            Self::Decks {
                decks,
                first_chooser,
            } => Game::start_player_game(selected.clone(), decks.clone(), *first_chooser, seed)
                .map_err(|_error| invalid("recorded decks no longer start")),
            #[cfg(feature = "runner")]
            Self::Position {
                setup,
                facts,
                random,
            } => {
                let mut game =
                    Game::from_selected_effects(selected.clone(), setup, facts, random, seed)?;
                game.referee_record_events();
                game.advance_player_input();
                if let Some(error) = game.referee_fault() {
                    return Err(error.clone());
                }
                Ok(game)
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct RecordedCommand {
    viewer: Viewer,
    command: Command,
    before: Input,
    result: Submission,
}

impl RecordedCommand {
    pub(crate) fn submit(
        game: &mut Game,
        record: &mut Vec<Self>,
        viewer: Viewer,
        command: Command,
    ) -> Submission {
        let before = game.input(viewer);
        let result = game.submit_player(viewer, command.clone());
        if result.kind != SubmissionKind::Rejected {
            record.push(Self {
                viewer,
                command,
                before,
                result: result.clone(),
            });
        }
        result
    }

    #[cfg(feature = "runner")]
    pub(crate) const fn kind(&self) -> SubmissionKind {
        self.result.kind
    }
}

enum Operation {
    Start,
    Commands(Vec<RecordedCommand>),
    Fork { observed: ReplayNode },
}

struct Node {
    game: Game,
    parent: ReplayNode,
    operation: Operation,
    line: ReplayNode,
}

/// A node as one seat saw it, with the committed events added at that node.
/// Parent event coordinates can refer to earlier summaries on the same line.
#[derive(Debug)]
pub struct ReplaySummary {
    /// Closed observation for that seat.
    pub observation: Observation,
    /// That seat's input at this stop.
    pub input: Input,
    /// Batches that became committed at this stop.
    pub events: result::Result<Vec<PlayerEventBatch>, EventError>,
}

/// Trusted host's record with the exact accepted selection, initialization and typed commands.
/// The pinned selection and the current process pin the rule inputs and RNG implementation.
pub struct Replay {
    selected: SelectedEffects,
    initial: Start,
    seed: String,
    nodes: Vec<Node>,
    tails: BTreeMap<ReplayNode, ReplayNode>,
}

impl fmt::Debug for Replay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Replay")
            .field("nodes", &self.nodes.len())
            .finish_non_exhaustive()
    }
}

impl Replay {
    /// Start from the same validated decks and selected effects as ordinary play.
    ///
    /// # Errors
    /// Returns the normal strict startup errors.
    pub fn start(
        selected: SelectedEffects,
        decks: [PlayerDeck; 2],
        first_chooser: Viewer,
        seed: String,
    ) -> result::Result<Self, StrictStartError> {
        let game = Game::start_player_game(selected.clone(), decks.clone(), first_chooser, &seed)?;
        Ok(Self::new(
            selected,
            Start::Decks {
                decks,
                first_chooser,
            },
            seed,
            game,
        ))
    }

    #[cfg(feature = "runner")]
    pub(crate) fn from_position(
        selected: SelectedEffects,
        setup: Value,
        facts: Value,
        random: Value,
        seed: String,
    ) -> Result<Self> {
        let initial = Start::Position {
            setup,
            facts,
            random,
        };
        let game = initial.game(&selected, &seed)?;
        Ok(Self::new(selected, initial, seed, game))
    }

    fn new(selected: SelectedEffects, initial: Start, seed: String, mut game: Game) -> Self {
        game.begin_replay_node(0);
        Self {
            selected,
            initial,
            seed,
            nodes: vec![Node {
                game,
                parent: ReplayNode::ROOT,
                operation: Operation::Start,
                line: ReplayNode::ROOT,
            }],
            tails: BTreeMap::from([(ReplayNode::ROOT, ReplayNode::ROOT)]),
        }
    }

    fn node(&self, at: ReplayNode) -> Result<&Node> {
        self.nodes
            .get(at.0)
            .ok_or_else(|| invalid("unknown replay node"))
    }

    pub(crate) fn game(&self, at: ReplayNode) -> Result<&Game> {
        Ok(&self.node(at)?.game)
    }

    /// Submit to a line's current node. A rejected command returns that same node.
    ///
    /// # Errors
    /// Rejects a position other than a current line tip.
    pub fn submit(
        &mut self,
        at: ReplayNode,
        viewer: Viewer,
        command: Command,
    ) -> Result<(ReplayNode, Submission)> {
        let (node, result, _commands) = self.advance(at, identity, |game, commands| {
            Ok(RecordedCommand::submit(game, commands, viewer, command))
        })?;
        Ok((node, result))
    }

    pub(crate) fn advance<T, E>(
        &mut self,
        at: ReplayNode,
        error: impl Fn(EngineFailure) -> E,
        action: impl FnOnce(&mut Game, &mut Vec<RecordedCommand>) -> result::Result<T, E>,
    ) -> result::Result<(ReplayNode, T, Vec<SubmissionKind>), E> {
        let parent = self.node(at).map_err(&error)?;
        if self.tails.get(&parent.line).copied() != Some(at) {
            return Err(error(invalid("decisions append only at a replay line tip")));
        }
        let line = parent.line;
        let mut game = parent.game.clone();
        let id = ReplayNode(self.nodes.len());
        game.begin_replay_node(id.0);
        let mut commands = Vec::new();
        let result = action(&mut game, &mut commands)?;
        if commands.is_empty() {
            return Ok((at, result, Vec::new()));
        }
        let receipts = commands.iter().map(|entry| entry.result.kind).collect();
        self.push(
            id,
            Node {
                game,
                parent: at,
                operation: Operation::Commands(commands),
                line,
            },
        );
        Ok((id, result, receipts))
    }

    fn push(&mut self, id: ReplayNode, node: Node) {
        self.tails.insert(node.line, id);
        self.nodes.push(node);
    }

    /// Open a new line at any previous node, retaining what each seat learned on
    /// its source line, without importing locations from that line.
    ///
    /// # Errors
    /// Rejects an unknown position or a damaged record.
    pub fn branch(&mut self, at: ReplayNode) -> Result<ReplayNode> {
        let point = self.node(at)?;
        let observed = self
            .tails
            .get(&point.line)
            .copied()
            .ok_or_else(|| invalid("unknown replay line"))?;
        let id = ReplayNode(self.nodes.len());
        let mut game = point.game.clone();
        game.fork_replay_knowledge(&self.node(observed)?.game, observed != at, id.0);
        self.push(
            id,
            Node {
                game,
                parent: at,
                operation: Operation::Fork { observed },
                line: id,
            },
        );
        Ok(id)
    }

    /// Rebuild this position by initializing the match and checking the outcome
    /// and resulting input of every recorded command needed by this position.
    ///
    /// # Errors
    /// Rejects an unknown position or a command whose result fails to reproduce.
    pub fn recover(&self, at: ReplayNode) -> Result<Game> {
        let mut needed = BTreeSet::new();
        let mut remaining = vec![at];
        while let Some(id) = remaining.pop() {
            if !needed.insert(id.0) {
                continue;
            }
            let node = self.node(id)?;
            if !matches!(node.operation, Operation::Start) {
                remaining.push(node.parent);
            }
            if let Operation::Fork { observed } = node.operation {
                remaining.push(observed);
            }
        }
        let mut games = BTreeMap::<usize, Game>::new();
        for index in needed {
            let node = self.node(ReplayNode(index))?;
            let mut game = if matches!(node.operation, Operation::Start) {
                self.initial.game(&self.selected, &self.seed)?
            } else {
                games
                    .get(&node.parent.0)
                    .ok_or_else(|| invalid("missing replay parent"))?
                    .clone()
            };
            match &node.operation {
                Operation::Start => game.begin_replay_node(0),
                Operation::Commands(commands) => {
                    game.begin_replay_node(index);
                    for entry in commands {
                        if game.input(entry.viewer) != entry.before
                            || game.submit_player(entry.viewer, entry.command.clone())
                                != entry.result
                        {
                            return Err(invalid("recorded command did not reproduce its result"));
                        }
                    }
                }
                Operation::Fork { observed } => {
                    let source = games
                        .get(&observed.0)
                        .ok_or_else(|| invalid("missing replay source"))?;
                    game.fork_replay_knowledge(source, *observed != node.parent, index);
                }
            }
            games.insert(index, game);
        }
        games
            .remove(&at.0)
            .ok_or_else(|| invalid("missing recovered replay node"))
    }

    /// Read only one seat's view at this position, using the same observation and
    /// event journal as live play.
    ///
    /// # Errors
    /// Rejects an unknown position or an unavailable seat observation.
    pub fn summary(&self, at: ReplayNode, viewer: Viewer) -> Result<ReplaySummary> {
        let node = self.node(at)?;
        let previous = self.game(node.parent)?;
        let events = (|| {
            let after = if at == ReplayNode::ROOT {
                None
            } else {
                previous.events(viewer, None)?.last().map(|event| &event.id)
            };
            Ok(node.game.event_batches(viewer, after)?.cloned().collect())
        })();
        Ok(ReplaySummary {
            observation: node.game.observe(viewer)?,
            input: node.game.input(viewer),
            events,
        })
    }
}
