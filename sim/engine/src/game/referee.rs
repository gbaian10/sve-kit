//! Trusted-runner authority reads. Players never receive these; the referee uses them to
//! resolve fixture labels, read diagnostics and compare raw rules events.

use super::Game;
use super::player::{ObjectHandle, Viewer};
use crate::EngineFailure;
use core::mem::take;
use serde_json::{Value, json};

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

    pub(crate) fn referee_replay_digest(&self) -> crate::Result<String> {
        self.normalized_state_digest()
    }

    pub(super) fn referee_trace_event(&mut self, mut event: Value) {
        if let Some(execution) = event
            .pointer("/cause/effect/execution")
            .and_then(Value::as_u64)
            && let Some((node, ability)) = self.replay.executions.get(&execution)
            && let Some(fields) = event.as_object_mut()
        {
            fields.insert("cause".into(), json!({"decision": node}));
            if let Some(ability) = ability {
                fields.entry("ability").or_insert_with(|| ability.clone());
            }
        }
        if let Some(log) = &mut self.referee_events {
            log.push(event);
        }
    }

    pub(in crate::game) fn referee_trace_execution(
        &mut self,
        execution: u64,
        ability: Option<Value>,
    ) {
        if let Some(node) = &self.replay.node {
            self.replay
                .executions
                .insert(execution, (node.clone(), ability));
        }
    }

    pub(in crate::game) fn referee_trace_pending(
        &mut self,
        pending: u64,
        source: &str,
        line: u64,
        cause: &Value,
    ) {
        if let Some(node) = &self.replay.node
            && let Some(log) = &mut self.referee_events
        {
            log.push(json!({"id":format!("{node}:p{pending}"),"kind":"待機",
                "ability":{"source":source,"line":line}, "cause":cause}));
        }
    }

    /// Events emitted since the previous call, in emission order.
    pub(crate) fn referee_take_events(&mut self) -> Vec<Value> {
        self.referee_events.as_mut().map(take).unwrap_or_default()
    }
}
