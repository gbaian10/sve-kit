//! Wire types shared by the battle host and its clients.
//!
//! These types are the data model of `docs/sim/battle-contract.md`; the JSON Schema,
//! TypeScript types and validator are generated from them. The crate holds no rules:
//! projecting an authority into these types belongs to the engine integration.

extern crate alloc;

mod answer;
mod codec;
mod common;
pub mod event;
mod message;
mod number;
pub mod observation;
pub mod presentation;
pub mod question;
mod schema;

pub use answer::{
    Answer, BucketChoice, ClientMessage, Command, GroupChoice, RoleChoice, Share, Submit,
};
pub use codec::{decode, encode};
pub use common::{
    BatchId, CardRef, CauseId, ContentRef, EventId, Format, NoticeId, ObjectHandle, OptionId,
    PresentationBatchId, PresentationId, QuestionId, RequestId, RuleRef, RuleSource, RuleStrength,
    ScopeId, Seat, SourceRef, Subject, TextRef, Zone,
};
pub use message::{
    BattleInput, BattleUpdate, HostError, HostErrorReason, HostMessage, Identity, NoAdvice, Ready,
    Receipt, ReceiptResult, Region, Rejection,
};
pub use number::{Int, MAX_SAFE_INTEGER, Nat};
pub use schema::schema;
