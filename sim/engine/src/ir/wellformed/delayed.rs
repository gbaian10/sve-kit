use super::{BindingKind, Checked, Checker, WellFormedReason};
use crate::ir::EventPattern;
use crate::ir::delayed as delayed_ir;
use crate::ir::events::subject::SubjectFields;
use crate::ir::{Action, TriggerCondition, Visibility};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Delayed callbacks isolate the shared checker environment."
)]
impl Checker {
    pub(super) fn delay(&mut self, action: &Action) -> Checked {
        let Action::Delay {
            trigger,
            body,
            captures,
            source,
            ..
        } = action
        else {
            return self.reject();
        };
        if !delayed_ir::valid(action) || self.ability_source.as_ref() != Some(source) {
            return self.reject();
        }
        let parent = self.env.clone();
        let root = self.root;
        let entry = self.entry;
        self.env = Self::new(body.scope).env;
        for capture in captures {
            let (at, from, into) = delayed_ir::transfer(capture)
                .ok_or_else(|| self.error(WellFormedReason::CaptureTiming))?;
            self.env = parent.clone();
            self.reference(from.binding, BindingKind::Capture)?;
            if parent.capture_times.get(&from.binding.key()) != Some(&at)
                || parent.access.get(&from.binding.key()) != Some(&Visibility::Public {})
            {
                return self.reject();
            }
            self.env = Self::new(body.scope).env;
            self.define(into.binding, BindingKind::Capture)?;
            self.env.capture_times.insert(into.binding.key(), at);
            self.env
                .access
                .insert(into.binding.key(), Visibility::Public {});
        }
        self.env.trigger = Some(trigger.view);
        self.root = body.scope;
        self.entry = false;
        let TriggerCondition::Event { pattern } = &trigger.on else {
            return self.reject();
        };
        if let EventPattern::LeaveField { binding, .. } = pattern {
            self.env.trigger_subject = Some(SubjectFields::Cemetery(*binding).object());
        }
        self.event_pattern(pattern, Some(trigger.view), true)?;
        let result = self.program(body);
        self.env = parent;
        self.root = root;
        self.entry = entry;
        result
    }
}
