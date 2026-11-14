//! Closed turn-scoped subscriptions installed by a resolving card ability.
use super::events::subject::{SubjectFields, SubjectFilter};
use super::{
    Action, Binding, Bool, Capture, CaptureRef, CaptureTime, DelayedLifetime, Duration,
    DurationEnd, EnterFieldEvent, EntryMethod, EntryMethodFields, EventPattern,
    EventSnapshotPolicy, Expr, InstructionContext, Int, LeaveFieldEvent, LimitSpec, Nat, Node,
    ObjectOperand, PlayerRef, Predicate, Selector, TimeAnchor, TriggerAggregation,
    TriggerCondition, ValueEquality,
};

/// 「次に…とき」: the installation triggers once; simultaneous matches compete for that use.
pub(crate) const NEXT_ONLY: LimitSpec = LimitSpec::triggered_per_turn(Nat(1));

/// An installed entry subscription: the subject filter plus how the card entered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EntryPlan {
    pub(crate) filter: SubjectFilter,
    pub(crate) method: Option<EntryMethod>,
}

pub(crate) fn method(event: Binding<EnterFieldEvent>, method: EntryMethod) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::Equal {
        operands: Box::new(ValueEquality::EntryMethod {
            left: Expr::Frozen(Box::new(EntryMethodFields::EnterFieldMethod { event })),
            right: Expr::Literal(method),
        }),
    }))
}

/// Reads the closed entry predicate: a subject filter with at most one entry-method equality.
pub(crate) fn entry(event: Binding<EnterFieldEvent>, predicate: &Expr<Bool>) -> Option<EntryPlan> {
    let Expr::Compute(root) = predicate else {
        return None;
    };
    let Predicate::And { predicates } = root.as_ref() else {
        return None;
    };
    let mut found = None;
    let mut rest = Vec::new();
    for part in predicates {
        let matched = [EntryMethod::Play, EntryMethod::AbilityPut]
            .into_iter()
            .find(|candidate| *part == method(event, *candidate));
        match matched {
            Some(candidate) if found.replace(candidate).is_some() => return None,
            Some(_) => {}
            None => rest.push(part.clone()),
        }
    }
    let filter = SubjectFilter::parse(
        SubjectFields::Entry(event),
        &Expr::Compute(Box::new(Predicate::And { predicates: rest })),
    )?;
    // An installed observer is not a card on the field, so it never watches its own entry.
    (filter.itself != Some(true)).then_some(EntryPlan {
        filter,
        method: found,
    })
}

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
        || trigger.aggregation != (TriggerAggregation::PerFact {})
        || body.steps.is_empty()
    {
        return false;
    }
    let observes_entry = match (&trigger.on, captures.as_slice()) {
        (
            TriggerCondition::Event {
                pattern: EventPattern::LeaveField { binding, predicate },
            },
            [capture],
        ) if transfer(capture).is_some_and(|(_, _, into)| {
            into.binding.key().0 == body.scope && *predicate == cemetery(*binding, into)
        }) && trigger.view == EventSnapshotPolicy::BeforeExit
            && trigger.limit.is_none() =>
        {
            false
        }
        (
            TriggerCondition::Event {
                pattern: EventPattern::EnterField { binding, predicate },
            },
            [],
        ) if entry(*binding, predicate).is_some()
            && trigger.view == EventSnapshotPolicy::AfterEntry
            && trigger.limit.is_none_or(|limit| limit == NEXT_ONLY) =>
        {
            true
        }
        _ => return false,
    };
    body.steps.iter().all(|step| {
        let instruction = match &step.node {
            Node::Instruction { instruction } => instruction.as_ref(),
            // 「次に…出たとき、その中の1体は進化する」 reuses the closed effect evolution.
            Node::Control { control } if observes_entry => {
                let Some(projection) = super::effect_evolution::projection(control) else {
                    return false;
                };
                projection.instruction
            }
            Node::Control { .. } | Node::Capture { .. } | Node::Decision { .. } => return false,
        };
        instruction.source == *source
            && instruction.context == InstructionContext::Effect
            && instruction.precondition.is_none()
            && instruction.actor == (PlayerRef::Controller {})
            // Entry callbacks run the ordinary card body admission; departures stay closed.
            && if observes_entry {
                !matches!(instruction.action, Action::Delay { .. })
            } else {
                fixed_action(&instruction.action)
            }
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
                && matches!(tokens.as_slice(), [token] if matches!(token.template, TokenTemplate::Fixed { .. }) && token.count == Expr::Literal(Nat(1)))
        }
        _ => false,
    }
}
