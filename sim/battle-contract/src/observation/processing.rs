//! What is being processed, as far as a seat is authorized to know.
//!
//! Hidden inner contexts leave no slot, depth or serial behind.

use super::costs::CostSummary;
use crate::common::{RuleRef, Seat, Subject, TextRef};
use crate::number::{Int, Nat};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Step of a procedure that a context or question belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingStage {
    /// Opening procedure.
    Opening,
    /// Declaring a play and its parameters.
    Declare,
    /// Choosing targets.
    Target,
    /// Allocating amounts.
    Allocate,
    /// Paying costs.
    Pay,
    /// Resolving an effect.
    Resolve,
    /// Admitting simultaneous triggers under a quota.
    TriggerAdmission,
    /// Ordering pending occurrences.
    PendingOrder,
    /// A Quick window.
    Quick,
    /// Turn-closing procedure.
    Closing,
}

/// How a procedure passes between seats or repeats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProcedureKind {
    /// Each player acts in turn.
    EachPlayer,
    /// One step repeats.
    Repeat,
}

/// Public position within a procedure that involves several seats or rounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Procedure {
    /// How the procedure advances.
    pub kind: ProcedureKind,
    /// Seat currently acting in it.
    pub actor: Seat,
    /// Public round number, when the procedure counts rounds.
    pub round: Option<Nat>,
}

/// One declared mode.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeclaredMode {
    /// Printed mode identifier.
    pub mode: Nat,
    /// Printed text of the mode, when it has an exact correspondence.
    pub text: Option<TextRef>,
}

/// Role of a declared number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeclaredNumber {
    /// The X of a cost or effect.
    X,
}

/// A declaration already public or known to this seat; never a payment receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Declared {
    /// Chosen modes in printed order.
    Modes {
        /// The modes.
        modes: Vec<DeclaredMode>,
    },
    /// A declared number.
    Number {
        /// What the number is.
        role: DeclaredNumber,
        /// Declared value.
        value: Int,
    },
    /// Declared targets.
    Targets {
        /// The targets.
        targets: Vec<Subject>,
    },
    /// Whether an optional cost was accepted.
    OptionalCost {
        /// The declaration.
        accepted: bool,
    },
    /// A declared card name.
    Name {
        /// Rules name, compared as the snapshot spells it.
        rules_name: String,
    },
}

/// One authorized processing context.
///
/// The seat answering a question need not be the controller.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProcessingContext {
    /// Step of the procedure.
    pub stage: ProcessingStage,
    /// Controller of what is processed, when it has one.
    pub controller: Option<Seat>,
    /// Authorized text being processed.
    pub source: Option<TextRef>,
    /// Authorized subject of the processing.
    pub subject: Option<Subject>,
    /// Rule the procedure rests on.
    pub rule: Option<RuleRef>,
    /// Position within a procedure of several seats or rounds.
    pub procedure: Option<Procedure>,
    /// Declarations made so far that this seat may know.
    pub declared: Vec<Declared>,
    /// Payment summary, when this seat may know it.
    pub payment: Option<CostSummary>,
}
