//! Body PP increases record actual changes independently of payment completion.

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
    /// Requested positive increase, distinct from the actual delta.
    pub requested: Int,
    /// Value before the increase.
    pub before: Int,
    /// Value after the resource cap applies.
    pub after: Int,
    /// Actual increase, including zero when already full.
    pub actual: Int,
}

pub(super) fn check_action(action: &Action) -> Result<()> {
    if let Action::Resource { .. } = action
        && !matches!(action, Action::Resource {
            player: PlayerRef::Controller {},
            resource: Resource::PpCurrent | Resource::PpMaximum,
            operation: ResourceOperation::Add,
            amount: Expr::Literal(Int(amount)),
        } if *amount > 0)
    {
        return Err(unsupported("fixed positive own PP increase"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Body resource increases mutate the authoritative player resource."
)]
impl Game {
    pub(super) fn internal_resource(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check_action(&instruction.action)?;
        let Action::Resource {
            resource,
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
        let current = read("current")?;
        let maximum = read("max")?;
        if !(0..=10).contains(&maximum) || !(0..=maximum).contains(&current) {
            return Err(invalid("invalid PP bounds"));
        }
        let (key, before, cap) = if resource == Resource::PpMaximum {
            ("max", maximum, 10)
        } else {
            ("current", current, maximum)
        };
        let actual = requested.0.min(cap.saturating_sub(before));
        let after = before
            .checked_add(actual)
            .ok_or_else(|| invalid("PP recovery overflow"))?;
        self.player_mut(seat)?.pp[key] = json!(after);
        if actual > 0 {
            let group = self.group();
            let event = if resource == Resource::PpCurrent {
                json!({"kind":"回復","player":seat,"amount":actual})
            } else {
                json!({"kind":"PP最大値変化","player":seat,"source":execution.frame().source,
                    "delta":actual,"before":before,"after":after})
            };
            self.emit(event, &execution.frame().cause, group);
        }
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Resource(ResourceOutcome {
                player,
                resource,
                requested,
                before: Int(before),
                after: Int(after),
                actual: Int(actual),
            }),
        );
        Ok(())
    }
}
