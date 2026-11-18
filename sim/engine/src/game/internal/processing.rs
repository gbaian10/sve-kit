//! Procedure boundaries stay in the runtime queue, separate from authored IR.

mod visibility;
pub(super) use visibility::{
    ProcessingDisclosure, capture_declarations, disclose_declared_source, disclose_played_source,
    disclose_source,
};
pub(in crate::game) use visibility::{observed_processing, public_number_declaration};
mod payment_observation;
mod procedures;

mod selections;
pub(super) use selections::{discard_selection_round, record_play_declaration};

mod targets;
pub use targets::TargetDeclaration;
pub(super) use targets::record_target_declaration;

mod exchanges;
pub use exchanges::{ExchangeMaterialPayment, ExchangePayment};

mod payments;
pub(super) use payments::record_cost_spec_payment;
pub use payments::{CostPaymentPurpose, CostSpecPayment, CostStepPayment};

use super::{Execution, Fact, Game, Result, invalid};
use crate::game::player::observation::PaymentPurpose;
use crate::ir::{
    ChoiceDomain, DecisionId, Int, ModeDecision, ModeId, Nat, Node, ObjectRef, PlayerId, ReceiptId,
    ResolvedDamageAllocation, RuntimeDamageRecipient, ScopeId,
};
use alloc::vec::Vec;

/// A typed question's producer-recorded phase, separate from its purpose and answer domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestionStage {
    /// A formal play is choosing its target parameters.
    Target,
    /// A player is allocating amounts through an actual distribution question.
    Allocate,
    /// An effect is offering its resolution payment.
    Pay,
    /// An effect is making a choice during resolution.
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

/// One completed public `Action::Declare` number, owned by its execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NumberDeclaration {
    /// Player who actually declared; this may differ from the effect controller.
    pub player: PlayerId,
    /// The accepted nonnegative value, without a wire-number conversion.
    pub value: Nat,
}

/// One successful PP debit owned by its execution, not a complete cost receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PpPayment {
    /// Actual payer, which may differ from the effect controller.
    pub player: PlayerId,
    /// Amount passed to the successful debit after price adjustments, including zero.
    pub amount: Int,
    /// The completed payment producer, when one has identified the group.
    pub purpose: Option<PaymentPurpose>,
}

/// Accepted formal play choices, distinct from payment and body declarations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayDeclaration {
    /// A player explicitly fixed X; implicit zero defaults are not answers.
    X {
        /// Actual answering player.
        player: PlayerId,
        /// Accepted nonnegative value.
        value: Nat,
    },
    /// Accepted modes, normalized to their printed order.
    Modes {
        /// Actual answering player.
        player: PlayerId,
        /// The particular printed mode group.
        decision: DecisionId,
        /// Unique selected modes in resolution order.
        modes: Vec<ModeId>,
    },
    /// Accepted formal targets that may include leaders, with their physical object generations.
    Recipients {
        /// Actual answering player.
        player: PlayerId,
        /// Admitted formal parameter.
        decision: DecisionId,
        /// Accepted members in answer order; object identities may be private.
        members: Vec<RuntimeDamageRecipient>,
    },
    /// An accepted chosen-hand cost, keyed by its actual cost instruction receipt.
    HandCostMaterials {
        /// Actual answering player.
        player: PlayerId,
        /// Cost instruction identity, not a play parameter decision.
        receipt: ReceiptId,
        /// Explicitly selected members, excluding any implicitly paid source.
        objects: Vec<ObjectRef>,
    },
    /// An accepted route of a play-cost exchange.
    ExchangeRoute {
        /// Actual answering player.
        player: PlayerId,
        /// Admitted route question.
        decision: DecisionId,
        /// The checked zero-based route.
        route: Nat,
    },
    /// Accepted materials of a play-cost exchange, before payment completes.
    ExchangeMaterials {
        /// Actual answering player.
        player: PlayerId,
        /// Admitted route material decision.
        decision: DecisionId,
        /// Selected physical generations.
        objects: Vec<ObjectRef>,
    },
    /// Accepted formal cost materials, before payment completes.
    Materials {
        /// Actual answering player.
        player: PlayerId,
        /// Admitted formal material parameter.
        decision: DecisionId,
        /// Selected physical generations; private materials still need viewer authorization.
        objects: Vec<ObjectRef>,
    },
    /// Explicitly accepted formal distribution, not actual delivered damage.
    Distribution {
        /// Actual answering player.
        player: PlayerId,
        /// Admitted formal parameter.
        decision: DecisionId,
        /// Resolved physical members and their assigned amounts, not question handles.
        allocations: Vec<ResolvedDamageAllocation>,
    },
    /// A play-time cost declaration, before any payment completes.
    OptionalCost {
        /// Actual answering player.
        player: PlayerId,
        /// Cost group or exchange definition; acceptance is not evidence of payment.
        /// Exchanges have separate `ExchangePayment` records, not corresponding `CostSpec` invocations.
        receipt: ReceiptId,
        /// Whether declining cancels the play.
        required: bool,
        /// Accepted or declined by the player.
        accepted: bool,
    },
}

pub(super) fn record_x(execution: &mut Execution, player: PlayerId, value: Nat) {
    execution
        .play_declarations
        .push(PlayDeclaration::X { player, value });
}

pub(super) fn record_pp_payment(execution: &mut Execution, player: PlayerId, amount: Int) {
    execution.pp_payments.push(PpPayment {
        player,
        amount,
        purpose: None,
    });
}

pub(super) fn record_modes(
    execution: &mut Execution,
    player: PlayerId,
    decision: &ModeDecision,
    selected: &[ModeId],
) -> Result<()> {
    let ChoiceDomain::Modes { modes, .. } = &decision.domain else {
        return Err(invalid("accepted mode declaration domain"));
    };
    // Answer order cannot change the printed order in which selected modes resolve.
    let modes = modes
        .iter()
        .copied()
        .filter(|mode| selected.contains(mode))
        .collect();
    execution.play_declarations.push(PlayDeclaration::Modes {
        player,
        decision: decision.id,
        modes,
    });
    Ok(())
}

pub(super) fn record_number_declaration(
    game: &mut Game,
    execution: &mut Execution,
    player: PlayerId,
    value: Nat,
) {
    let declaration = NumberDeclaration { player, value };
    execution.number_declarations.push(declaration);
    execution.record_fact(
        game,
        Fact::Declared {
            player: declaration.player,
            value: declaration.value,
        },
    );
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
    ProcedureRound((ScopeId, u64), u64),
    EndRepeat(ScopeId),
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
            Self::EndProcedure(_) | Self::ProcedureRound(..) | Self::EndRepeat(_) => None,
        }
    }
}

pub(super) fn next_node(execution: &mut Execution) -> Result<Option<Node>> {
    loop {
        match execution.todo.pop_front() {
            Some(Queued::Node(node)) => return Ok(Some(*node)),
            Some(Queued::EndProcedure(player)) => execution.end_procedure(player)?,
            Some(Queued::ProcedureRound(key, round)) => execution
                .processing_disclosure
                .procedures
                .round(key, round)?,
            Some(Queued::EndRepeat(scope)) => {
                execution.processing_disclosure.procedures.end((scope, 0))?;
            }
            None => return Ok(None),
        }
    }
}
