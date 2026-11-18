//! Assisted play over two positions in the same typed authority record.

use super::player::{
    AttackTarget, Command, DecisionShape, Input, MainIntentKind, ObjectHandle, PendingChoice,
    QuestionShape, Submission, Viewer, observation::seat,
};
use super::replay::{RecordedCommand, Replay, ReplayNode, ReplaySummary};
use super::{Game, int};
use crate::{EngineFailure, Result, invalid};
use core::{convert::identity, result};
use serde_json::json;

/// Which line of an assisted game to inspect.
#[derive(Clone, Copy, Debug)]
pub enum Layer {
    /// The players' board, including any manual edit.
    Sandbox,
    /// The rules-checked position.
    Shadow,
}

/// One explicit manual edit of the sandbox's public board.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualOverride {
    /// Mark a controlled field follower as acted, as part of a manual attack.
    MarkAttack {
        /// The submitting seat's current attacker capability.
        attacker: ObjectHandle,
        /// The intended attack recipient.
        target: AttackTarget,
    },
    /// Correct a leader's current life without applying damage, healing or their triggers.
    SetLife {
        /// Leader whose life was corrected.
        player: Viewer,
        /// The value to enter.
        life: i64,
    },
}

/// The rule position against which the acting seat's edit was compared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expected {
    /// The leader's life on the shadow board.
    LeaderLife(i64),
    /// Whether the declared attack is offered, and any pending choices visible to that seat.
    Attack {
        /// Whether the shadow offers the exact declaration.
        allowed: bool,
        /// The seat's own pending choices if they block the declaration.
        pending: Vec<PendingChoice>,
    },
}

impl Expected {
    /// Rules that require resolving pending abilities before another main action.
    #[must_use]
    pub const fn rule_refs(&self) -> &'static [&'static str] {
        match self {
            Self::Attack { pending, .. } if !pending.is_empty() => &["7.3.4", "10.7.3.1"],
            Self::LeaderLife(_) | Self::Attack { .. } => &[],
        }
    }
}

/// How the two positions were brought back into alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Realign {
    /// Resume the rules-checked line before the edit.
    Rewind,
    /// Continue from the manual line.
    Adopt,
}

/// A divergence visible to the seat that made the edit.
#[derive(Clone, Debug)]
pub struct Divergence {
    /// The node immediately before the edit.
    pub before: ReplayNode,
    /// The alternate node produced by the edit.
    pub node: ReplayNode,
    /// What the acting seat could see in the shadow.
    pub expected: Expected,
    /// The seat's manual edit.
    pub actual: ManualOverride,
    /// The eventual realignment, if one was chosen.
    pub resolved: Option<Realign>,
    /// The seat that made and can inspect this edit.
    pub viewer: Viewer,
}

/// One match's aligned tip, optional manual branch, and divergence history.
#[derive(Debug)]
pub struct Assist {
    pub(crate) replay: Replay,
    aligned: ReplayNode,
    records: Vec<Divergence>,
}

impl Assist {
    /// Use a newly started match's replay as the authority record.
    ///
    /// # Errors
    /// The replay already advanced along its initial line.
    pub fn new(replay: Replay) -> Result<Self> {
        replay.require_tip(ReplayNode::ROOT)?;
        Ok(Self {
            replay,
            aligned: ReplayNode::ROOT,
            records: Vec::new(),
        })
    }

    pub(crate) fn node(&self, layer: Layer) -> ReplayNode {
        match layer {
            Layer::Sandbox => self.pending().map_or(self.aligned, |record| record.node),
            Layer::Shadow => self.aligned,
        }
    }

    /// The ordinary typed observation, question and events for this seat and layer.
    ///
    /// # Errors
    /// The seat observation is unavailable.
    pub fn summary(&self, layer: Layer, viewer: Viewer) -> Result<ReplaySummary> {
        self.replay.summary(self.node(layer), viewer)
    }

    pub(crate) fn pending(&self) -> Option<&Divergence> {
        self.records
            .last()
            .filter(|record| record.resolved.is_none())
    }

    /// Whether rules progression can accept commands. Only one divergence can be pending.
    #[must_use]
    pub fn auto_advance(&self) -> bool {
        self.pending().is_none()
    }

    /// Submit a normal typed command to the shared line.
    ///
    /// # Errors
    /// Progression is paused until the pending divergence is resolved.
    pub fn submit(&mut self, viewer: Viewer, command: Command) -> Result<Submission> {
        self.advance(identity, |game, record| {
            Ok(RecordedCommand::submit(game, record, viewer, command))
        })
    }

    pub(crate) fn advance<T, E>(
        &mut self,
        error: impl Fn(EngineFailure) -> E,
        action: impl FnOnce(&mut Game, &mut Vec<RecordedCommand>) -> result::Result<T, E>,
    ) -> result::Result<T, E> {
        if !self.auto_advance() {
            return Err(error(invalid("assist is paused at a divergence")));
        }
        let (node, result) = self.replay.advance(self.aligned, error, action)?;
        self.aligned = node;
        Ok(result)
    }

    /// Fork and apply a manual edit. Returns the divergence node if the board changed.
    ///
    /// # Errors
    /// A divergence is already pending, or the requested edit is unavailable to this seat.
    pub fn manual_override(
        &mut self,
        viewer: Viewer,
        command: ManualOverride,
    ) -> Result<Option<ReplayNode>> {
        if !self.auto_advance() {
            return Err(invalid(
                "resolve the pending divergence before editing again",
            ));
        }
        let Some((node, expected)) = self
            .replay
            .record_override(self.aligned, viewer, &command)?
        else {
            return Ok(None);
        };
        self.records.push(Divergence {
            before: self.aligned,
            node,
            viewer,
            expected,
            actual: command,
            resolved: None,
        });
        Ok(Some(node))
    }

    /// The seat's own divergences, including resolved records in their original order.
    pub fn divergences(&self, viewer: Viewer) -> impl Iterator<Item = &Divergence> {
        self.records
            .iter()
            .filter(move |record| record.viewer == viewer)
    }

    /// Recover the selected replay tip and resume ordinary commands from it.
    ///
    /// # Errors
    /// The node is not the pending divergence for this seat, or recovery fails.
    pub fn realign(&mut self, viewer: Viewer, node: ReplayNode, mode: Realign) -> Result<()> {
        let record = self
            .records
            .last_mut()
            .filter(|record| {
                record.resolved.is_none() && record.node == node && record.viewer == viewer
            })
            .ok_or_else(|| invalid("unknown pending divergence"))?;
        let next = match mode {
            Realign::Rewind => record.before,
            Realign::Adopt => record.node,
        };
        self.replay.recover_tip(next)?;
        self.aligned = next;
        record.resolved = Some(mode);
        Ok(())
    }
}

impl ManualOverride {
    pub(super) fn apply(&self, game: &mut Game, viewer: Viewer) -> Result<Option<Expected>> {
        let input = game.input(viewer);
        if matches!(input, Input::Finished { .. } | Input::EngineFault) {
            return Err(invalid("cannot edit a stopped game"));
        }
        let expected = match self {
            Self::SetLife { player, life } => {
                let leader = &mut game.player_mut(seat(*player))?.leader;
                let previous = int(&leader["life"]);
                if *life == previous {
                    return Ok(None);
                }
                if *life <= 0 {
                    return Err(invalid("manual life correction must be positive"));
                }
                leader["life"] = json!(life);
                Expected::LeaderLife(previous)
            }
            Self::MarkAttack { attacker, target } => {
                let id = field_object(game, viewer, attacker, viewer)?;
                if !game.object_type(id)?.contains("フォロワー") {
                    return Err(invalid("manual attacker must be a follower"));
                }
                match target {
                    AttackTarget::Leader(player) if *player == viewer => {
                        return Err(invalid("manual target must belong to the opponent"));
                    }
                    AttackTarget::Card(card) => {
                        let opponent = if viewer == Viewer::P1 {
                            Viewer::P2
                        } else {
                            Viewer::P1
                        };
                        field_object(game, viewer, card, opponent)?;
                    }
                    AttackTarget::Leader(_) => {}
                }
                let id = id.to_owned();
                let object = game.object_mut(&id)?;
                if object.state["acted"] == true {
                    return Ok(None);
                }
                object.state["acted"] = json!(true);
                attack_expectation(input, attacker, target)
            }
        };
        // Adopting an edit must invalidate commands issued on the pre-edit board.
        game.player_epoch = game
            .player_epoch
            .checked_add(1)
            .ok_or_else(|| invalid("player input epoch exhausted"))?;
        Ok(Some(expected))
    }
}

fn field_object<'game>(
    game: &'game Game,
    viewer: Viewer,
    handle: &ObjectHandle,
    controller: Viewer,
) -> Result<&'game str> {
    let id = game.resolve_player_object(viewer, handle)?;
    let object = game.object(id)?;
    if object.zone != "field" || object.controller != seat(controller) {
        return Err(invalid(
            "manual attack requires a field card of the indicated seat",
        ));
    }
    Ok(id)
}

fn attack_expectation(input: Input, attacker: &ObjectHandle, target: &AttackTarget) -> Expected {
    let mut pending = Vec::new();
    let mut allowed = false;
    if let Input::Question(question) = input {
        match question.shape {
            QuestionShape::ActionWindow(actions) => {
                allowed = actions.intents.iter().any(|intent| {
                    matches!(&intent.kind, MainIntentKind::Attack {
                        attacker: offered, targets,
                    } if offered.handle == *attacker && targets.contains(target))
                });
            }
            QuestionShape::Decision(DecisionShape::Pending(choices)) => pending = choices,
            QuestionShape::Evolution(_)
            | QuestionShape::Opening(_)
            | QuestionShape::Closing(_)
            | QuestionShape::Decision(_) => {}
        }
    }
    Expected::Attack { allowed, pending }
}
