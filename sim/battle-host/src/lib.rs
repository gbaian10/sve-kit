//! Local battle host shared by the browser Worker (WASM) and native tests.
//!
//! One engine authority, one ordered channel per player seat, policy seats answered
//! inside the host. Message payloads in [`wire`] are temporary until the generated
//! battle contract replaces them; the routing and rejection rules are final.

pub mod session;
pub mod wire;

pub use session::{Delivery, LocalSession, SeatControl};
