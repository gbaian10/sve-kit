//! Translation between the neutral architecture runner and the typed authority record.

use super::{adapter, awaiting_on, decide_on};
use crate::adapter::{core_view, error};
use crate::catalog::SelectedEffects;
use crate::game::Game;
use crate::game::player::SubmissionKind;
use crate::game::replay::{Replay, ReplayNode};
use crate::random::ALGORITHM;
use alloc::collections::BTreeMap;
use core::fmt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use sve_scenario_runner::arch::{BranchKind, NodeId, Observation, ReplayEngine};
use sve_scenario_runner::{EngineError, Fixture, Step, View};

#[derive(Clone, Serialize, Deserialize)]
struct Position {
    setup: Value,
    facts: Value,
    random: Value,
    seed: String,
}

#[derive(Clone, Serialize, Deserialize)]
enum Operation {
    Decision {
        at: String,
        decision: Value,
        receipts: Vec<SubmissionKind>,
        outcome: String,
    },
    Branch {
        at: String,
        undo: bool,
    },
}

// The fixture vocabulary is sufficient to reproduce native commands for this runner.
// This blob is private to the adapter. See docs/sim/authority-recovery.md for the
// requirements of a future persistent authority protocol.
#[derive(Serialize, Deserialize)]
struct Archive {
    selection: Vec<u8>,
    random_algorithm: String,
    initial: Position,
    operations: Vec<Operation>,
    selected: String,
}

struct Node {
    native: ReplayNode,
    operation_count: usize,
    parent: Option<String>,
    decision: Option<Value>,
    events: Vec<Value>,
}

/// Architecture runner using the typed player interface and its authority record.
pub struct TypedReplayAdapter {
    selected: SelectedEffects,
    replay: Option<Replay>,
    initial: Option<Position>,
    nodes: BTreeMap<String, Node>,
    operations: Vec<Operation>,
    admin: Vec<Value>,
}

impl fmt::Debug for TypedReplayAdapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TypedReplayAdapter")
            .field("nodes", &self.nodes.len())
            .finish_non_exhaustive()
    }
}

impl TypedReplayAdapter {
    /// Pin the same admitted effects and rules for initialization and recovery.
    #[must_use]
    pub const fn new(selected: SelectedEffects) -> Self {
        Self {
            selected,
            replay: None,
            initial: None,
            nodes: BTreeMap::new(),
            operations: Vec::new(),
            admin: Vec::new(),
        }
    }

    fn start_position(&mut self, position: Position) -> Result<NodeId, EngineError> {
        self.replay = Some(
            Replay::from_position(
                self.selected.clone(),
                position.setup.clone(),
                position.facts.clone(),
                position.random.clone(),
                position.seed.clone(),
            )
            .map_err(error)?,
        );
        self.initial = Some(position);
        self.nodes.clear();
        self.operations.clear();
        self.admin.clear();
        Ok(self.insert(Node {
            native: ReplayNode::ROOT,
            operation_count: 0,
            parent: None,
            decision: None,
            events: Vec::new(),
        }))
    }

    fn insert(&mut self, mut node: Node) -> NodeId {
        node.operation_count = self.operations.len();
        let id = format!("n{}", self.nodes.len());
        self.nodes.insert(id.clone(), node);
        NodeId(id)
    }

    fn node(&self, at: &NodeId) -> Result<&Node, EngineError> {
        self.nodes
            .get(&at.0)
            .ok_or_else(|| adapter("unknown replay node"))
    }

    fn replay(&self) -> Result<&Replay, EngineError> {
        self.replay
            .as_ref()
            .ok_or_else(|| adapter("replay not started"))
    }

    fn replay_mut(&mut self) -> Result<&mut Replay, EngineError> {
        self.replay
            .as_mut()
            .ok_or_else(|| adapter("replay not started"))
    }

    fn position(&self) -> Result<&Position, EngineError> {
        self.initial
            .as_ref()
            .ok_or_else(|| adapter("replay not started"))
    }

    fn game(&self, at: &NodeId) -> Result<&Game, EngineError> {
        self.replay()?.game(self.node(at)?.native).map_err(error)
    }

    fn selection_identity(&self) -> Result<Vec<u8>, EngineError> {
        self.selected.check_runtime().map_err(error)?;
        let programs: Vec<_> = self
            .selected
            .current
            .iter()
            .map(|(printing, card)| {
                (
                    printing,
                    &card.source_version,
                    &card.expanded,
                    &card.source_payload,
                    &card.face_ordinals,
                )
            })
            .collect();
        let exact = serde_json::to_vec(&(&self.selected.prototypes, programs))
            .map_err(|e| adapter(e.to_string()))?;
        Ok(Sha256::digest(exact).to_vec())
    }

    fn record_decision(
        &mut self,
        at: &NodeId,
        decision: &Value,
    ) -> Result<(NodeId, Step), EngineError> {
        let native = self.node(at)?.native;
        let initial = self
            .initial
            .as_ref()
            .ok_or_else(|| adapter("replay not started"))?;
        let replay = self
            .replay
            .as_mut()
            .ok_or_else(|| adapter("replay not started"))?;
        let (next, step, receipts) = replay.advance(native, error, |game, commands| {
            decide_on(game, &initial.setup, decision, Some(commands))
        })?;
        if next == native {
            return Ok((at.clone(), step));
        }
        self.operations.push(Operation::Decision {
            at: at.0.clone(),
            decision: decision.clone(),
            receipts,
            outcome: step.outcome.clone(),
        });
        let node = self.insert(Node {
            native: next,
            operation_count: 0,
            parent: Some(at.0.clone()),
            decision: Some(decision.clone()),
            events: step.events.clone(),
        });
        Ok((node, step))
    }

    fn rebuild(&mut self, operation: Operation) -> Result<(), EngineError> {
        match operation {
            Operation::Decision {
                at,
                decision,
                receipts,
                outcome,
            } => {
                let source = NodeId(at);
                let (next, step) = self.record_decision(&source, &decision)?;
                if let Some(Operation::Decision {
                    receipts: actual, ..
                }) = self.operations.last()
                    && source != next
                    && *actual == receipts
                    && step.outcome == outcome
                {
                    return Ok(());
                }
                Err(adapter("recorded decision did not reproduce its result"))
            }
            Operation::Branch { at, undo } => {
                let kind = if undo {
                    BranchKind::Undo
                } else {
                    BranchKind::Replay
                };
                self.branch(&NodeId(at), kind)?;
                Ok(())
            }
        }
    }
}

impl ReplayEngine for TypedReplayAdapter {
    fn start(&mut self, fixture: &Fixture, seed: &str, _game: &str) -> Result<NodeId, EngineError> {
        self.start_position(Position {
            setup: fixture.setup.clone(),
            facts: fixture.card_facts.clone(),
            random: fixture.random.clone(),
            seed: seed.into(),
        })
    }

    fn decide(&mut self, at: &NodeId, decision: &Value) -> Result<(NodeId, Step), EngineError> {
        self.record_decision(at, decision)
    }

    fn branch(&mut self, at: &NodeId, kind: BranchKind) -> Result<NodeId, EngineError> {
        let native = self.node(at)?.native;
        let next = self.replay_mut()?.branch(native).map_err(error)?;
        let undo = kind == BranchKind::Undo;
        self.operations.push(Operation::Branch {
            at: at.0.clone(),
            undo,
        });
        let node = self.insert(Node {
            native: next,
            operation_count: 0,
            parent: Some(at.0.clone()),
            decision: None,
            events: Vec::new(),
        });
        self.admin.push(
            json!({"kind": if undo { "undo" } else { "branch" }, "from": at.0, "to": node.0}),
        );
        Ok(node)
    }

    fn save(&self, at: &NodeId) -> Result<Vec<u8>, EngineError> {
        let selected = self.node(at)?;
        let archive = Archive {
            selection: self.selection_identity()?,
            random_algorithm: ALGORITHM.into(),
            initial: self.position()?.clone(),
            operations: self
                .operations
                .iter()
                .take(selected.operation_count)
                .cloned()
                .collect(),
            selected: at.0.clone(),
        };
        serde_json::to_vec(&archive).map_err(|e| adapter(e.to_string()))
    }

    fn restore(&mut self, blob: &[u8]) -> Result<NodeId, EngineError> {
        let archive: Archive = serde_json::from_slice(blob).map_err(|e| adapter(e.to_string()))?;
        if archive.random_algorithm != ALGORITHM
            || archive.selection != self.selection_identity()?
        {
            return Err(adapter(
                "replay inputs differ from the recorded selection or random algorithm",
            ));
        }
        let mut rebuilt = Self::new(self.selected.clone());
        rebuilt.start_position(archive.initial)?;
        for operation in archive.operations {
            rebuilt.rebuild(operation)?;
        }
        let selected = NodeId(archive.selected);
        rebuilt.node(&selected)?;
        *self = rebuilt;
        Ok(selected)
    }

    fn export(&self, at: &NodeId, view: View) -> Result<Value, EngineError> {
        Ok(self.observe(at, view)?.projection)
    }

    fn digest(&self, at: &NodeId) -> Result<String, EngineError> {
        self.game(at)?.referee_replay_digest().map_err(error)
    }

    fn observe(&self, at: &NodeId, view: View) -> Result<Observation, EngineError> {
        let game = self.game(at)?;
        let mut projection = game.projection(core_view(view)).map_err(error)?;
        let awaiting = awaiting_on(game, view);
        let knowledge = projection.get("knowledge").cloned().unwrap_or_default();
        if let Some(fields) = projection.as_object_mut() {
            fields.insert("awaiting".into(), awaiting.clone().unwrap_or_default());
            if view != View::Omniscient {
                for authority in ["legal", "flow", "continuation", "semantic_state"] {
                    fields.remove(authority);
                }
            }
        }
        Ok(Observation {
            projection,
            awaiting,
            knowledge,
        })
    }

    fn query(&self, at: &NodeId, view: View, path: &str) -> Result<Option<Value>, EngineError> {
        Ok(Game::packet_query(&self.export(at, view)?, path))
    }

    fn decisions(&self, at: &NodeId) -> Result<Vec<Value>, EngineError> {
        let mut decisions = Vec::new();
        let mut next = Some(at.clone());
        while let Some(id) = next.take() {
            let node = self.node(&id)?;
            decisions.extend(node.decision.iter().cloned());
            next = node.parent.clone().map(NodeId);
        }
        decisions.reverse();
        Ok(decisions)
    }

    fn events(&self, at: &NodeId) -> Result<Vec<Value>, EngineError> {
        Ok(self.node(at)?.events.clone())
    }

    fn admin_log(&self) -> Result<Vec<Value>, EngineError> {
        Ok(self.admin.clone())
    }
}
