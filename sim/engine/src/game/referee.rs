//! Trusted-runner authority reads. Players never receive these; the referee uses them to
//! resolve fixture labels, read diagnostics and compare raw rules events.

use super::Game;
use super::player::{ObjectHandle, Viewer};
use crate::EngineFailure;
use core::mem::take;
use serde_json::Value;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Referee reads share the game authority without widening the player boundary."
)]
impl Game {
    /// Fixture label of a viewer capability, while that viewer can still identify it.
    pub(crate) fn referee_label(&self, viewer: Viewer, handle: &ObjectHandle) -> Option<String> {
        self.resolve_player_object(viewer, handle)
            .ok()
            .map(str::to_owned)
    }

    /// Why player progression stopped; never part of a player answer.
    pub(crate) const fn referee_fault(&self) -> Option<&EngineFailure> {
        self.player_fault.as_ref()
    }

    /// Start keeping every emitted rules event for referee comparison.
    pub(crate) fn referee_record_events(&mut self) {
        self.referee_events.get_or_insert_with(Vec::new);
    }

    /// Events emitted since the previous call, in emission order.
    pub(crate) fn referee_take_events(&mut self) -> Vec<Value> {
        self.referee_events.as_mut().map(take).unwrap_or_default()
    }
}
