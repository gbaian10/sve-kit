use super::*;

fn event_card(
    pattern: EventPattern,
    view: EventSnapshotPolicy,
    steps: Vec<Located<Node>>,
) -> CardProgram {
    let mut card = playable(steps);
    let ability = &mut card.abilities[0].node;
    ability.kind = AbilityKind::Triggered;
    ability.play = None;
    ability.trigger = Some(TriggerSpec {
        on: TriggerCondition::Event { pattern },
        active_zones: vec![ZoneKind::Field],
        view,
        limit: None,
        aggregation: TriggerAggregation::PerFact {},
    });
    card
}

fn leave(predicate: Expr<Bool>) -> EventPattern {
    EventPattern::LeaveField {
        binding: Binding::new(1, ScopeId(1)),
        predicate,
    }
}

fn branch(condition: Expr<Bool>) -> Located<Node> {
    control(Control::If {
        condition,
        then_body: body(20, vec![]),
        else_body: None,
    })
}

fn cause(event: EventContext) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::EventCause {
        event,
        pattern: CausePattern::Effect {
            source: EffectSource::Any {},
        },
    }))
}

#[test]
fn event_header_requires_a_real_entry_and_the_exact_payload_type() {
    let binding = Binding::new(1, ScopeId(1));
    let predicate = cause(EventContext::LeaveField { event: binding });
    accept_card(event_card(
        leave(predicate.clone()),
        EventSnapshotPolicy::BeforeExit,
        vec![],
    ));
    reject(
        body(1, vec![branch(cause(EventContext::Trigger {}))]),
        WellFormedReason::UnsupportedShape,
    );
    reject_card(
        event_card(
            leave(cause(EventContext::Damage {
                event: Binding::new(1, ScopeId(1)),
            })),
            EventSnapshotPolicy::BeforeExit,
            vec![],
        ),
        WellFormedReason::BindingType,
    );
    reject_card(
        event_card(
            leave(cause(EventContext::LeaveField {
                event: Binding::new(2, ScopeId(1)),
            })),
            EventSnapshotPolicy::BeforeExit,
            vec![],
        ),
        WellFormedReason::UnavailableBinding,
    );
    let header = Expr::Compute(Box::new(Predicate::Equal {
        operands: Box::new(ValueEquality::EventId {
            left: Expr::Frozen(Box::new(EventIdHeaderFields::HeaderId {
                event: EventContext::Trigger {},
            })),
            right: Expr::Frozen(Box::new(EventIdHeaderFields::HeaderId {
                event: EventContext::LeaveField { event: binding },
            })),
        }),
    }));
    accept_card(event_card(
        leave(predicate),
        EventSnapshotPolicy::BeforeExit,
        vec![branch(header)],
    ));
}

#[test]
fn event_union_narrows_each_predicate_without_exporting_a_branch_payload() {
    let pattern = EventPattern::AnyOf {
        patterns: vec![
            leave(cause(EventContext::LeaveField {
                event: Binding::new(1, ScopeId(1)),
            })),
            EventPattern::Destroy {
                binding: Binding::new(2, ScopeId(1)),
                predicate: cause(EventContext::Destroy {
                    event: Binding::new(2, ScopeId(1)),
                }),
            },
        ],
    };
    accept_card(event_card(
        pattern.clone(),
        EventSnapshotPolicy::BeforeExit,
        vec![branch(cause(EventContext::Trigger {}))],
    ));
    reject_card(
        event_card(
            pattern.clone(),
            EventSnapshotPolicy::BeforeExit,
            vec![branch(cause(EventContext::LeaveField {
                event: Binding::new(1, ScopeId(1)),
            }))],
        ),
        WellFormedReason::UnavailableBinding,
    );
    let mut sibling = pattern;
    if let EventPattern::AnyOf { patterns } = &mut sibling
        && let EventPattern::Destroy { predicate, .. } = &mut patterns[1]
    {
        *predicate = cause(EventContext::LeaveField {
            event: Binding::new(1, ScopeId(1)),
        });
    }
    reject_card(
        event_card(sibling, EventSnapshotPolicy::BeforeExit, vec![]),
        WellFormedReason::UnavailableBinding,
    );
    reject_card(
        event_card(
            EventPattern::AnyOf { patterns: vec![] },
            EventSnapshotPolicy::BeforeExit,
            vec![],
        ),
        WellFormedReason::UnsupportedShape,
    );
}

#[test]
fn bp07_088_captures_prayer_from_the_leave_snapshot() {
    let mut card = event_card(
        leave(Expr::Compute(Box::new(Predicate::ObjectEquals {
            left: Expr::Frozen(Box::new(ObjectFrozen::LeaveFieldObjectBefore {
                event: Binding::new(1, ScopeId(1)),
            })),
            right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
        }))),
        EventSnapshotPolicy::BeforeExit,
        vec![draw(
            1,
            signed(Expr::Captured(CaptureRef {
                binding: Binding::new(2, ScopeId(1)),
            })),
        )],
    );
    card.abilities[0]
        .node
        .event_captures
        .push(Capture::Natural {
            at: CaptureTime::AtEvent {
                event: EventContext::Trigger {},
                policy: EventSnapshotPolicy::BeforeExit,
            },
            value: Expr::Frozen(Box::new(NatFrozen::SnapshotCounter {
                object: Expr::Frozen(Box::new(SnapshotFields::LeaveFieldLki {
                    event: Binding::new(1, ScopeId(1)),
                })),
                kind: CounterKind("prayer".to_owned()),
            })),
            into: CaptureRef {
                binding: Binding::new(2, ScopeId(1)),
            },
        });
    accept_card(card.clone());
    if let Capture::Natural { value, .. } = &mut card.abilities[0].node.event_captures[0] {
        *value = Expr::Live(Box::new(NatRead::Counter {
            holder: Holder::Object {
                object: ObjectOperand::SourceHolder {},
            },
            kind: CounterKind("prayer".to_owned()),
        }));
    }
    reject_card(card, WellFormedReason::UnsupportedShape);
}

#[test]
fn follow_uses_only_a_triggering_move_subject_not_the_receipt_collection() {
    let follow = instruction(
        Action::Reveal {
            objects: Selector::Objects {
                objects: vec![ObjectOperand::FollowEventMove {
                    event: MovementEvent::LeaveField {
                        event: Binding::new(1, ScopeId(1)),
                    },
                }],
            },
        },
        1,
        "synthetic",
    );
    accept_card(event_card(
        leave(Expr::Literal(Bool(true))),
        EventSnapshotPolicy::BeforeExit,
        vec![follow.clone()],
    ));
    reject(body(1, vec![follow]), WellFormedReason::UnavailableBinding);
    let all = instruction(
        Action::Reveal {
            objects: Selector::MoveAfter {
                receipt: MovementReceipt::TriggerEvent {
                    event: MovementEvent::LeaveField {
                        event: Binding::new(1, ScopeId(1)),
                    },
                },
                permission: FollowPermission::TriggerExplicitReference {
                    event: EventContext::Trigger {},
                },
            },
        },
        1,
        "synthetic",
    );
    reject_card(
        event_card(
            leave(Expr::Literal(Bool(true))),
            EventSnapshotPolicy::BeforeExit,
            vec![all],
        ),
        WellFormedReason::UnsupportedShape,
    );
}

#[test]
fn effect_source_matching_keeps_its_candidate_local_to_that_cause() {
    let source = Binding::new(1, ScopeId(3));
    let matched = Expr::Compute(Box::new(Predicate::EventCause {
        event: EventContext::Trigger {},
        pattern: CausePattern::Effect {
            source: EffectSource::Matching {
                predicate: Box::new(SourcePredicate {
                    binding: source,
                    condition: Expr::Compute(Box::new(Predicate::ObjectEquals {
                        left: Expr::Bound(source),
                        right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
                    })),
                }),
            },
        },
    }));
    accept_card(event_card(
        leave(matched.clone()),
        EventSnapshotPolicy::BeforeExit,
        vec![],
    ));
    let leaked = Expr::Compute(Box::new(Predicate::And {
        predicates: vec![
            matched,
            Expr::Compute(Box::new(Predicate::ObjectEquals {
                left: Expr::Bound(source),
                right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
            })),
        ],
    }));
    reject_card(
        event_card(leave(leaked), EventSnapshotPolicy::BeforeExit, vec![]),
        WellFormedReason::UnavailableBinding,
    );
    for pattern in [
        CausePattern::AnyRule {},
        CausePattern::Rule { rule: RuleId(1) },
        CausePattern::PlayerAction {},
        CausePattern::Any {},
    ] {
        accept_card(event_card(
            leave(Expr::Compute(Box::new(Predicate::EventCause {
                event: EventContext::Trigger {},
                pattern,
            }))),
            EventSnapshotPolicy::BeforeExit,
            vec![],
        ));
    }
}

fn static_card(effect: Effect) -> CardProgram {
    let mut card = playable(vec![]);
    let ability = &mut card.abilities[0].node;
    ability.kind = AbilityKind::Static;
    ability.play = None;
    ability.body = None;
    ability.declarations.push(effect);
    card
}

fn source_objects() -> Selector {
    Selector::Objects {
        objects: vec![ObjectOperand::SourceHolder {}],
    }
}

fn permanent() -> Duration {
    Duration {
        end: DurationEnd::NoTemporalExpiry {},
        anchor: TimeAnchor::Installation {},
        usage: None,
        intervals: vec![],
    }
}

fn redirect() -> Effect {
    Effect::Replacement {
        id: EffectDefinitionId(1),
        source: ability_source(),
        pattern: ProposedEventPattern::LeavesZone {
            objects: source_objects(),
            from: ZoneKind::Field,
        },
        condition: Expr::Literal(Bool(true)),
        rewrite: EventRewrite::RedirectMove {
            to: MoveDestination::Owners {
                zone: OwnerZone::Deck,
            },
            position: Some(Position::Top {}),
        },
        scope: ReplacementScope::General,
        affected_player: PlayerRef::Controller {},
        once_per_event: OncePerEvent::Enforced,
        usage: None,
        prospective_chars: None,
        duration: permanent(),
    }
}

#[test]
fn static_holder_redirect_is_general_and_keeps_deck_top() {
    let original = static_card(redirect());
    accept_card(original.clone());
    for change in 0_u8..6 {
        let mut card = original.clone();
        let Effect::Replacement {
            pattern,
            rewrite,
            condition,
            affected_player,
            scope,
            ..
        } = &mut card.abilities[0].node.declarations[0]
        else {
            panic!("replacement")
        };
        match change {
            0 => {
                *rewrite = EventRewrite::RedirectMove {
                    to: MoveDestination::Owners {
                        zone: OwnerZone::Hand,
                    },
                    position: Some(Position::Top {}),
                }
            }
            1 => {
                *rewrite = EventRewrite::RedirectMove {
                    to: MoveDestination::Shared {
                        zone: ZoneRef {
                            player: PlayerRef::Controller {},
                            kind: ZoneKind::Deck,
                        },
                    },
                    position: Some(Position::Top {}),
                }
            }
            2 => {
                *pattern = ProposedEventPattern::LeavesZone {
                    objects: source_objects(),
                    from: ZoneKind::Hand,
                }
            }
            3 => *condition = Expr::Bound(Binding::new(99, ScopeId(1))),
            // A spell's own conditional redirect (BP02-041) is a self-replacement, not this shape.
            4 => *scope = ReplacementScope::SelfOnly,
            _ => {
                *affected_player = PlayerRef::Bound {
                    binding: Binding::new(99, ScopeId(1)),
                }
            }
        }
        reject_card(
            card,
            match change {
                0 | 1 => WellFormedReason::InvalidMovement,
                2 | 4 => WellFormedReason::UnsupportedShape,
                _ => WellFormedReason::UnavailableBinding,
            },
        );
    }
}

#[test]
fn bp02_089_permission_keeps_effect_destruction_distinct_from_rule_destruction() {
    let effect = Effect::Permission {
        subject: PermissionSubject::Objects {
            objects: source_objects(),
        },
        action: ActionPattern::Destroy {
            objects: source_objects(),
            cause: CausePattern::Effect {
                source: EffectSource::Any {},
            },
        },
        polarity: Polarity::Forbid,
        condition: Expr::Literal(Bool(true)),
        duration: permanent(),
        rule: RuleId(1),
    };
    let checked = WellFormedCardProgram::check(static_card(effect.clone())).unwrap();
    assert_eq!(checked.card().abilities[0].node.declarations[0], effect);
    let mut rule = effect.clone();
    if let Effect::Permission {
        action: ActionPattern::Destroy { cause, .. },
        ..
    } = &mut rule
    {
        *cause = CausePattern::AnyRule {};
    }
    assert_ne!(rule, effect);
    accept_card(static_card(rule));
    let mut invalid = effect;
    if let Effect::Permission { condition, .. } = &mut invalid {
        *condition = Expr::Bound(Binding::new(99, ScopeId(1)));
    }
    reject_card(static_card(invalid), WellFormedReason::UnavailableBinding);
}

fn look_deck(viewer: PlayerRef) -> Located<Node> {
    instruction(
        Action::Look {
            viewer,
            zone: ZoneRef {
                player: PlayerRef::Controller {},
                kind: ZoneKind::Deck,
            },
            count: Expr::Literal(Nat(3)),
            mode: StepwiseMode::Exact,
        },
        1,
        "synthetic",
    )
}

fn looked_cards() -> Selector {
    Selector::ReceiptMembers {
        receipt: ObjectReceipt::Look {
            receipt: ReceiptReference::new(ReceiptId(1)),
        },
        projection: MemberProjection::After,
    }
}

#[test]
fn a_live_player_role_cannot_name_a_private_audience_across_a_control_change() {
    let holder_controller = PlayerRef::FromExpression {
        value: Box::new(Expr::Live(Box::new(PlayerRead::Role {
            object: ObjectView::Current {
                object: Box::new(ObjectOperand::SourceHolder {}),
            },
            role: Role::Controller,
        }))),
    };
    let give_away = instruction(
        Action::Transfer {
            objects: source_objects(),
            destination_controller: PlayerRef::RequireOne {
                players: Box::new(PlayerSet {
                    relation: PlayerRelation::Opponents,
                    ..controller_set()
                }),
            },
        },
        2,
        "synthetic",
    );
    let choose = |actor: PlayerRef| {
        let mut choice = chosen_objects(3, 1, looked_cards());
        choice.actor = actor.clone();
        choice.visibility = Visibility::Only {
            players: PlayerSet {
                relative_to: actor,
                ..controller_set()
            },
        };
        decision(choice)
    };
    accept(body(
        1,
        vec![
            look_deck(PlayerRef::Controller {}),
            give_away.clone(),
            choose(PlayerRef::Controller {}),
        ],
    ));
    reject(
        body(
            1,
            vec![
                look_deck(holder_controller.clone()),
                give_away,
                choose(holder_controller),
            ],
        ),
        WellFormedReason::UnsupportedShape,
    );
    let leaver = PlayerRef::FromExpression {
        value: Box::new(Expr::Frozen(Box::new(PlayerFrozen::LeaveFieldController {
            event: Binding::new(1, ScopeId(1)),
        }))),
    };
    accept_card(event_card(
        leave(Expr::Literal(Bool(true))),
        EventSnapshotPolicy::BeforeExit,
        vec![instruction(
            Action::Life {
                player: leaver,
                change: NumericChange::Add {
                    amount: Expr::Literal(Int(1)),
                },
            },
            1,
            "synthetic",
        )],
    ));
}

#[test]
fn a_snapshot_player_role_is_stable_but_requires_its_declared_event() {
    let predicate = |object| {
        Expr::Compute(Box::new(Predicate::ZoneEquals {
            left: Expr::Frozen(Box::new(ZoneFrozen::LeaveFieldTo {
                event: Binding::new(1, ScopeId(1)),
            })),
            right: Expr::Compute(Box::new(ZoneOperation::OfPlayer {
                player: PlayerRef::FromExpression {
                    value: Box::new(Expr::Live(Box::new(PlayerRead::Role {
                        object,
                        role: Role::Owner,
                    }))),
                },
                kind: ZoneKind::Cemetery,
            })),
        }))
    };
    let snapshot = |id| ObjectView::Snapshot {
        value: Box::new(Expr::Frozen(Box::new(SnapshotFields::LeaveFieldLki {
            event: Binding::new(id, ScopeId(1)),
        }))),
    };
    accept_card(event_card(
        leave(predicate(snapshot(1))),
        EventSnapshotPolicy::BeforeExit,
        vec![],
    ));
    reject_card(
        event_card(
            leave(predicate(snapshot(2))),
            EventSnapshotPolicy::BeforeExit,
            vec![],
        ),
        WellFormedReason::UnavailableBinding,
    );
    reject_card(
        event_card(
            leave(predicate(ObjectView::Current {
                object: Box::new(ObjectOperand::SourceHolder {}),
            })),
            EventSnapshotPolicy::BeforeExit,
            vec![],
        ),
        WellFormedReason::UnsupportedShape,
    );
}
