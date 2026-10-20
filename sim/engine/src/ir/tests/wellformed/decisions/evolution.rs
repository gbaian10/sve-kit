use super::*;

fn keyword(keyword: KeywordId, line: u64) -> AbilitySource {
    AbilitySource {
        origin: AbilityOrigin::Keyword {
            keyword,
            origin: Box::new(AbilityOrigin::Printed {
                face: FaceId("base".into()),
                line: Nat(line),
            }),
        },
        ..ability_source()
    }
}

fn assign_source(node: &mut Located<Node>, source: &AbilitySource) {
    match &mut node.node {
        Node::Instruction { instruction } => instruction.source = source.clone(),
        Node::Control {
            control: Control::EachPlayer { stages, .. },
        } => {
            for stage in stages {
                for step in &mut stage.body.steps {
                    assign_source(step, source);
                }
            }
        }
        Node::Control {
            control:
                Control::If {
                    then_body,
                    else_body,
                    ..
                },
        } => {
            for step in &mut then_body.steps {
                assign_source(step, source);
            }
            if let Some(branch) = else_body {
                for step in &mut branch.steps {
                    assign_source(step, source);
                }
            }
        }
        Node::Control { .. } | Node::Capture { .. } | Node::Decision { .. } => {
            panic!("unexpected fanfare body shape");
        }
    }
}

fn fanfare() -> Located<Ability> {
    let actor = PlayerRef::Bound {
        binding: Binding::new(1, ScopeId(2)),
    };
    let mut each = player_cards::mitra_body(actor.clone(), actor);
    let source = keyword(KeywordId::Fanfare, 2);
    assign_source(&mut each, &source);
    let mut entry = card(body(1, vec![each]), "BP06-110")
        .node
        .abilities
        .remove(0);
    let event = Binding::new(90, ScopeId(1));
    entry.node.source = source;
    entry.node.kind = AbilityKind::Triggered;
    entry.node.trigger = Some(TriggerSpec {
        on: TriggerCondition::Event {
            pattern: EventPattern::EnterField {
                binding: event,
                predicate: Expr::Compute(Box::new(Predicate::ObjectEquals {
                    left: Expr::Frozen(Box::new(ObjectFrozen::EnterFieldObject { event })),
                    right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
                })),
            },
        },
        active_zones: vec![ZoneKind::Field],
        view: EventSnapshotPolicy::AfterEntry,
        limit: None,
        aggregation: TriggerAggregation::PerFact {},
    });
    entry
}

fn face() -> Decision {
    let mut selector = zone(PlayerRef::Controller {}, ZoneKind::Evolve, 30);
    let names = |object| {
        Expr::Live(Box::new(NamesRead::Names {
            object: ObjectView::Current {
                object: Box::new(object),
            },
        }))
    };
    if let Selector::Zone {
        binding, predicate, ..
    } = &mut selector
    {
        **predicate = Expr::Compute(Box::new(Predicate::NamesIntersect {
            left: names(ObjectOperand::Bound { binding: *binding }),
            right: names(ObjectOperand::SourceHolder {}),
        }));
    }
    let mut result = chosen_objects(20, 10, selector);
    result.visibility = Visibility::Only {
        players: controller_set(),
    };
    result.kind = DecisionKind::PlayParameter {
        slot: PlaySlot::Face,
        selection_snapshot: SelectionSnapshot::AtDecision,
        play_scope: ExecutionRef::Current {},
        input: InputMode::Inline,
    };
    if let ChoiceDomain::Objects { constraint, .. } = &mut result.domain {
        constraint.cardinality = bounds(1, Some(1));
        constraint.groups[0].cardinality = bounds(1, Some(1));
    }
    result
}

fn evolution_cost(action: Action, id: u64, source: &AbilitySource) -> CostStep {
    let Node::Instruction { mut instruction } = instruction(action, id, "BP06-110").node else {
        panic!("instruction");
    };
    instruction.context = InstructionContext::Cost;
    instruction.source = source.clone();
    CostStep {
        instruction,
        materials: None,
    }
}

fn evolution() -> Located<Ability> {
    let source = keyword(KeywordId::Evolution, 1);
    let selected = Binding::new(20, ScopeId(10));
    let element = Binding::new(1, ScopeId(11));
    let mut evolve = instruction(
        Action::Evolve {
            subject: ObjectOperand::SourceHolder {},
            form: EvolutionCardRef {
                object: ObjectOperand::Bound { binding: element },
                face: FaceId("base".into()),
            },
            mode: EvolutionMode::Normal,
            link: Binding::new(2, ScopeId(11)),
        },
        22,
        "BP06-110",
    );
    if let Node::Instruction { instruction } = &mut evolve.node {
        instruction.source = source.clone();
    }
    let mut ability = card(
        body(
            10,
            vec![control(Control::EachObject {
                input: Selector::Bound {
                    reference: selected,
                },
                binding: element,
                body: body(11, vec![evolve]),
                order: None,
            })],
        ),
        "BP06-110",
    )
    .node
    .abilities
    .remove(0);
    ability.node.id = AbilityId(2);
    ability.node.kind = AbilityKind::Activated;
    ability.node.play = Some(PlaySpec {
        captures: vec![],
        parameters: vec![face()],
        costs: CostSpec::Ordered {
            steps: vec![
                evolution_cost(
                    Action::Reveal {
                        objects: Selector::Bound {
                            reference: selected,
                        },
                    },
                    20,
                    &source,
                ),
                evolution_cost(
                    Action::Resource {
                        player: PlayerRef::Controller {},
                        resource: Resource::PpCurrent,
                        operation: ResourceOperation::Pay,
                        amount: Expr::Literal(Int(1)),
                    },
                    21,
                    &source,
                ),
            ],
        },
    });
    ability.node.source = source;
    ability
}

fn mitra() -> CardProgram {
    CardProgram {
        abilities: vec![evolution(), fanfare()],
    }
}

fn invalid(card: CardProgram) {
    WellFormedCardProgram::check(card).unwrap_err();
}

fn play(card: &mut CardProgram) -> &mut PlaySpec {
    card.abilities[0].node.play.as_mut().unwrap()
}

fn evolve_instruction(card: &mut CardProgram) -> &mut Instruction {
    let Node::Control {
        control: Control::EachObject { body, .. },
    } = &mut card.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("projection");
    };
    let Node::Instruction { instruction } = &mut body.steps[0].node else {
        panic!("evolve");
    };
    instruction
}

#[test]
fn bp06_110_whole_card_checks_fanfare_and_printed_evolution() {
    let card = mitra();
    assert_eq!(
        WellFormedCardProgram::check(card.clone()).unwrap().card(),
        &card
    );
    invalid(CardProgram {
        abilities: vec![{
            let mut ordinary = evolution();
            ordinary.node.source.origin = ability_source().origin;
            ordinary
        }],
    });
    let mut invalid_event = card.clone();
    if let TriggerCondition::Event {
        pattern: EventPattern::EnterField { predicate, .. },
    } = &mut invalid_event.abilities[1].node.trigger.as_mut().unwrap().on
        && let Expr::Compute(predicate) = predicate
        && let Predicate::ObjectEquals { left, .. } = predicate.as_mut()
    {
        *left = Expr::Frozen(Box::new(ObjectFrozen::EnterFieldObject {
            event: Binding::new(91, ScopeId(1)),
        }));
    }
    invalid(invalid_event);
    let mut all_entries = card.clone();
    if let TriggerCondition::Event {
        pattern: EventPattern::EnterField { predicate, .. },
    } = &mut all_entries.abilities[1].node.trigger.as_mut().unwrap().on
    {
        *predicate = Expr::Literal(Bool(true));
    }
    invalid(all_entries);
    for mode in [EvolutionMode::Super, EvolutionMode::Effect] {
        let mut wrong = card.clone();
        if let Action::Evolve { mode: actual, .. } = &mut evolve_instruction(&mut wrong).action {
            *actual = mode;
        }
        invalid(wrong);
    }
    let mut subject = card;
    if let Action::Evolve { subject, form, .. } = &mut evolve_instruction(&mut subject).action {
        *subject = form.object.clone();
    }
    invalid(subject);
}

#[test]
fn evolution_face_is_exactly_one_private_same_name_card_from_own_evolve_deck() {
    let changes: [fn(&mut Decision); 5] = [
        |face| face.visibility = Visibility::Public {},
        |face| {
            if let ChoiceDomain::Objects { constraint, .. } = &mut face.domain {
                constraint.cardinality = bounds(0, Some(1));
            }
        },
        |face| {
            if let ChoiceDomain::Objects { constraint, .. } = &mut face.domain
                && let Selector::Zone { players, .. } = &mut constraint.groups[0].selector
            {
                players.relative_to = PlayerRef::Owner {};
            }
        },
        |face| {
            if let ChoiceDomain::Objects { constraint, .. } = &mut face.domain
                && let Selector::Zone { predicate, .. } = &mut constraint.groups[0].selector
            {
                **predicate = Expr::Literal(Bool(true));
            }
        },
        |face| {
            if let DecisionKind::PlayParameter { slot, .. } = &mut face.kind {
                *slot = PlaySlot::Targets;
            }
        },
    ];
    for change in changes {
        let mut card = mitra();
        change(&mut play(&mut card).parameters[0]);
        invalid(card);
    }
}

#[test]
fn evolution_costs_reveal_the_selected_card_then_pay_printed_pp() {
    let changes: [fn(&mut Vec<CostStep>); 6] = [
        |steps| {
            steps.remove(0);
        },
        |steps| steps.swap(0, 1),
        |steps| {
            steps[0].instruction.action = Action::Reveal {
                objects: Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
            }
        },
        |steps| {
            if let Action::Resource { resource, .. } = &mut steps[1].instruction.action {
                *resource = Resource::Ep;
            }
        },
        |steps| steps[0].instruction.context = InstructionContext::Effect,
        |steps| steps[0].materials = Some(Binding::new(90, ScopeId(10))),
    ];
    for change in changes {
        let mut card = mitra();
        if let CostSpec::Ordered { steps } = &mut play(&mut card).costs {
            change(steps);
        }
        invalid(card);
    }
    let mut alternative = mitra();
    play(&mut alternative).costs = CostSpec::Alternative {
        alternatives: vec![play(&mut mitra()).costs.clone()],
    };
    invalid(alternative);
}

#[test]
fn self_entry_comparison_retains_its_bound_event_in_body_and_capture() {
    let event = Binding::new(90, ScopeId(1));
    let self_entry = Expr::Compute(Box::new(Predicate::ObjectEquals {
        left: Expr::Frozen(Box::new(ObjectFrozen::EnterFieldObject { event })),
        right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
    }));
    let mut printed = fanfare();
    printed.node.source = ability_source();
    printed.node.body = Some(body(1, vec![draw(5, Expr::Literal(Int(1)))]));
    let reason = |ability: Located<Ability>| {
        WellFormedCardProgram::check(CardProgram {
            abilities: vec![ability],
        })
        .map(|_| ())
        .map_err(|error| error.reason)
    };
    assert_eq!(reason(printed.clone()), Ok(()));
    let mut in_body = printed.clone();
    in_body.node.body = Some(body(
        1,
        vec![control(Control::If {
            condition: self_entry.clone(),
            then_body: body(2, vec![]),
            else_body: None,
        })],
    ));
    assert_eq!(reason(in_body), Ok(()));
    let mut at_event = printed;
    at_event.node.event_captures.push(Capture::Boolean {
        at: CaptureTime::AtEvent {
            event: EventContext::Trigger {},
            policy: EventSnapshotPolicy::AfterEntry,
        },
        value: self_entry,
        into: CaptureRef {
            binding: Binding::new(91, ScopeId(1)),
        },
    });
    assert_eq!(reason(at_event), Ok(()));
}

#[test]
fn evolve_is_rejected_outside_the_printed_evolution_entry() {
    let mut outside = evolution();
    outside.node.source = ability_source();
    outside.node.play = Some(PlaySpec {
        captures: vec![],
        parameters: vec![],
        costs: CostSpec::Ordered { steps: vec![] },
    });
    if let Some(body) = &mut outside.node.body
        && let Node::Control {
            control:
                Control::EachObject {
                    input,
                    body: projected,
                    ..
                },
        } = &mut body.steps[0].node
    {
        *input = Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        };
        if let Node::Instruction { instruction } = &mut projected.steps[0].node {
            instruction.source = ability_source();
        }
    }
    let error = WellFormedCardProgram::check(CardProgram {
        abilities: vec![outside],
    })
    .unwrap_err();
    assert_eq!(error.reason, WellFormedReason::UnsupportedShape);
    assert_eq!(
        error.ir_path,
        "/abilities/0/body/steps/0/control/steps/0/instruction/action"
    );
}
