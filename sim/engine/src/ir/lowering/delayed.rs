use super::{BodyContext, LoweringError, ast, unsupported};
use crate::ir::delayed as delayed_ir;
use crate::ir::{AbilitySource, SourceSpan};
use crate::ir::{
    Action, Binding, Capture, CaptureRef, DelayedLifetime, EventPattern, EventSnapshotPolicy,
    Selector, TriggerAggregation, TriggerCondition, TriggerSpec,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Delayed bodies share the enclosing lowering allocator."
)]
impl BodyContext<'_> {
    pub(super) fn delayed(
        &mut self,
        value: &ast::Delayed,
        path: &str,
        source: &AbilitySource,
        span: &SourceSpan,
    ) -> Result<Action, LoweringError> {
        if value.next_only || !matches!(value.until, ast::Until::Turn) {
            return Err(unsupported(path));
        }
        let parent = self.scope;
        let child = self.card_allocator.scope(path)?;
        let mut captures = vec![];
        let movement = &value.on.moved;
        let ast::FieldZone::Field = movement.from;
        let ast::CemeteryZone::Cemetery = movement.to;
        if !self.follower_targets.contains(&movement.r#ref)
            || self.hand_targets.contains(&movement.r#ref)
            || self.ex_targets.contains(&movement.r#ref)
            || self.deck_return_targets.contains(&movement.r#ref)
        {
            return Err(unsupported(path));
        }
        let (at, original) = self.selected_objects(&movement.r#ref, path)?;
        let into = CaptureRef {
            binding: Binding::new(self.allocator.id(path)?, child),
        };
        captures.push(Capture::Objects {
            at,
            value: Selector::Captured {
                reference: original,
            },
            into: into.clone(),
        });
        let binding = Binding::new(self.allocator.id(path)?, child);
        let pattern = EventPattern::LeaveField {
            binding,
            predicate: delayed_ir::cemetery(binding, into),
        };
        self.scope = child;
        let body = self.steps(&value.steps, &format!("{path}/delayed/do"), span, source);
        self.scope = parent;
        let action = Action::Delay {
            trigger: TriggerSpec {
                active_zones: vec![],
                on: TriggerCondition::Event { pattern },
                view: EventSnapshotPolicy::BeforeExit,
                aggregation: TriggerAggregation::PerFact {},
                limit: None,
            },
            body: body?,
            captures,
            lifetime: DelayedLifetime::Until {
                duration: Box::new(super::numeric::duration(Some(&value.until), path)?),
            },
            source: source.clone(),
        };
        if !delayed_ir::valid(&action) {
            return Err(unsupported(path));
        }
        Ok(action)
    }
}
