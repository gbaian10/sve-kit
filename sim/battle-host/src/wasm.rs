//! Browser Worker bindings. Every call is synchronous; the Worker queues commands so
//! exactly one runs at a time. Seat names and messages are validated by the session, so
//! a malformed call stops interaction instead of throwing past it.

use crate::load::{Inputs, load};
use crate::session::{Delivery, LocalSession};
use wasm_bindgen::prelude::{JsError, wasm_bindgen};

/// One local match inside the Worker.
#[wasm_bindgen]
#[derive(Debug)]
pub struct Session {
    inner: LocalSession,
}

#[wasm_bindgen]
impl Session {
    /// Create a match from downloaded inputs and a JSON match setup.
    ///
    /// # Errors
    /// Throws `format`, `selection` or `match`, never engine diagnostics.
    #[wasm_bindgen(constructor)]
    pub fn new(
        snapshot: &str,
        keywords: &str,
        selection: &str,
        rules: &str,
        setup: &str,
    ) -> Result<Self, JsError> {
        let inputs = Inputs {
            snapshot,
            keywords,
            selection,
            rules,
        };
        load(inputs, setup)
            .map(|inner| Self { inner })
            .map_err(|error| JsError::new(error.code()))
    }

    /// First baselines, as a JSON array of `{seat, message}`.
    pub fn start(&mut self) -> String {
        deliveries(&self.inner.start())
    }

    /// Process one JSON command from the seat named `seat`.
    pub fn submit(&mut self, seat: &str, message: &str) -> String {
        deliveries(&self.inner.submit_named(seat, message))
    }

    /// Resend the named seat's complete current state.
    pub fn baseline(&mut self, seat: &str) -> String {
        deliveries(&self.inner.baseline_named(seat))
    }
}

fn deliveries(out: &[Delivery]) -> String {
    // Every message is plain strings and integers, so serialization cannot fail.
    serde_json::to_string(out).unwrap_or_default()
}
