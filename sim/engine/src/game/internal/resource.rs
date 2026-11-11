//! Body resource increases record actual changes independently of payment completion.

#![expect(
    clippy::indexing_slicing,
    reason = "The PP object fields are validated before mutation."
)]

use super::{
    Execution, Game, InstructionOutcome, Result, invalid, number_plan::AmountPlan, seat_name,
    unsupported,
};
use crate::EngineFailure;
use crate::ir::{Action, Instruction, Int, PlayerId, PlayerRef, Resource, ResourceOperation};
use serde_json::json;

/// Authority-only result of one body resource instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceOutcome {
    /// Player whose resource changed.
    pub player: PlayerId,
    /// Explicitly requested resource.
    pub resource: Resource,
    /// Requested nonnegative increase, distinct from the actual delta.
    pub requested: Int,
    /// Value before the increase.
    pub before: Int,
    /// Value after checked arithmetic and any PP cap apply.
    pub after: Int,
    /// Actual increase, including zero when a PP resource is already full.
    pub actual: Int,
}

pub(super) fn check_action(action: &Action) -> Result<()> {
    let Action::Resource {
        player: PlayerRef::Controller {},
        resource: Resource::PpCurrent | Resource::PpMaximum | Resource::Ep,
        operation: ResourceOperation::Add,
        amount,
    } = action
    else {
        return if matches!(action, Action::Resource { .. }) {
            Err(unsupported("nonnegative own resource increase"))
        } else {
            Ok(())
        };
    };
    if !AmountPlan::compile(amount)?.proven_nonnegative() {
        return Err(unsupported(
            "resource increase cannot be proved nonnegative",
        ));
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
            resource, amount, ..
        } = &instruction.action
        else {
            return Err(unsupported("resource recovery amount"));
        };
        let resource = *resource;
        let requested = execution.integer(self, amount)?;
        if requested.0 < 0 {
            return Err(EngineFailure::Invariant(
                "proved resource increase became negative".into(),
            ));
        }
        if resource == Resource::Ep {
            return self.internal_ep_increase(execution, instruction, requested);
        }
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

    fn internal_ep_increase(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        requested: Int,
    ) -> Result<()> {
        let player = execution.context.controller;
        let seat = seat_name(player)?;
        let before = self.player(seat)?.ep;
        if before < 0 {
            return Err(invalid("negative EP value"));
        }
        let after = before.checked_add(requested.0).ok_or_else(|| {
            EngineFailure::Invariant("EP increase exceeds the integer representation".into())
        })?;
        self.player_mut(seat)?.ep = after;
        if requested.0 > 0 {
            let group = self.group();
            self.emit(
                json!({"kind":"EP変化","player":seat,"source":execution.frame().source,
                    "delta":requested.0,"before":before,"after":after}),
                &execution.frame().cause,
                group,
            );
        }
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Resource(ResourceOutcome {
                player,
                resource: Resource::Ep,
                requested,
                before: Int(before),
                after: Int(after),
                actual: requested,
            }),
        );
        Ok(())
    }
}
