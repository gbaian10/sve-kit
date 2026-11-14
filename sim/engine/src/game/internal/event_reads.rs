//! Resolving occurrences read only the payload accepted by their own event header.

use super::source::{EventObject, SourceSnapshot};
use super::{Result, invalid, unsupported};
use crate::ir::events::banish as banish_header;
use crate::ir::events::discard as discard_header;
use crate::ir::events::drive as drive_header;
use crate::ir::events::equipment as equipment_event;
use crate::ir::events::subject::SubjectFields;
use crate::ir::{
    AttackEvent, Binding, DamageEvent, DiceEvent, EnterFieldEvent, EntryMethod, EntryOrigin,
    EventPattern, EventPayload, LeaveFieldEvent, MoveEvent, MovementEvent, ObjectRef,
    TriggerCondition, TriggerSpec,
};

#[derive(Clone, Debug, Default)]
pub(super) struct EventReads(Context);

#[derive(Clone, Debug, Default)]
enum Context {
    #[default]
    Empty,
    Subject(EventObject),
    Entry {
        subject: EventObject,
        binding: Binding<EnterFieldEvent>,
        method: EntryMethod,
        origin: EntryOrigin,
    },
    Attack {
        subject: EventObject,
        binding: Binding<AttackEvent>,
        event: AttackEvent,
    },
    Movement {
        subject: Option<EventObject>,
        event: MovementEvent,
        after: Option<ObjectRef>,
    },
    Damage(Binding<DamageEvent>, DamageEvent),
    Dice(Binding<DiceEvent>, DiceEvent),
    Move(Binding<MoveEvent>, MoveEvent),
}

impl Context {
    fn capture(
        trigger: Option<&TriggerSpec>,
        timing: Option<&EventPayload>,
        leave: Option<LeaveFieldEvent>,
        after: Option<ObjectRef>,
        information: Option<&SourceSnapshot>,
    ) -> Result<Self> {
        let Some(trigger) = trigger else {
            return Ok(Self::Empty);
        };
        let movement = match &trigger.on {
            TriggerCondition::Event {
                pattern: EventPattern::LeaveField { binding, .. },
            } => {
                if after.is_none() {
                    return Err(invalid("missing committed departure"));
                }
                Some(MovementEvent::LeaveField { event: *binding })
            }
            _ => discard_header::self_event(trigger)
                .map(|event| MovementEvent::Discard { event })
                .or_else(|| {
                    banish_header::self_event(trigger).map(|event| MovementEvent::Banish { event })
                }),
        };
        let TriggerCondition::Event { pattern } = &trigger.on else {
            return Ok(Self::Empty);
        };
        let subject = subject(pattern, timing, leave, information)?;
        if let Some(event) = movement {
            return Ok(Self::Movement {
                subject,
                event,
                after,
            });
        }
        if let Some(subject) = subject {
            return Ok(match (pattern, timing) {
                (EventPattern::Attack { binding, .. }, Some(EventPayload::Attack { event })) => {
                    Self::Attack {
                        subject,
                        binding: *binding,
                        event: *event,
                    }
                }
                (
                    EventPattern::EnterField { binding, .. },
                    Some(EventPayload::EnterField { event }),
                ) => Self::Entry {
                    subject,
                    binding: *binding,
                    method: event.method,
                    origin: event.from,
                },
                _ => Self::Subject(subject),
            });
        }
        Ok(match (pattern, timing) {
            (EventPattern::Damage { binding, .. }, Some(EventPayload::Damage { event })) => {
                Self::Damage(*binding, *event)
            }
            (EventPattern::Dice { binding, .. }, Some(EventPayload::Dice { event })) => {
                Self::Dice(*binding, event.clone())
            }
            (EventPattern::Move { binding, .. }, Some(EventPayload::Move { event })) => {
                Self::Move(*binding, *event)
            }
            _ => Self::Empty,
        })
    }
}

impl EventReads {
    pub(super) fn capture(
        trigger: Option<&TriggerSpec>,
        timing: Option<&EventPayload>,
        leave: Option<LeaveFieldEvent>,
        after: Option<ObjectRef>,
        information: Option<&SourceSnapshot>,
    ) -> Result<Self> {
        Context::capture(trigger, timing, leave, after, information).map(Self)
    }

    pub(super) const fn object(&self) -> Option<&EventObject> {
        match &self.0 {
            Context::Subject(subject)
            | Context::Entry { subject, .. }
            | Context::Attack { subject, .. } => Some(subject),
            Context::Movement { subject, .. } => subject.as_ref(),
            Context::Empty | Context::Damage(..) | Context::Dice(..) | Context::Move(..) => None,
        }
    }

    pub(super) fn move_batch_count(&self, binding: Binding<MoveEvent>) -> Option<u64> {
        match &self.0 {
            Context::Move(saved, event) if *saved == binding => Some(event.batch_size.0),
            _ => None,
        }
    }

    pub(super) fn attack(&self, binding: Binding<AttackEvent>) -> Option<AttackEvent> {
        match &self.0 {
            Context::Attack {
                binding: saved,
                event,
                ..
            } if *saved == binding => Some(*event),
            _ => None,
        }
    }

    pub(super) fn movement(&self, event: MovementEvent) -> Result<Option<ObjectRef>> {
        match &self.0 {
            Context::Movement {
                event: saved,
                after,
                ..
            } if *saved == event => Ok(*after),
            _ => Err(unsupported(
                if matches!(event, MovementEvent::LeaveField { .. }) {
                    "missing triggering movement authority"
                } else {
                    "missing triggering hand departure authority"
                },
            )),
        }
    }

    pub(super) fn damage(&self, binding: Binding<DamageEvent>) -> Option<DamageEvent> {
        match &self.0 {
            Context::Damage(saved, event) if *saved == binding => Some(*event),
            _ => None,
        }
    }

    pub(super) fn dice(&self, binding: Binding<DiceEvent>) -> Option<&DiceEvent> {
        match &self.0 {
            Context::Dice(saved, event) if *saved == binding => Some(event),
            _ => None,
        }
    }

    pub(super) fn entry_origin(&self, binding: Binding<EnterFieldEvent>) -> Option<EntryOrigin> {
        match &self.0 {
            Context::Entry {
                binding: saved,
                origin,
                ..
            } if *saved == binding => Some(*origin),
            _ => None,
        }
    }

    pub(super) fn entry_method(&self, binding: Binding<EnterFieldEvent>) -> Option<EntryMethod> {
        match &self.0 {
            Context::Entry {
                binding: saved,
                method,
                ..
            } if *saved == binding => Some(*method),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(super) fn for_attack(binding: Binding<AttackEvent>, event: AttackEvent) -> Self {
        Self(Context::Attack {
            subject: EventObject {
                expression: SubjectFields::Attack(binding).object(),
                reference: event.attacker,
                information: None,
            },
            binding,
            event,
        })
    }
}

fn subject(
    pattern: &EventPattern,
    timing: Option<&EventPayload>,
    leave: Option<LeaveFieldEvent>,
    information: Option<&SourceSnapshot>,
) -> Result<Option<EventObject>> {
    if let Some(binding) = equipment_event::binding(pattern) {
        let Some(EventPayload::Equip { event }) = timing else {
            return Err(invalid("missing triggering equipment payload"));
        };
        return Ok(Some(EventObject {
            expression: equipment_event::holder(binding),
            reference: event.holder,
            information: None,
        }));
    }
    if let Some(binding) = drive_header::checked_binding(pattern) {
        let Some(EventPayload::DriveChecked { event }) = timing else {
            return Err(invalid("missing completed Drive check"));
        };
        return Ok(Some(EventObject {
            expression: drive_header::holder(binding),
            reference: event.holder.holder,
            information: None,
        }));
    }
    let Some(fields) = SubjectFields::from_pattern(pattern) else {
        return Ok(None);
    };
    let departure = leave.map(|event| EventPayload::LeaveField { event });
    let payload = timing
        .or(departure.as_ref())
        .ok_or_else(|| invalid("missing triggering subject payload"))?;
    let reference = fields
        .payload_object(payload)
        .ok_or_else(|| invalid("triggering subject payload type disagrees"))?;
    Ok(Some(EventObject {
        expression: fields.object(),
        reference,
        information: information.cloned(),
    }))
}
