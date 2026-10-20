use super::*;
mod events;
mod review;

fn playable(steps: Vec<Located<Node>>) -> CardProgram {
    let mut card = card(body(1, steps), "BP01-007").node;
    card.abilities[0].node.play = Some(PlaySpec {
        captures: vec![],
        parameters: vec![],
        costs: CostSpec::Ordered { steps: vec![] },
    });
    card
}
fn play_target() -> Decision {
    let mut decision = chosen_objects(1, 1, zone(PlayerRef::Controller {}, ZoneKind::Field, 10));
    decision.kind = DecisionKind::PlayParameter {
        slot: PlaySlot::Targets,
        selection_snapshot: SelectionSnapshot::AtDecision,
        play_scope: ExecutionRef::Current {},
        input: InputMode::Inline,
    };
    decision
}
fn accept_card(card: CardProgram) {
    let expected = card.clone();
    let checked = WellFormedCardProgram::check(card).expect("valid card");
    assert_eq!(checked.card(), &expected);
    assert_eq!(checked.clone(), checked);
}
fn reject_card(card: CardProgram, reason: WellFormedReason) {
    let error = WellFormedCardProgram::check(card).expect_err("invalid card");
    assert_eq!(error.reason, reason, "{error}");
    assert!(error.source.is_some());
}
fn reveal_binding(id: u64, binding: Binding<Selector>) -> Located<Node> {
    instruction(
        Action::Reveal {
            objects: Selector::Bound { reference: binding },
        },
        id,
        "BP21-PR13",
    )
}
fn play_mut(card: &mut CardProgram) -> &mut PlaySpec {
    card.abilities[0].node.play.as_mut().unwrap()
}

#[test]
fn card_entry_owns_parameters_and_program_is_not_a_card_certificate() {
    let binding = Binding::new(1, ScopeId(1));
    let mut card = playable(vec![reveal_binding(2, binding)]);
    play_mut(&mut card).parameters.push(play_target());
    accept_card(card.clone());
    reject(
        card.abilities[0].node.body.clone().unwrap(),
        WellFormedReason::UnavailableBinding,
    );
    let mut later = card.clone();
    later.abilities.push(Located {
        source: source("BP01-007"),
        node: Ability {
            id: AbilityId(2),
            scope: ScopeId(2),
            play: Some(PlaySpec {
                captures: vec![],
                parameters: vec![],
                costs: CostSpec::Ordered { steps: vec![] },
            }),
            body: Some(body(
                2,
                vec![draw(3, Expr::Bound(Binding::new(99, ScopeId(2))))],
            )),
            ..later.abilities[0].node.clone()
        },
    });
    reject_card(later, WellFormedReason::UnavailableBinding);
    card.abilities[0].node.scope = ScopeId(4);
    reject_card(card, WellFormedReason::InvalidEntry);
}

#[test]
fn play_parameter_timing_slots_and_handoff_are_checked() {
    let mut card = playable(vec![]);
    play_mut(&mut card).parameters.push(play_target());
    let original = card.clone();
    for kind in [
        DecisionKind::PlayParameter {
            slot: PlaySlot::Targets,
            selection_snapshot: SelectionSnapshot::AtDecision,
            play_scope: ExecutionRef::Bound {
                binding: Binding::new(2, ScopeId(1)),
            },
            input: InputMode::Inline,
        },
        DecisionKind::PlayParameter {
            slot: PlaySlot::Targets,
            selection_snapshot: SelectionSnapshot::AtDecision,
            play_scope: ExecutionRef::Current {},
            input: InputMode::Handoff,
        },
        DecisionKind::RuleChoice {
            procedure: RuleProcedure::Capacity,
            rule: RuleId(1),
        },
    ] {
        let mut invalid = original.clone();
        play_mut(&mut invalid).parameters[0].kind = kind;
        reject_card(invalid, WellFormedReason::DecisionTiming);
    }
    play_mut(&mut card).parameters[0].actor = PlayerRef::RequireOne {
        players: Box::new(PlayerSet {
            relation: PlayerRelation::Opponents,
            ..controller_set()
        }),
    };
    reject_card(card.clone(), WellFormedReason::DecisionTiming);
    if let DecisionKind::PlayParameter { input, .. } = &mut play_mut(&mut card).parameters[0].kind {
        *input = InputMode::Handoff;
    }
    accept_card(card.clone());
    if let DecisionKind::PlayParameter { slot, .. } = &mut play_mut(&mut card).parameters[0].kind {
        *slot = PlaySlot::X;
    }
    reject_card(card, WellFormedReason::UnsupportedShape);
    reject(
        body(1, vec![decision(play_target())]),
        WellFormedReason::DecisionTiming,
    );
}

#[test]
fn may_answers_are_visible_throughout_execute_and_never_after_decline() {
    let mut choice = choose(1, 2);
    if let DecisionKind::ResolutionChoice { may, .. } = &mut choice.kind {
        *may = Declinable::MayDecline;
    }
    let may = control(Control::May {
        choice: choice.clone(),
        receipt: ReceiptId(9),
        body: body(
            2,
            vec![
                draw(2, signed(Expr::Bound(Binding::new(1, ScopeId(2))))),
                control(Control::Sequence {
                    body: body(
                        3,
                        vec![draw(3, signed(Expr::Bound(Binding::new(1, ScopeId(2)))))],
                    ),
                }),
            ],
        ),
    });
    accept(body(1, vec![may.clone(), draw(4, Expr::Literal(Int(1)))]));
    reject(
        body(
            1,
            vec![
                may.clone(),
                draw(4, signed(Expr::Bound(Binding::new(1, ScopeId(2))))),
            ],
        ),
        WellFormedReason::UnavailableBinding,
    );
    reject(
        body(2, vec![decision(choice)]),
        WellFormedReason::UnsupportedShape,
    );
    if let Node::Control {
        control:
            Control::May {
                choice: mut required_choice,
                body: inner,
                receipt,
            },
    } = may.node
    {
        if let DecisionKind::ResolutionChoice { may: flag, .. } = &mut required_choice.kind {
            *flag = Declinable::Required;
        }
        reject(
            body(
                1,
                vec![control(Control::May {
                    choice: required_choice,
                    body: inner,
                    receipt,
                })],
            ),
            WellFormedReason::DecisionTiming,
        );
    }
}

#[test]
fn compound_answer_domains_do_not_read_sibling_answers() {
    let mut choice = choose(1, 2);
    choice.kind = DecisionKind::ResolutionChoice {
        may: Declinable::MayDecline,
        cardinality: bounds(1, Some(1)),
        precondition: None,
    };
    choice.domain = ChoiceDomain::Compound {
        domains: vec![
            ChoiceDomain::Integer {
                cardinality: bounds(0, Some(3)),
                binding: Binding::new(1, ScopeId(2)),
            },
            ChoiceDomain::Objects {
                constraint: constraint(zone(PlayerRef::Controller {}, ZoneKind::Field, 10)),
                binding: Binding::new(2, ScopeId(2)),
                target_roles: None,
            },
        ],
    };
    let optional = |payload| {
        body(
            1,
            vec![control(Control::May {
                choice: payload,
                receipt: ReceiptId(9),
                body: body(2, vec![reveal_binding(3, Binding::new(2, ScopeId(2)))]),
            })],
        )
    };
    accept(optional(choice.clone()));
    if let ChoiceDomain::Compound { domains } = &mut choice.domain
        && let ChoiceDomain::Objects { constraint, .. } = &mut domains[1]
        && let Selector::Zone { predicate, .. } = &mut constraint.groups[0].selector
    {
        **predicate = positive(Expr::Bound(Binding::new(1, ScopeId(2))));
    }
    reject(optional(choice), WellFormedReason::UnavailableBinding);
}

fn roles(scope: u64) -> TargetRoleSpec {
    TargetRoleSpec {
        roles: vec![
            TargetRole {
                role: Nat(1),
                cardinality: RoleCardinality::IfNonempty { count: Nat(1) },
                binding: Binding::new(2, ScopeId(scope)),
            },
            TargetRole {
                role: Nat(2),
                cardinality: RoleCardinality::Remainder {},
                binding: Binding::new(3, ScopeId(scope)),
            },
        ],
    }
}
#[test]
fn bp21_pr13_roles_bind_in_the_same_answer_and_validate_the_partition() {
    let spec = roles(1);
    let mut card = playable(vec![
        reveal_binding(2, Binding::new(2, ScopeId(1))),
        reveal_binding(3, Binding::new(3, ScopeId(1))),
    ]);
    let mut target = play_target();
    if let ChoiceDomain::Objects {
        constraint,
        target_roles,
        ..
    } = &mut target.domain
    {
        constraint.cardinality = bounds(0, Some(3));
        constraint.groups[0].cardinality = bounds(0, Some(3));
        *target_roles = Some(spec.clone());
    }
    play_mut(&mut card).parameters.push(target);
    accept_card(card.clone());
    let a = ObjectRef {
        id: ObjectId(1),
        generation: Generation(1),
    };
    let b = ObjectRef {
        id: ObjectId(2),
        generation: Generation(1),
    };
    let answer = |first: Vec<ObjectRef>, rest: Vec<ObjectRef>| {
        vec![
            TargetRoleAnswer {
                role: Nat(1),
                objects: first,
            },
            TargetRoleAnswer {
                role: Nat(2),
                objects: rest,
            },
        ]
    };
    assert!(spec.accepts_answer(&[], &answer(vec![], vec![])));
    assert!(spec.accepts_answer(&[a], &answer(vec![a], vec![])));
    assert!(spec.accepts_answer(&[a, b], &answer(vec![a], vec![b])));
    assert!(!spec.accepts_answer(&[a, b], &answer(vec![a], vec![a, b])));
    assert!(!spec.accepts_answer(&[a, b], &answer(vec![a], vec![])));
    assert!(!spec.accepts_answer(&[a], &answer(vec![b], vec![])));
    assert!(!spec.accepts_answer(&[], &answer(vec![a], vec![])));
    assert!(!spec.accepts_answer(&[a, a], &answer(vec![a], vec![])));
    assert!(!spec.accepts_answer(&[a], &[]));
    for replacement in [
        RoleCardinality::IfNonempty { count: Nat(2) },
        RoleCardinality::Remainder {},
        RoleCardinality::Fixed {
            cardinality: bounds(1, Some(1)),
        },
    ] {
        let mut bad = card.clone();
        if let ChoiceDomain::Objects {
            target_roles: Some(edited_roles),
            ..
        } = &mut play_mut(&mut bad).parameters[0].domain
        {
            edited_roles.roles[0].cardinality = replacement;
        }
        reject_card(bad, WellFormedReason::InvalidRoles);
    }
    if let ChoiceDomain::Objects {
        target_roles: Some(edited_roles),
        ..
    } = &mut play_mut(&mut card).parameters[0].domain
    {
        edited_roles.roles[1].binding = edited_roles.roles[0].binding;
    }
    reject_card(card, WellFormedReason::DuplicateBinding);
}

fn at_play_capture(binding: Binding<Nat>, value: Expr<Nat>) -> Capture {
    Capture::Natural {
        at: CaptureTime::AtPlay {
            execution: ExecutionRef::Current {},
        },
        value,
        into: CaptureRef { binding },
    }
}
#[test]
fn entry_captures_are_simultaneous_and_cannot_read_future_body_outputs() {
    let bound = Binding::new(1, ScopeId(1));
    let mut card = playable(vec![draw(
        2,
        signed(Expr::Captured(CaptureRef { binding: bound })),
    )]);
    play_mut(&mut card)
        .captures
        .push(at_play_capture(bound, Expr::Literal(Nat(3))));
    accept_card(card.clone());
    play_mut(&mut card).captures.push(at_play_capture(
        Binding::new(2, ScopeId(1)),
        Expr::Captured(CaptureRef { binding: bound }),
    ));
    reject_card(card, WellFormedReason::UnavailableBinding);
    let mut future = playable(vec![draw(2, Expr::Literal(Int(2)))]);
    play_mut(&mut future).captures.push(at_play_capture(
        bound,
        actual(ReceiptReference::new(ReceiptId(2))),
    ));
    reject_card(future, WellFormedReason::UnavailableReceipt);
    reject(
        body(
            1,
            vec![Located {
                source: source("BP02-045"),
                node: Node::Capture {
                    capture: at_play_capture(bound, Expr::Literal(Nat(1))),
                },
            }],
        ),
        WellFormedReason::CaptureTiming,
    );
}

#[test]
fn selection_capture_reads_its_answer_and_is_not_a_later_live_read() {
    let mut card = playable(vec![]);
    let mut target = play_target();
    target.captures.push(Capture::Natural {
        at: CaptureTime::AtSelection {
            decision: target.id,
        },
        value: Expr::Compute(Box::new(NatOperation::Count {
            objects: Selector::Bound {
                reference: Binding::new(1, ScopeId(1)),
            },
        })),
        into: CaptureRef {
            binding: Binding::new(2, ScopeId(1)),
        },
    });
    play_mut(&mut card).parameters.push(target);
    card.abilities[0].node.body = Some(body(
        1,
        vec![draw(
            2,
            signed(Expr::Captured(CaptureRef {
                binding: Binding::new(2, ScopeId(1)),
            })),
        )],
    ));
    accept_card(card.clone());
    if let Capture::Natural { at, .. } = &mut play_mut(&mut card).parameters[0].captures[0] {
        *at = CaptureTime::AtSelection {
            decision: DecisionId(99),
        };
    }
    reject_card(card, WellFormedReason::CaptureTiming);
}

fn mode_group(at: SpellCountTime, necro: bool) -> Located<Node> {
    let read = if necro {
        NatFrozen::NecroCharge {
            player: PlayerRef::Controller {},
            execution: ExecutionRef::Current {},
            at,
        }
    } else {
        NatFrozen::SpellChain {
            player: PlayerRef::Controller {},
            execution: ExecutionRef::Current {},
            at,
        }
    };
    control(Control::Mode {
        modes: vec![
            Mode {
                id: ModeId(1),
                parameters: vec![],
                body: body(2, vec![draw(1, Expr::Literal(Int(1)))]),
            },
            Mode {
                id: ModeId(2),
                parameters: vec![],
                body: body(3, vec![]),
            },
            Mode {
                id: ModeId(3),
                parameters: vec![],
                body: body(4, vec![]),
            },
        ],
        count: Expr::Compute(Box::new(NatOperation::Conditional {
            condition: Expr::Compute(Box::new(Predicate::CompareNat {
                left: Expr::Frozen(Box::new(read)),
                comparison: Comparison::GreaterEqual,
                right: Expr::Literal(Nat(10)),
            })),
            then_value: Expr::Literal(Nat(3)),
            else_value: Expr::Literal(Nat(1)),
        })),
        chosen: ModeDecision {
            id: DecisionId(1),
            actor: PlayerRef::Controller {},
            domain: ChoiceDomain::Modes {
                modes: vec![ModeId(1), ModeId(2), ModeId(3)],
                cardinality: bounds(1, None),
                binding: Binding::new(9, ScopeId(1)),
                exclude_used: None,
            },
            kind: ModeDecisionKind::PlayParameter {
                play_scope: ExecutionRef::Current {},
                input: InputMode::Inline,
            },
            visibility: Visibility::Public {},
        },
        choice_kind: ModeChoiceKind::ChoiceKeyword,
    })
}
#[test]
fn bp02_045_and_bp02_088_mode_count_instead_uses_at_play_nat_conditional() {
    for necro in [false, true] {
        accept_card(playable(vec![mode_group(SpellCountTime::AtPlay, necro)]));
        reject_card(
            playable(vec![mode_group(SpellCountTime::AtResolution, necro)]),
            WellFormedReason::CaptureTiming,
        );
    }
    reject(
        body(1, vec![mode_group(SpellCountTime::AtPlay, false)]),
        WellFormedReason::DecisionTiming,
    );
    let mut mode = mode_group(SpellCountTime::AtPlay, false);
    if let Node::Control {
        control: Control::Mode { modes, .. },
    } = &mut mode.node
    {
        modes[2]
            .body
            .steps
            .push(draw(3, Expr::Bound(Binding::new(88, ScopeId(4)))));
    }
    reject_card(playable(vec![mode]), WellFormedReason::UnavailableBinding);
}

#[test]
fn mode_parameters_use_play_inputs_and_selected_body_keeps_its_own_answers() {
    let mut mode = mode_group(SpellCountTime::AtPlay, false);
    if let Node::Control {
        control: Control::Mode { modes, .. },
    } = &mut mode.node
    {
        let mut target = play_target();
        target.id = DecisionId(2);
        if let ChoiceDomain::Objects { binding, .. } = &mut target.domain {
            *binding = Binding::new(10, ScopeId(2));
        }
        modes[0].parameters.push(target);
        modes[0]
            .body
            .steps
            .push(reveal_binding(3, Binding::new(10, ScopeId(2))));
    }
    accept_card(playable(vec![mode.clone()]));
    if let Node::Control {
        control: Control::Mode { modes, .. },
    } = &mut mode.node
    {
        modes[1]
            .body
            .steps
            .push(reveal_binding(4, Binding::new(10, ScopeId(2))));
    }
    reject_card(playable(vec![mode]), WellFormedReason::UnavailableBinding);
}

#[test]
fn rule_and_quick_decisions_require_their_matching_procedure_entry() {
    let choice = Decision {
        id: DecisionId(1),
        captures: vec![],
        actor: PlayerRef::Active {},
        kind: DecisionKind::RuleChoice {
            procedure: RuleProcedure::GuardAct,
            rule: RuleId(1),
        },
        domain: ChoiceDomain::EndPhaseGuardAct {
            binding: Binding::new(1, ScopeId(1)),
        },
        visibility: Visibility::Public {},
    };
    let procedure = Procedure::Rule {
        procedure: RuleProcedure::GuardAct,
        rule: RuleId(1),
        decision: choice.clone(),
        scope: ScopeId(1),
    };
    let checked = WellFormedProcedure::check(procedure.clone()).unwrap();
    assert_eq!(checked.procedure(), &procedure);
    reject(
        body(1, vec![decision(choice.clone())]),
        WellFormedReason::DecisionTiming,
    );
    assert_eq!(
        WellFormedProcedure::check(Procedure::Rule {
            procedure: RuleProcedure::Capacity,
            rule: RuleId(1),
            decision: choice,
            scope: ScopeId(1)
        })
        .unwrap_err()
        .reason,
        WellFormedReason::DecisionTiming
    );
    let non_active = PlayerRef::RequireOne {
        players: Box::new(PlayerSet {
            relation: PlayerRelation::Opponents,
            relative_to: PlayerRef::Active {},
            ..controller_set()
        }),
    };
    let quick = Decision {
        id: DecisionId(2),
        captures: vec![],
        actor: non_active.clone(),
        domain: ChoiceDomain::QuickActionOrPass {
            player: non_active.clone(),
        },
        kind: DecisionKind::QuickWindow {
            window: QuickWindow::Attack,
        },
        visibility: Visibility::Only {
            players: PlayerSet {
                relative_to: non_active,
                ..controller_set()
            },
        },
    };
    WellFormedProcedure::check(Procedure::Quick {
        window: QuickWindow::Attack,
        decision: quick.clone(),
    })
    .unwrap();
    assert_eq!(
        WellFormedProcedure::check(Procedure::Quick {
            window: QuickWindow::EndPhase,
            decision: quick.clone()
        })
        .unwrap_err()
        .reason,
        WellFormedReason::DecisionTiming
    );
    reject(
        body(1, vec![decision(quick)]),
        WellFormedReason::DecisionTiming,
    );
}

#[test]
fn stepwise_protocol_orders_decisions_and_does_not_preselect_a_count() {
    let mut draw = StepwiseSession::new(StepwiseTiming::BeforeDraw, Nat(3));
    assert_eq!(draw.phase(), StepwisePhase::Decide);
    assert!(!draw.performed());
    assert!(draw.answer(StepwiseAnswer::Stop));
    assert_eq!(draw.phase(), StepwisePhase::Complete);
    assert!(!draw.answer(StepwiseAnswer::Continue));
    let mut look = StepwiseSession::new(StepwiseTiming::AfterLook, Nat(2));
    assert_eq!(look.phase(), StepwisePhase::Decide);
    assert!(look.answer(StepwiseAnswer::Continue));
    assert!(!look.answer(StepwiseAnswer::Stop));
    assert!(look.performed());
    assert_eq!(look.phase(), StepwisePhase::Decide);
    assert!(look.answer(StepwiseAnswer::Continue));
    assert!(look.performed());
    assert_eq!(look.phase(), StepwisePhase::Complete);
    for timing in [StepwiseTiming::BeforeDraw, StepwiseTiming::AfterLook] {
        let zero = StepwiseSession::new(timing, Nat(0));
        assert_eq!(zero.phase(), StepwisePhase::Complete);
        let mut exhausted = StepwiseSession::new(timing, Nat(5));
        if exhausted.phase() == StepwisePhase::Decide {
            assert!(exhausted.answer(StepwiseAnswer::Continue));
        }
        assert!(exhausted.performed());
        assert_eq!(exhausted.phase(), StepwisePhase::Decide);
        assert!(exhausted.answer(StepwiseAnswer::Stop));
    }
}

#[test]
fn stepwise_decision_timing_actor_visibility_and_identity_are_checked() {
    let valid = up_to_draw(1);
    accept(body(1, vec![valid.clone()]));
    for change in 0_u8..3 {
        let mut invalid = valid.clone();
        if let Node::Instruction { instruction } = &mut invalid.node
            && let Action::Draw {
                mode: StepwiseMode::UpToStepwise { decision },
                ..
            } = &mut instruction.action
        {
            match change {
                0 => decision.timing = StepwiseTiming::AfterLook,
                1 => decision.actor = PlayerRef::Owner {},
                _ => decision.visibility = Visibility::Public {},
            }
        }
        reject(
            body(1, vec![invalid]),
            if change < 2 {
                WellFormedReason::DecisionTiming
            } else {
                WellFormedReason::UnsupportedShape
            },
        );
    }
    reject(
        body(1, vec![decision(choose(1, 1)), valid]),
        WellFormedReason::DuplicateDecision,
    );
}

fn triggered() -> CardProgram {
    let mut card = playable(vec![draw(
        2,
        signed(Expr::Captured(CaptureRef {
            binding: Binding::new(2, ScopeId(1)),
        })),
    )]);
    let ability = &mut card.abilities[0].node;
    ability.kind = AbilityKind::Triggered;
    ability.trigger = Some(TriggerSpec {
        on: TriggerCondition::Event {
            pattern: EventPattern::LeaveField {
                binding: Binding::new(1, ScopeId(1)),
                predicate: Expr::Literal(Bool(true)),
            },
        },
        active_zones: vec![ZoneKind::Field],
        view: EventSnapshotPolicy::BeforeExit,
        limit: None,
        aggregation: TriggerAggregation::PerFact {},
    });
    ability.event_captures.push(Capture::Natural {
        at: CaptureTime::AtEvent {
            event: EventContext::Trigger {},
            policy: EventSnapshotPolicy::BeforeExit,
        },
        value: Expr::Literal(Nat(3)),
        into: CaptureRef {
            binding: Binding::new(2, ScopeId(1)),
        },
    });
    card
}
#[test]
fn event_capture_requires_the_trigger_entry_and_matching_snapshot() {
    let card = triggered();
    accept_card(card.clone());
    let mut wrong = card.clone();
    if let Capture::Natural { at, .. } = &mut wrong.abilities[0].node.event_captures[0] {
        *at = CaptureTime::AtEvent {
            event: EventContext::Trigger {},
            policy: EventSnapshotPolicy::AfterEntry,
        };
    }
    reject_card(wrong, WellFormedReason::CaptureTiming);
    let mut no_event = card.clone();
    no_event.abilities[0].node.kind = AbilityKind::Spell;
    no_event.abilities[0].node.trigger = None;
    reject_card(no_event, WellFormedReason::InvalidEntry);
    let mut state = card.clone();
    state.abilities[0].node.trigger.as_mut().unwrap().on = TriggerCondition::State {
        predicate: Expr::Literal(Bool(true)),
        latch: Binding::new(1, ScopeId(1)),
    };
    reject_card(state, WellFormedReason::UnsupportedShape);
    let mut future = card.clone();
    future.abilities[0].node.event_captures = vec![Capture::Natural {
        at: CaptureTime::AtEvent {
            event: EventContext::Trigger {},
            policy: EventSnapshotPolicy::BeforeExit,
        },
        value: actual(ReceiptReference::new(ReceiptId(2))),
        into: CaptureRef {
            binding: Binding::new(2, ScopeId(1)),
        },
    }];
    reject_card(future, WellFormedReason::UnavailableReceipt);
    let mut bad_trigger = card;
    if let TriggerCondition::Event {
        pattern: EventPattern::LeaveField { predicate, .. },
    } = &mut bad_trigger.abilities[0].node.trigger.as_mut().unwrap().on
    {
        *predicate = positive(Expr::Captured(CaptureRef {
            binding: Binding::new(2, ScopeId(1)),
        }));
    }
    reject_card(bad_trigger, WellFormedReason::UnavailableBinding);
}

fn continuous(patch: CharacteristicPatch, layer: ContinuousLayer) -> Effect {
    Effect::Continuous {
        id: EffectDefinitionId(1),
        source: ability_source(),
        target: EffectTarget::Dynamic {
            selector: Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
        },
        condition: None,
        patch,
        layer,
        timestamp: TimestampBasis::ZoneEntry {
            object: ObjectOperand::SourceHolder {},
        },
        active_zones: vec![ZoneKind::Field],
        duration: Duration {
            end: DurationEnd::NoTemporalExpiry {},
            anchor: TimeAnchor::Installation {},
            usage: None,
            intervals: vec![],
        },
    }
}
#[test]
fn entire_card_checks_declarations_and_nested_granted_ability_bodies() {
    let mut card = playable(vec![]);
    let parent = &mut card.abilities[0].node;
    parent.kind = AbilityKind::Static;
    parent.play = None;
    parent.body = None;
    parent.declarations.push(continuous(
        CharacteristicPatch::Numeric {
            stat: Stat::Attack,
            change: NumericChange::Add {
                amount: Expr::Literal(Int(1)),
            },
        },
        ContinuousLayer::Numeric,
    ));
    accept_card(card.clone());
    let mut invalid = card.clone();
    invalid.abilities[0].node.declarations = vec![continuous(
        CharacteristicPatch::Numeric {
            stat: Stat::Attack,
            change: NumericChange::Add {
                amount: Expr::Bound(Binding::new(99, ScopeId(1))),
            },
        },
        ContinuousLayer::Numeric,
    )];
    reject_card(invalid, WellFormedReason::UnavailableBinding);
    let mut granted = playable(vec![]).abilities.remove(0).node;
    granted.id = AbilityId(2);
    granted.scope = ScopeId(2);
    granted.body = Some(body(2, vec![draw(3, Expr::Literal(Int(1)))]));
    card.abilities[0].node.declarations = vec![continuous(
        CharacteristicPatch::GrantAbility {
            abilities: vec![GrantedAbility::Definition {
                ability: Box::new(granted.clone()),
            }],
        },
        ContinuousLayer::Ability,
    )];
    accept_card(card.clone());
    granted
        .body
        .as_mut()
        .unwrap()
        .steps
        .push(draw(4, Expr::Bound(Binding::new(99, ScopeId(2)))));
    card.abilities[0].node.declarations = vec![continuous(
        CharacteristicPatch::GrantAbility {
            abilities: vec![GrantedAbility::Definition {
                ability: Box::new(granted),
            }],
        },
        ContinuousLayer::Ability,
    )];
    reject_card(card, WellFormedReason::UnavailableBinding);
}

#[test]
fn may_empty_execute_payload_never_asks_a_second_question() {
    let choice = Decision {
        domain: ChoiceDomain::Compound { domains: vec![] },
        kind: DecisionKind::ResolutionChoice {
            may: Declinable::MayDecline,
            cardinality: bounds(1, Some(1)),
            precondition: None,
        },
        ..choose(1, 2)
    };
    accept(body(
        1,
        vec![control(Control::May {
            choice: choice.clone(),
            body: body(2, vec![draw(2, Expr::Literal(Int(1)))]),
            receipt: ReceiptId(9),
        })],
    ));
    for nested in [
        decision(choose(2, 2)),
        up_to_draw(2),
        declare(2, Binding::new(2, ScopeId(2))),
        discard(DiscardMode::Chosen {
            constraint: constraint(hand()),
        }),
    ] {
        reject(
            body(
                1,
                vec![control(Control::May {
                    choice: choice.clone(),
                    body: body(2, vec![nested]),
                    receipt: ReceiptId(9),
                })],
            ),
            WellFormedReason::UnsupportedShape,
        );
    }
}

#[test]
fn play_modes_cannot_be_hidden_behind_resolution_control_flow() {
    let mode = mode_group(SpellCountTime::AtPlay, false);
    for hidden in [
        control(Control::If {
            condition: Expr::Literal(Bool(true)),
            then_body: body(5, vec![mode.clone()]),
            else_body: None,
        }),
        each_player(mode.clone(), StageMode::ChoiceProcedure {}),
    ] {
        reject_card(playable(vec![hidden]), WellFormedReason::DecisionTiming);
    }
    let mut nested = mode;
    if let Node::Control {
        control: Control::Mode { chosen, .. },
    } = &mut nested.node
        && let ChoiceDomain::Modes { binding, .. } = &mut chosen.domain
    {
        *binding = Binding::new(9, ScopeId(5));
    }
    accept_card(playable(vec![control(Control::Sequence {
        body: body(5, vec![nested]),
    })]));
}

#[test]
fn mode_count_does_not_read_later_targets_or_live_state() {
    let mut card = playable(vec![mode_group(SpellCountTime::AtPlay, false)]);
    play_mut(&mut card).parameters.push(play_target());
    // Mode and target decision identities are independent, even though their phases differ.
    play_mut(&mut card).parameters[0].id = DecisionId(2);
    if let Node::Control {
        control: Control::Mode { count, .. },
    } = &mut card.abilities[0].node.body.as_mut().unwrap().steps[0].node
        && let Expr::Compute(operation) = count
        && let NatOperation::Conditional { condition, .. } = operation.as_mut()
    {
        *condition = Expr::Compute(Box::new(Predicate::Exists {
            objects: Selector::Bound {
                reference: Binding::new(1, ScopeId(1)),
            },
        }));
    }
    reject_card(card, WellFormedReason::UnavailableBinding);
}

#[test]
fn mode_answers_respect_evaluated_count_zero_clamping_and_unique_ids() {
    let Node::Control {
        control: Control::Mode { chosen, .. },
    } = mode_group(SpellCountTime::AtPlay, false).node
    else {
        panic!("mode");
    };
    assert!(chosen.accepts_answer(Nat(1), &[ModeId(1)]));
    assert!(!chosen.accepts_answer(Nat(1), &[ModeId(1), ModeId(2)]));
    assert!(chosen.accepts_answer(Nat(3), &[ModeId(1), ModeId(2), ModeId(3)]));
    assert!(chosen.accepts_answer(Nat(99), &[ModeId(1), ModeId(2), ModeId(3)]));
    assert!(chosen.accepts_answer(Nat(0), &[]));
    assert!(!chosen.accepts_answer(Nat(0), &[ModeId(1)]));
    assert!(!chosen.accepts_answer(Nat(3), &[ModeId(1), ModeId(1)]));
    assert!(!chosen.accepts_answer(Nat(3), &[ModeId(9)]));
}
