use super::{Checked, Checker, WellFormedReason};
use crate::ir::trigger_multiplicity::projection;
use crate::ir::{Effect, ZoneKind};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Rule declaration validation shares the structural scope environment."
)]
impl Checker {
    pub(super) fn trigger_multiplicity(&mut self, effect: &Effect, zones: &[ZoneKind]) -> Checked {
        let shape = projection(effect).ok_or_else(|| self.error(WellFormedReason::InvalidEntry))?;
        if zones != [ZoneKind::Field] {
            return self.reject();
        }
        let (scope, id) = shape.binding.key();
        if id == 0 || scope == self.env.scope || !self.scopes.insert(scope) {
            return Err(self.error(WellFormedReason::DuplicateScope));
        }
        self.ability_source(shape.source)?;
        self.player(shape.controller)?;
        self.boolean(shape.condition)
    }
}
