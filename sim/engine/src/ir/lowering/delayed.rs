use super::{BodyContext, LoweringError, ast, unsupported};
use crate::ir::delayed as delayed_ir;
use crate::ir::events::subject::SubjectFields;
use crate::ir::{AbilitySource, SourceSpan};
use crate::ir::{
    Action, Binding, Capture, CaptureRef, DelayedLifetime, EntryMethod, EventPattern,
    EventSnapshotPolicy, Expr, Predicate, Selector, TriggerAggregation, TriggerCondition,
    TriggerSpec,
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
        if !matches!(value.until, ast::Until::Turn) {
            return Err(unsupported(path));
        }
        let action = match &value.on {
            ast::DelayedOn::Moved { moved } if !value.next_only => {
                self.delayed_departure(value, moved, path, source, span)?
            }
            ast::DelayedOn::Moved { .. } => return Err(unsupported(path)),
            ast::DelayedOn::EnterField { enter_field } => {
                self.delayed_entry(value, enter_field, path, source, span)?
            }
        };
        if !delayed_ir::valid(&action) {
            return Err(unsupported(path));
        }
        Ok(action)
    }

    fn delayed_departure(
        &mut self,
        value: &ast::Delayed,
        movement: &ast::DelayedMove,
        path: &str,
        source: &AbilitySource,
        span: &SourceSpan,
    ) -> Result<Action, LoweringError> {
        let parent = self.scope;
        let child = self.card_allocator.scope(path)?;
        let mut captures = vec![];
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
        Ok(Action::Delay {
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
        })
    }

    /// 「このターン、次に自分の場に…が出たとき、それは…」: `it` is the entering card.
    fn delayed_entry(
        &mut self,
        value: &ast::Delayed,
        entry: &ast::DelayedEntry,
        path: &str,
        source: &AbilitySource,
        span: &SourceSpan,
    ) -> Result<Action, LoweringError> {
        // An installed observer has no card of its own to compare names with.
        if entry
            .sel
            .as_ref()
            .is_some_and(|sel| sel.not_same_name_as_this.is_some())
        {
            return Err(unsupported(path));
        }
        let parent = self.scope;
        let child = self.card_allocator.scope(path)?;
        let event = Binding::new(self.allocator.id(path)?, child);
        let fields = SubjectFields::Entry(event);
        let mut predicate = super::movement::movement_filter(
            entry
                .who
                .as_ref()
                .map(|side| matches!(side, ast::EntryPlayer::Controller)),
            false,
            entry.sel.as_ref(),
            false,
            path,
            super::movement::FilterSupport::EntryCost,
        )?
        .predicate(fields);
        if let Some(by) = &entry.entered_by {
            let Expr::Compute(root) = &mut predicate else {
                return Err(unsupported(path));
            };
            let Predicate::And { predicates } = root.as_mut() else {
                return Err(unsupported(path));
            };
            predicates.push(delayed_ir::method(
                event,
                match by {
                    ast::EntryMethod::Ability => EntryMethod::AbilityPut,
                    ast::EntryMethod::Play => EntryMethod::Play,
                },
            ));
        }
        if delayed_ir::entry(event, &predicate).is_none() {
            return Err(unsupported(path));
        }
        let pattern = EventPattern::EnterField {
            binding: event,
            predicate,
        };
        let outer_subject = self.subject_event.replace(fields);
        self.scope = child;
        let body = self.steps(&value.steps, &format!("{path}/delayed/do"), span, source);
        self.scope = parent;
        self.subject_event = outer_subject;
        Ok(Action::Delay {
            trigger: TriggerSpec {
                active_zones: vec![],
                on: TriggerCondition::Event { pattern },
                view: EventSnapshotPolicy::AfterEntry,
                aggregation: TriggerAggregation::PerFact {},
                limit: value.next_only.then_some(delayed_ir::NEXT_ONLY),
            },
            body: body?,
            captures: vec![],
            lifetime: DelayedLifetime::Until {
                duration: Box::new(super::numeric::duration(Some(&value.until), path)?),
            },
            source: source.clone(),
        })
    }
}
