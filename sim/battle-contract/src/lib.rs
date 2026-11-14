//! Wire types shared by the battle host and its clients.
//!
//! These types are the data model of `docs/sim/battle-contract.md`; the JSON Schema,
//! TypeScript types and validator are generated from them. The crate holds no rules:
//! projecting an authority into these types belongs to the engine integration.

extern crate alloc;

mod codec;
mod common;
mod message;
mod number;
pub mod observation;
mod schema;

pub use codec::{decode, encode};
pub use common::{
    CardRef, ContentRef, Format, ObjectHandle, OptionId, QuestionId, RequestId, RuleRef,
    RuleSource, RuleStrength, Seat, SourceRef, Subject, TextRef, Zone,
};
pub use message::{
    HostError, HostErrorReason, HostMessage, Identity, Ready, Receipt, ReceiptResult, Region,
    Rejection,
};
pub use number::{Int, MAX_SAFE_INTEGER, Nat};
pub use schema::schema;
