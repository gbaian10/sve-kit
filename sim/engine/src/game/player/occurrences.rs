//! Printed occurrence choices with links only to still-identifiable holders.

use super::observation::{EffectSource, ObservedZone};
use super::{ObjectHandle, Viewer};

/// One effective ability instance, retaining its printed definition coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AbilityChoice {
    /// Original question coordinate.
    pub handle: u64,
    /// Printing supplying the definition, including granted text.
    pub source_card: String,
    /// One-based printed source line.
    pub source_line: u32,
    /// Related printed section when applicable.
    pub source_section: Option<String>,
    /// Current Observation capability of the effective holder.
    pub holder: Option<ObjectHandle>,
}

/// A rule-owned Quick action; Pass remains a separate None answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickChoice {
    /// Original question coordinate.
    pub handle: u64,
    /// Printing authorized to its controller.
    pub card: String,
    /// Printed activation line, absent for card play.
    pub line: Option<u64>,
    /// Authorized source zone.
    pub zone: ObservedZone,
    /// Original one-based private source position, scoped to the question.
    pub position: u64,
    /// Current Observation capability, never a physical identity.
    pub object: Option<ObjectHandle>,
}

/// One pending occurrence, preserving its captured public text context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingChoice {
    /// Original question coordinate; historical quota coordinates are not answerable.
    pub handle: u64,
    /// Authorized printing disclosed by the occurrence.
    pub card: String,
    /// Printed ability line.
    pub line: u64,
    /// Existing public event description.
    pub event: String,
    /// Captured text source and a holder link only during its original identification period.
    pub source: Option<EffectSource>,
}

/// An earlier, already completed selection in this simultaneous batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EarlierQuotaChoice {
    /// Selecting controller.
    pub actor: Viewer,
    /// Public historical context; no physical association is inferred.
    pub occurrence: PendingChoice,
}

/// Simultaneous quota admission, separate from later pending resolution order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuotaQuestion {
    /// Only these current coordinates may be answered.
    pub candidates: Vec<PendingChoice>,
    /// Earlier public choices in the native turn-player-first order.
    pub earlier: Vec<EarlierQuotaChoice>,
}

/// One replacement occurrence in the complete ordering answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplacementChoice {
    /// Original question coordinate.
    pub handle: u64,
    /// Authorized provider printing.
    pub card: String,
    /// Authorized provider display name.
    pub name: String,
    /// Printed line when present.
    pub line: Option<u64>,
    /// Current Observation capability of the captured provider.
    pub object: Option<ObjectHandle>,
}
