//! Surrender commits the executed prefix and releases every continuation without rollback.

use super::{ExecutionId, Game, PlayerId, Progress, Result, invalid, seat_name};
use crate::game::{Step, other};
use core::mem::take;
use serde_json::json;

/// Successful rules termination; an interrupted execution has no resolved fact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Terminal {
    /// Execution interrupted by termination, if a body or play question was active.
    pub execution: Option<ExecutionId>,
    /// Already performed instruction receipts and facts, with complete=false.
    pub progress: Option<Progress>,
    /// Committed events through the terminal rule result.
    pub step: Step,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Termination owns the same authority as prepared continuations."
)]
impl Game {
    pub(in crate::game) fn internal_terminal_cleanup(&mut self) {
        self.typed = None;
        self.prepared.clear();
        self.card_runtime.terminate();
        self.state.pending.clear();
        self.state.prompt = None;
        self.state.frame = None;
    }

    /// Surrender immediately, including while a play or pending-order question is waiting.
    ///
    /// # Errors
    /// Rejects an invalid player or an already ended game without changing authority.
    pub fn concede_internal(&mut self, actor: PlayerId) -> Result<Terminal> {
        let seat = seat_name(actor)?;
        if self.state.game["ended"] == true {
            return Err(invalid("game already ended"));
        }
        let epoch = self
            .state
            .ir_epoch
            .checked_add(1)
            .ok_or_else(|| invalid("IR epoch exhausted"))?;
        let interrupted = self.typed.take();
        let interrupted_id = interrupted.as_ref().map(|execution| execution.id);
        let progress = interrupted.map(|execution| {
            let mut progress = execution.progress();
            progress.complete = false;
            progress
        });
        self.internal_terminal_cleanup();
        self.state.ir_epoch = epoch;
        self.state.game = json!({"ended":true,"winner":other(seat),"by":"1.2.3"});
        let group = self.group();
        self.emit(
            json!({"kind":"敗北","player":seat,"by":"rule-1.2.3"}),
            &json!({"rule":"1.2.3"}),
            group,
        );
        Ok(Terminal {
            execution: interrupted_id,
            progress,
            step: Step {
                outcome: "game-end".into(),
                events: take(&mut self.emitted),
            },
        })
    }
}
