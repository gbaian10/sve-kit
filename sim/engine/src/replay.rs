//! Append-only replay nodes, trusted saves and administrative branches.

use alloc::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::game::{Game, Step};
use crate::{Result, invalid};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Node {
    parent: Option<String>,
    branch: String,
    game: Game,
    decision: Option<Value>,
    step: Step,
}

/// An immutable snapshot graph. Administrative branches never rewrite old decisions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Replay {
    name: String,
    next: u64,
    nodes: BTreeMap<String, Node>,
    tails: BTreeMap<String, String>,
    admin: Vec<Value>,
}

impl Replay {
    /// Starts a graph with the given opening snapshot.
    #[must_use]
    pub fn start(mut game: Game, name: &str) -> (Self, String) {
        let root = format!("{name}:n0");
        game.opening_node(&root);
        let node = Node {
            parent: None,
            branch: root.clone(),
            game,
            decision: None,
            step: Step {
                outcome: "opening".into(),
                events: Vec::new(),
            },
        };
        (
            Self {
                name: name.into(),
                next: 1,
                nodes: BTreeMap::from([(root.clone(), node)]),
                tails: BTreeMap::from([(root.clone(), root.clone())]),
                admin: Vec::new(),
            },
            root,
        )
    }

    /// Appends a player decision at a branch tail.
    ///
    /// # Errors
    /// The node is missing, is not a tail, or the engine cannot execute the decision.
    pub fn decide(&mut self, at: &str, decision: &Value) -> Result<(String, Step)> {
        let mut node = self.node(at)?.clone();
        if self.tails.get(&node.branch).is_none_or(|tail| tail != at) {
            return Err(invalid("decisions append only at branch tails"));
        }
        let id = self.allocate();
        let step = node.game.decide(decision, &id)?;
        node.parent = Some(at.into());
        node.decision = Some(decision.clone());
        node.step = step.clone();
        self.tails.insert(node.branch.clone(), id.clone());
        self.nodes.insert(id.clone(), node);
        Ok((id, step))
    }

    /// Starts an administrative branch, carrying each observer's own previously seen identities.
    ///
    /// # Errors
    /// The source node is unknown.
    pub fn branch(&mut self, at: &str, undo: bool) -> Result<String> {
        let original = self.node(at)?.clone();
        let mut game = original.game;
        if let Some(tail) = self.tails.get(&original.branch)
            && tail != at
        {
            game.carry_from(&self.node(tail)?.game);
        }
        let id = self.allocate();
        self.nodes.insert(
            id.clone(),
            Node {
                parent: Some(at.into()),
                branch: id.clone(),
                game,
                decision: None,
                step: Step {
                    outcome: "branch".into(),
                    events: Vec::new(),
                },
            },
        );
        self.tails.insert(id.clone(), id.clone());
        self.admin
            .push(json!({"kind":if undo {"undo"} else {"branch"},"from":at,"to":id}));
        Ok(id)
    }

    /// Saves the graph with stable node/event identities and an explicit selected position.
    ///
    /// # Errors
    /// The node is unknown or serialization fails.
    pub fn save(&self, at: &str) -> Result<Vec<u8>> {
        self.node(at)?;
        serde_json::to_vec(&("astra-save/1", self, at)).map_err(invalid)
    }

    /// Restores a trusted server save into a fresh engine instance.
    ///
    /// # Errors
    /// The version or graph is invalid.
    pub fn restore(blob: &[u8]) -> Result<(Self, String)> {
        let (version, replay, at): (String, Self, String) =
            serde_json::from_slice(blob).map_err(invalid)?;
        if version != "astra-save/1" {
            return Err(invalid("unsupported save version"));
        }
        replay.node(&at)?;
        Ok((replay, at))
    }

    /// Authoritative snapshot at one immutable node.
    ///
    /// # Errors
    /// The node is unknown.
    pub fn game(&self, at: &str) -> Result<&Game> {
        Ok(&self.node(at)?.game)
    }

    /// Decision sequence on the selected line, excluding administrative operations.
    ///
    /// # Errors
    /// A parent node is unknown.
    pub fn decisions(&self, at: &str) -> Result<Vec<Value>> {
        let mut result = Vec::new();
        let mut current = Some(at);
        while let Some(id) = current {
            let node = self.node(id)?;
            if let Some(decision) = &node.decision {
                result.push(decision.clone());
            }
            current = node.parent.as_deref();
        }
        result.reverse();
        Ok(result)
    }

    /// Unchanged raw events emitted by this node's transition.
    ///
    /// # Errors
    /// The node is unknown.
    pub fn events(&self, at: &str) -> Result<Vec<Value>> {
        Ok(self.node(at)?.step.events.clone())
    }

    /// Immutable administrative log.
    #[must_use]
    pub fn admin_log(&self) -> Vec<Value> {
        self.admin.clone()
    }

    fn node(&self, id: &str) -> Result<&Node> {
        self.nodes
            .get(id)
            .ok_or_else(|| invalid(format!("unknown replay node: {id}")))
    }
    fn allocate(&mut self) -> String {
        let id = format!("{}:n{}", self.name, self.next);
        self.next = self.next.saturating_add(1);
        id
    }
}
