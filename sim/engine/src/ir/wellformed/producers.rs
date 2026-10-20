use super::{BindingKind, Checked, Checker, Expr, WellFormedReason};
use crate::ir::{
    BatchPolicy, DamageAmount, DamageFlags, DamageRecipient, ObjectView, RecipientSet,
    TokenDestination, TokenInitialState, TokenSpec, ZoneKind, ZoneOperation,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Producer validation shares the closed program environment."
)]
impl Checker {
    pub(super) fn recipients(&mut self, recipients: &RecipientSet) -> Checked {
        match recipients {
            RecipientSet::Objects { objects } => self.selector(objects),
            RecipientSet::Leaders { players } => self.players(players),
            RecipientSet::Union { sets } => {
                for set in sets {
                    self.recipients(set)?;
                }
                Ok(())
            }
            RecipientSet::Explicit { recipients } => {
                for recipient in recipients {
                    match recipient {
                        DamageRecipient::Object { object } => self.object(object)?,
                        DamageRecipient::Leader { player } => self.player(player)?,
                        DamageRecipient::Bound { .. } | DamageRecipient::FromExpression { .. } => {
                            return self.reject();
                        }
                    }
                }
                Ok(())
            }
            RecipientSet::Bound { recipients } => self.reference(*recipients, BindingKind::Value),
        }
    }

    pub(super) fn damage(
        &mut self,
        recipients: &RecipientSet,
        amount: &DamageAmount,
        flags: DamageFlags,
        source: &ObjectView,
        batch: BatchPolicy,
    ) -> Checked {
        if flags != DamageFlags::Ability || batch != (BatchPolicy::Instruction {}) {
            return self.reject();
        }
        self.view(source)?;
        match amount {
            DamageAmount::Uniform { amount } => {
                self.recipients(recipients)?;
                self.integer(amount)
            }
            DamageAmount::Allocation { binding } => {
                self.reference(*binding, BindingKind::Value)?;
                if self.env.distributions.get(&binding.key()) != Some(recipients) {
                    return Err(self.error(WellFormedReason::DistributionRecipients));
                }
                // The decision already checked this domain. Runtime reuses its frozen membership.
                Ok(())
            }
        }
    }

    pub(super) fn tokens(
        &mut self,
        tokens: &[TokenSpec],
        destination: &TokenDestination,
        initial: Option<&TokenInitialState>,
    ) -> Checked {
        if tokens.is_empty() {
            return self.reject();
        }
        for (index, token) in tokens.iter().enumerate() {
            self.at(&format!("tokens/{index}"), |checker| {
                if !matches!(token.name, Expr::Literal(_)) {
                    return checker.reject();
                }
                checker.natural(&token.count)
            })?;
        }
        let TokenDestination::Single { zone } = destination else {
            return self.reject();
        };
        match zone {
            Expr::Literal(zone) if matches!(zone.kind, ZoneKind::Field | ZoneKind::Ex) => {
                self.player(&zone.player)?;
            }
            Expr::Compute(operation) => {
                let ZoneOperation::OfPlayer { player, kind } = operation.as_ref();
                if !matches!(kind, ZoneKind::Field | ZoneKind::Ex) {
                    return self.reject();
                }
                self.player(player)?;
            }
            Expr::Literal(_)
            | Expr::Frozen(_)
            | Expr::Live(_)
            | Expr::Captured(_)
            | Expr::Bound(_) => return self.reject(),
        }
        if let Some(TokenInitialState::StackCounters { amount }) = initial {
            self.natural(amount)?;
        }
        Ok(())
    }
}
