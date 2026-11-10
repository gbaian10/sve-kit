//! Closed turn-scoped subscriptions installed by a resolving card ability.
use super::events::subject::{SubjectFields, SubjectFilter};
use super::{
    Action, Binding, Bool, Capture, CaptureRef, CaptureTime, DelayedLifetime, Duration,
    DurationEnd, EventPattern, EventSnapshotPolicy, Expr, InstructionContext, Int, LeaveFieldEvent,
    Node, ObjectOperand, PlayerRef, Predicate, Selector, TimeAnchor, TriggerAggregation,
    TriggerCondition,
};

pub(crate) fn cemetery(
    event: Binding<LeaveFieldEvent>,
    target: CaptureRef<Selector>,
) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::And {
        predicates: vec![
            SubjectFilter::default().predicate(SubjectFields::Cemetery(event)),
            Expr::Compute(Box::new(Predicate::MemberOf {
                object: ObjectOperand::FromExpression {
                    value: Box::new(SubjectFields::Cemetery(event).object()),
                },
                set: Selector::Captured { reference: target },
            })),
        ],
    }))
}

pub(crate) fn transfer(
    capture: &Capture,
) -> Option<(CaptureTime, CaptureRef<Selector>, CaptureRef<Selector>)> {
    if let Capture::Objects {
        at: at @ CaptureTime::AtSelection { .. },
        value: Selector::Captured { reference },
        into,
    } = capture
    {
        Some((*at, reference.clone(), into.clone()))
    } else {
        None
    }
}

pub(crate) fn valid(action: &Action) -> bool {
    let Action::Delay {
        trigger,
        body,
        captures,
        lifetime,
        source,
    } = action
    else {
        return false;
    };
    let lifetime_ok = matches!(lifetime, DelayedLifetime::Until { duration }
        if **duration == Duration { end: DurationEnd::CurrentTurnEnd {}, anchor: TimeAnchor::Installation {}, usage: None, intervals: vec![] });
    if !lifetime_ok
        || !trigger.active_zones.is_empty()
        || trigger.limit.is_some()
        || trigger.aggregation != (TriggerAggregation::PerFact {})
        || body.steps.is_empty()
    {
        return false;
    }
    let on_ok = match (&trigger.on, captures.as_slice()) {
        (
            TriggerCondition::Event {
                pattern: EventPattern::LeaveField { binding, predicate },
            },
            [capture],
        ) => {
            transfer(capture).is_some_and(|(_, _, into)| {
                into.binding.key().0 == body.scope && *predicate == cemetery(*binding, into)
            }) && trigger.view == EventSnapshotPolicy::BeforeExit
        }
        _ => false,
    };
    on_ok
        && body.steps.iter().all(|step| {
            let Node::Instruction { instruction } = &step.node else {
                return false;
            };
            instruction.source == *source
                && instruction.context == InstructionContext::Effect
                && instruction.precondition.is_none()
                && instruction.actor == (PlayerRef::Controller {})
                && fixed_action(&instruction.action)
        })
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Only the required callback actions pass this closed boundary."
)]
fn fixed_action(action: &Action) -> bool {
    use super::{NumericChange, StepwiseMode, TokenDestination, TokenTemplate, ZoneKind, ZoneRef};
    match action {
        Action::Draw {
            player: PlayerRef::Controller {},
            count: Expr::Literal(Int(1)),
            mode: StepwiseMode::Exact,
        }
        | Action::Life {
            player: PlayerRef::Controller {},
            change:
                NumericChange::Add {
                    amount: Expr::Literal(Int(2)),
                },
        } => true,
        Action::Token {
            tokens,
            destination:
                TokenDestination::Single {
                    zone:
                        Expr::Literal(ZoneRef {
                            player: PlayerRef::Controller {},
                            kind: ZoneKind::Ex,
                        }),
                },
            initial_state: None,
            entry_effects,
            ..
        } => {
            entry_effects.is_empty()
                && matches!(tokens.as_slice(), [token] if matches!(token.template, TokenTemplate::Fixed { .. }) && token.count == Expr::Literal(super::Nat(1)))
        }
        _ => false,
    }
}
