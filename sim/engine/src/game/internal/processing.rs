//! Procedure boundaries stay in the runtime queue, separate from authored IR.

use super::{Execution, Result};
use crate::ir::{Node, ScopeId};

/// A typed question's producer-recorded phase, separate from its purpose and answer domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestionStage {
    /// An effect is offering its resolution payment.
    Pay,
    /// An effect is offering its optional resolution body.
    Resolve,
}

#[derive(Clone, Debug)]
pub(super) enum Queued {
    Node(Box<Node>),
    EndProcedure((ScopeId, u64)),
}

impl From<Node> for Queued {
    fn from(node: Node) -> Self {
        Self::Node(Box::new(node))
    }
}

impl Queued {
    pub(super) fn node(&self) -> Option<&Node> {
        match self {
            Self::Node(node) => Some(node.as_ref()),
            Self::EndProcedure(_) => None,
        }
    }
}

pub(super) fn next_node(execution: &mut Execution) -> Result<Option<Node>> {
    loop {
        match execution.todo.pop_front() {
            Some(Queued::Node(node)) => return Ok(Some(*node)),
            Some(Queued::EndProcedure(player)) => execution.end_procedure(player)?,
            None => return Ok(None),
        }
    }
}
