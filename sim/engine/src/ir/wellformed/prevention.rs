use super::{Checked, Checker, WellFormedReason};
use crate::ir::prevention::{Recipients, projection};
use crate::ir::{
    Bool, DurationEnd, Effect, EventRewrite, Expr, ProposedEventPattern, Selector, TimeAnchor,
    ZoneKind,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Prevention uses the existing structural environment."
)]
impl Checker {
    pub(super) fn prevention(&mut self, effect: &Effect, zones: &[ZoneKind]) -> Checked {
        let shape = projection(effect).ok_or_else(|| self.error(WellFormedReason::InvalidEntry))?;
        if !matches!(zones, [ZoneKind::Field | ZoneKind::Equipment])
            || shape.duration.end != (DurationEnd::NoTemporalExpiry {})
            || shape.duration.anchor != (TimeAnchor::Installation {})
            || shape.duration.usage.is_some()
            || !shape.duration.intervals.is_empty()
        {
            return self.reject();
        }
        self.prevention_shape(effect)
    }

    fn prevention_shape(&mut self, effect: &Effect) -> Checked {
        let shape = projection(effect).ok_or_else(|| self.error(WellFormedReason::InvalidEntry))?;
        let Effect::Replacement {
            id,
            pattern:
                ProposedEventPattern::Damage {
                    recipient, source, ..
                },
            ..
        } = effect
        else {
            return self.reject();
        };
        if !self.effect_ids.insert(*id) {
            return self.reject();
        }
        // Event predicate binders cannot escape into ordinary object selectors.
        for binding in recipient
            .iter()
            .map(|filter| filter.binding.key())
            .chain(source.iter().map(|filter| filter.binding.key()))
        {
            if binding.1 == 0 || binding.0 == self.env.scope || !self.scopes.insert(binding.0) {
                return Err(self.error(WellFormedReason::DuplicateScope));
            }
        }
        self.ability_source(shape.source)?;
        match shape.recipients {
            Some(Recipients::Objects(objects)) => self.selector(objects)?,
            Some(Recipients::Leaders(players)) => self.players(players)?,
            None => {}
        }
        if let Some(objects) = shape.damage_source {
            self.selector(objects)?;
        }
        match shape.rewrite {
            EventRewrite::AddDamage { amount } => self.integer(amount)?,
            EventRewrite::CapDamage {
                threshold,
                replacement,
            } => {
                self.natural(threshold)?;
                self.natural(replacement)?;
            }
            EventRewrite::PreventDamage { .. } => {}
            EventRewrite::RedirectMove { .. }
            | EventRewrite::MultiplyDamage { .. }
            | EventRewrite::EntryPosture { .. }
            | EventRewrite::EvolutionCost { .. }
            | EventRewrite::ReplaceInstruction { .. } => return self.reject(),
        }
        self.boolean(shape.condition)
    }

    pub(super) fn installed_prevention(&mut self, effect: &Effect, targets: &Selector) -> Checked {
        let shape = projection(effect).ok_or_else(|| self.error(WellFormedReason::InvalidEntry))?;
        let captured = match (shape.recipients, shape.damage_source) {
            (Some(Recipients::Objects(objects)), None) | (None, Some(objects)) => {
                objects == targets
            }
            _ => false,
        };
        if !captured || *shape.condition != Expr::Literal(Bool(true)) {
            return self.reject();
        }
        self.prevention_shape(effect)
    }
}
