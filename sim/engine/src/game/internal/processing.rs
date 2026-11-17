//! Procedure boundaries stay in the runtime queue, separate from authored IR.

use super::{Execution, Result, invalid};
use crate::ir::{Node, ScopeId};

/// A typed question's producer-recorded phase, separate from its purpose and answer domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestionStage {
    /// An effect is offering its resolution payment.
    Pay,
    /// An effect is offering its optional resolution body.
    Resolve,
}

/// Producer-recorded lifecycle of a card or ability play, independent of its current question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayPhase {
    /// Declaring parameters before intrinsic payment.
    Declare,
    /// Intrinsic payment and entry checks before the play is committed.
    Pay,
    /// The play has committed and its body is resolving.
    Resolve,
}

pub(super) fn enter_play_phase(execution: &mut Execution, next: PlayPhase) -> Result<()> {
    let previous = match next {
        PlayPhase::Declare => None,
        PlayPhase::Pay => Some(PlayPhase::Declare),
        PlayPhase::Resolve => Some(PlayPhase::Pay),
    };
    if execution.play_phase != previous {
        return Err(invalid("formal play lifecycle transition"));
    }
    execution.play_phase = Some(next);
    Ok(())
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
