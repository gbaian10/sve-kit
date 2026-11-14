//! Provisional presentation and short-lived notices.
//!
//! Neither is rules history: provisional events are shown before their batch commits,
//! and notices carry what a seat was authorized to look at, not what happened.

use crate::common::{
    CardRef, NoticeId, PresentationBatchId, PresentationId, ScopeId, Seat, Subject, TextRef,
};
use crate::event::EventBody;
use crate::number::Nat;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One event shown before it commits; it has no committed identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresentationEvent {
    /// Identity that the committed event will refer to.
    pub id: PresentationId,
    /// What happened; unchanged when it commits.
    pub body: EventBody,
    /// Authorized subjects for generic highlighting.
    pub about: Vec<Subject>,
}

/// One complete, sealed batch shown early.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresentationBatch {
    /// Identity of the batch.
    pub id: PresentationBatchId,
    /// Its events, in the order they will commit.
    pub events: Vec<PresentationEvent>,
}

/// Why a provisional scope was withdrawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CancelReason {
    /// An internal fault rolled the shown result back; a rules cancellation never does.
    EngineFault,
}

/// One step in the life of a provisional scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PresentationOp {
    /// Show one complete batch now; a scope never gains members later.
    Provisional {
        /// The scope it opens.
        scope_id: ScopeId,
        /// The batch.
        batch: PresentationBatch,
    },
    /// The scope's batch committed in this update; it is not played again.
    Promote {
        /// The scope.
        scope_id: ScopeId,
    },
    /// The scope was discarded and must be removed from the picture.
    Cancel {
        /// The scope.
        scope_id: ScopeId,
        /// Why.
        reason: CancelReason,
    },
}

/// What kind of look a notice records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NoticeKind {
    /// This seat looked at cards.
    Look,
    /// Cards were revealed for a moment.
    TemporaryReveal,
}

/// One card of a notice, as it was seen; it links to no current object.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShownCard {
    /// Face seen.
    pub card: CardRef,
    /// Its owner.
    pub owner: Seat,
    /// Position among the cards looked at, when the look was ordered.
    pub position: Option<Nat>,
}

/// Cards a seat was authorized to see for a moment; kept by the client until read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Notice {
    /// Identity; a notice is shown once.
    pub id: NoticeId,
    /// Kind of look.
    pub kind: NoticeKind,
    /// Text that granted the look, when authorized.
    pub source: Option<TextRef>,
    /// The cards seen.
    pub cards: Vec<ShownCard>,
}
