use super::*;

#[test]
fn damage_accepts_only_ability_flags_instruction_batch_and_current_source() {
    let original = Action::Damage {
        recipients: RecipientSet::Leaders {
            players: controller_set(),
        },
        amount: DamageAmount::Uniform {
            amount: Expr::Literal(Int(2)),
        },
        flags: DamageFlags::Ability,
        source: ObjectView::Current {
            object: Box::new(ObjectOperand::SourceHolder {}),
        },
        batch: BatchPolicy::Instruction {},
    };
    accept(body(1, vec![instruction(original.clone(), 1, "BP02-052")]));
    for flag in [
        DamageFlags::Attack,
        DamageFlags::Combat,
        DamageFlags::AttackCombat,
    ] {
        let mut action = original.clone();
        if let Action::Damage { flags, .. } = &mut action {
            *flags = flag;
        }
        reject(
            body(1, vec![instruction(action, 1, "BP02-052")]),
            WellFormedReason::UnsupportedShape,
        );
    }
    for lki in [false, true] {
        let mut action = original.clone();
        if let Action::Damage { source, batch, .. } = &mut action {
            if lki {
                *source = ObjectView::LastKnown {
                    object: Box::new(ObjectOperand::SourceHolder {}),
                    reason: LkiReason::SourceDeparted,
                };
            } else {
                *batch = BatchPolicy::Simultaneous {
                    group: InstructionId(1),
                };
            }
        }
        reject(
            body(1, vec![instruction(action, 1, "BP02-052")]),
            WellFormedReason::UnsupportedShape,
        );
    }
}

#[test]
fn kind_predicates_reject_lki_and_live_event_capture() {
    let kind_value = |object| {
        Expr::Compute(Box::new(Predicate::KindEquals {
            left: Expr::Live(Box::new(KindRead::Kind {
                object,
                view: CharacteristicView::Effective,
            })),
            right: Expr::Literal(CardKind::Follower),
        }))
    };
    let captured = |value| Capture::Boolean {
        at: CaptureTime::AtResolution { scope: ScopeId(1) },
        value,
        into: CaptureRef {
            binding: Binding::new(1, ScopeId(1)),
        },
    };
    let current = kind_value(ObjectView::Current {
        object: Box::new(ObjectOperand::SourceHolder {}),
    });
    let capture_node = |value| Located {
        source: source("synthetic"),
        node: Node::Capture {
            capture: captured(value),
        },
    };
    accept(body(1, vec![capture_node(current.clone())]));
    reject(
        body(
            1,
            vec![capture_node(kind_value(ObjectView::LastKnown {
                object: Box::new(ObjectOperand::SourceHolder {}),
                reason: LkiReason::SourceDeparted,
            }))],
        ),
        WellFormedReason::UnsupportedShape,
    );
    let mut entry = card(body(1, vec![]), "synthetic").node;
    let ability = &mut entry.abilities[0].node;
    ability.kind = AbilityKind::Triggered;
    ability.trigger = Some(TriggerSpec {
        on: TriggerCondition::Event {
            pattern: EventPattern::EnterField {
                binding: Binding::new(2, ScopeId(1)),
                predicate: Expr::Literal(Bool(true)),
            },
        },
        active_zones: vec![ZoneKind::Field],
        view: EventSnapshotPolicy::AfterEntry,
        limit: None,
        aggregation: TriggerAggregation::PerFact {},
    });
    let mut event_capture = captured(current);
    if let Capture::Boolean { at, .. } = &mut event_capture {
        *at = CaptureTime::AtEvent {
            event: EventContext::Trigger {},
            policy: EventSnapshotPolicy::AfterEntry,
        };
    }
    ability.event_captures.push(event_capture);
    assert_eq!(
        WellFormedCardProgram::check(entry).unwrap_err().reason,
        WellFormedReason::UnsupportedShape
    );
}
