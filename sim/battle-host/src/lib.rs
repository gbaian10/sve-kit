//! Local battle host shared by the browser Worker (WASM) and native tests.
//!
//! One engine authority, one ordered channel per player seat, policy seats answered
//! inside the host. Message payloads in [`wire`] are temporary until the generated
//! battle contract replaces them; the routing and rejection rules are final.

#![expect(
    clippy::multiple_crate_versions,
    reason = "The engine's dependencies already pull two syn and getrandom generations."
)]

pub mod load;
pub mod session;
#[cfg(target_arch = "wasm32")]
pub mod wasm;
pub mod wire;

pub use session::{Delivery, LocalSession, SeatControl};
