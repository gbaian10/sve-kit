//! Body PP recovery records actual changes independently of payment completion.

#![expect(
    clippy::indexing_slicing,
    reason = "The PP object fields are validated before mutation."
)]

use super::{Execution, Game, InstructionOutcome, Result, invalid, seat_name, unsupported};
use crate::ir::{Action, Expr, Instruction, Int, PlayerId, PlayerRef, Resource, ResourceOperation};
use serde_json::json;

/// Authority-only result of one body resource instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceOutcome {
    /// Player whose resource changed.
    pub player: PlayerId,
    /// Explicitly requested resource.
    pub resource: Resource,
    /// Requested positive recovery, distinct from the actual delta.
    pub requested: Int,
    /// Value before recovery.
    pub before: Int,
    /// Value after the maximum PP cap applies.
    pub after: Int,
    /// Actual increase, including zero when already full.
    pub actual: Int,
}

pub(super) fn check_action(action: &Action) -> Result<()> {
    if let Action::Resource { .. } = action
        && !matches!(action, Action::Resource {
            player: PlayerRef::Controller {},
            resource: Resource::PpCurrent,
            operation: ResourceOperation::Add,
            amount: Expr::Literal(Int(amount)),
        } if *amount > 0)
    {
        return Err(unsupported("fixed positive own PP recovery"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Body recovery mutates the authoritative player resource."
)]
impl Game {
    pub(super) fn internal_resource(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check_action(&instruction.action)?;
        let Action::Resource {
            amount: Expr::Literal(requested),
            ..
        } = instruction.action
        else {
            return Err(unsupported("resource recovery amount"));
        };
        let player = execution.context.controller;
        let seat = seat_name(player)?;
        let pp = &self.player(seat)?.pp;
        let read = |key| {
            pp.get(key)
                .and_then(serde_json::Value::as_i64)
                .ok_or_else(|| invalid("invalid PP value"))
        };
        let before = read("current")?;
        let maximum = read("max")?;
        if !(0..=10).contains(&maximum) || !(0..=maximum).contains(&before) {
            return Err(invalid("invalid PP bounds"));
        }
        let actual = requested.0.min(maximum.saturating_sub(before));
        let after = before
            .checked_add(actual)
            .ok_or_else(|| invalid("PP recovery overflow"))?;
        self.player_mut(seat)?.pp["current"] = json!(after);
        if actual > 0 {
            let group = self.group();
            self.emit(
                json!({"kind":"回復","player":seat,"amount":actual}),
                &execution.frame().cause,
                group,
            );
        }
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Resource(ResourceOutcome {
                player,
                resource: Resource::PpCurrent,
                requested,
                before: Int(before),
                after: Int(after),
                actual: Int(actual),
            }),
        );
        Ok(())
    }
}
