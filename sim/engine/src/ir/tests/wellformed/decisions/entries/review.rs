use super::*;

#[test]
fn play_targets_are_public_and_never_private_hand_cards() {
    let mut card = playable(vec![]);
    let mut target = play_target();
    target.visibility = Visibility::Only {
        players: controller_set(),
    };
    play_mut(&mut card).parameters.push(target);
    reject_card(card.clone(), WellFormedReason::UnsupportedShape);
    let mut hand = chosen_objects(1, 1, zone(PlayerRef::Controller {}, ZoneKind::Hand, 10));
    hand.kind = play_target().kind;
    play_mut(&mut card).parameters = vec![hand];
    reject_card(card.clone(), WellFormedReason::UnsupportedShape);
    play_mut(&mut card).parameters[0].visibility = Visibility::Only {
        players: controller_set(),
    };
    reject_card(card, WellFormedReason::UnsupportedShape);
}

#[test]
fn handoff_requires_exactly_one_opponent() {
    let mut card = playable(vec![]);
    let mut target = play_target();
    if let DecisionKind::PlayParameter { input, .. } = &mut target.kind {
        *input = InputMode::Handoff;
    }
    for cardinality in [bounds(0, Some(1)), bounds(1, Some(2))] {
        target.actor = PlayerRef::RequireOne {
            players: Box::new(PlayerSet {
                relation: PlayerRelation::Opponents,
                cardinality,
                ..controller_set()
            }),
        };
        play_mut(&mut card).parameters = vec![target.clone()];
        reject_card(card.clone(), WellFormedReason::DecisionTiming);
    }
}

fn information(object: ObjectView, view: CharacteristicView, stats: bool) -> Expr<Int> {
    Expr::Compute(Box::new(IntOperation::InformationOrZero {
        information: if stats {
            NumberRead::Stats {
                object,
                property: Stat::Attack,
                view,
            }
        } else {
            NumberRead::Cost { object, view }
        },
    }))
}

#[test]
fn information_reads_require_supported_characteristics() {
    for stats in [false, true] {
        for view in [CharacteristicView::Original, CharacteristicView::Effective] {
            accept(body(
                1,
                vec![draw(
                    1,
                    information(
                        ObjectView::Current {
                            object: Box::new(ObjectOperand::SourceHolder {}),
                        },
                        view,
                        stats,
                    ),
                )],
            ));
        }
        for view in [
            CharacteristicView::LastKnown,
            CharacteristicView::ProspectiveEntry,
        ] {
            reject(
                body(
                    1,
                    vec![draw(
                        1,
                        information(
                            ObjectView::Current {
                                object: Box::new(ObjectOperand::SourceHolder {}),
                            },
                            view,
                            stats,
                        ),
                    )],
                ),
                WellFormedReason::UnsupportedShape,
            );
        }
    }
}

fn event_integer(value: Expr<Int>) -> CardProgram {
    let mut card = triggered();
    let ability = &mut card.abilities[0].node;
    ability.body = Some(body(
        1,
        vec![draw(
            2,
            Expr::Captured(CaptureRef {
                binding: Binding::new(2, ScopeId(1)),
            }),
        )],
    ));
    ability.event_captures = vec![Capture::Integer {
        at: CaptureTime::AtEvent {
            event: EventContext::Trigger {},
            policy: EventSnapshotPolicy::BeforeExit,
        },
        value,
        into: CaptureRef {
            binding: Binding::new(2, ScopeId(1)),
        },
    }];
    card
}

#[test]
fn event_capture_reads_only_an_explicit_matching_object_snapshot() {
    let snapshot = ObjectView::EventSnapshot {
        object: Box::new(ObjectOperand::SourceHolder {}),
        event: EventContext::Trigger {},
        policy: EventSnapshotPolicy::BeforeExit,
    };
    accept_card(event_integer(information(
        snapshot.clone(),
        CharacteristicView::Effective,
        true,
    )));
    reject(
        body(
            1,
            vec![draw(
                1,
                information(snapshot, CharacteristicView::Effective, true),
            )],
        ),
        WellFormedReason::UnsupportedShape,
    );
    for object in [
        ObjectView::Current {
            object: Box::new(ObjectOperand::SourceHolder {}),
        },
        ObjectView::EventSnapshot {
            object: Box::new(ObjectOperand::SourceHolder {}),
            event: EventContext::Trigger {},
            policy: EventSnapshotPolicy::AfterEntry,
        },
    ] {
        reject_card(
            event_integer(information(object, CharacteristicView::Effective, false)),
            WellFormedReason::UnsupportedShape,
        );
    }
    reject_card(
        event_integer(Expr::Live(Box::new(IntRead::PlayerResource {
            player: PlayerRef::Controller {},
            property: PlayerResource::Life,
        }))),
        WellFormedReason::UnsupportedShape,
    );
}

#[test]
fn bp07_088_counter_and_other_current_state_need_event_snapshot_reads() {
    for value in [
        Expr::Live(Box::new(NatRead::Counter {
            holder: Holder::Object {
                object: ObjectOperand::SourceHolder {},
            },
            kind: CounterKind("prayer".to_owned()),
        })),
        Expr::Compute(Box::new(NatOperation::Count {
            objects: zone(PlayerRef::Controller {}, ZoneKind::Field, 10),
        })),
    ] {
        let mut card = triggered();
        if let Capture::Natural { value: into, .. } = &mut card.abilities[0].node.event_captures[0]
        {
            *into = value;
        }
        reject_card(card, WellFormedReason::UnsupportedShape);
    }
    let mut card = triggered();
    card.abilities[0].node.event_captures = vec![Capture::Boolean {
        at: CaptureTime::AtEvent {
            event: EventContext::Trigger {},
            policy: EventSnapshotPolicy::BeforeExit,
        },
        value: Expr::Live(Box::new(BoolRead::OwnTurn {
            player: PlayerRef::Controller {},
        })),
        into: CaptureRef {
            binding: Binding::new(3, ScopeId(1)),
        },
    }];
    reject_card(card, WellFormedReason::UnsupportedShape);
}

#[test]
fn quick_windows_reject_the_turn_player_even_with_matching_domain_and_visibility() {
    for window in [QuickWindow::Attack, QuickWindow::EndPhase] {
        for actor in [PlayerRef::Active {}, PlayerRef::Controller {}] {
            let decision = Decision {
                id: DecisionId(1),
                actor: actor.clone(),
                domain: ChoiceDomain::QuickActionOrPass {
                    player: actor.clone(),
                },
                kind: DecisionKind::QuickWindow { window },
                visibility: Visibility::Only {
                    players: PlayerSet {
                        relative_to: actor,
                        ..controller_set()
                    },
                },
                captures: vec![],
            };
            assert_eq!(
                WellFormedProcedure::check(Procedure::Quick { window, decision })
                    .unwrap_err()
                    .reason,
                WellFormedReason::DecisionTiming
            );
        }
    }
}

#[test]
fn compound_answers_do_not_smuggle_unchecked_order_domains() {
    let mut choice = play_target();
    choice.kind = DecisionKind::ResolutionChoice {
        may: Declinable::Required,
        cardinality: bounds(1, Some(1)),
        precondition: None,
    };
    choice.domain = ChoiceDomain::Compound {
        domains: vec![ChoiceDomain::Order {
            objects: zone(PlayerRef::Controller {}, ZoneKind::Field, 10),
            binding: Binding::new(1, ScopeId(1)),
        }],
    };
    reject(
        body(1, vec![decision(choice)]),
        WellFormedReason::UnsupportedShape,
    );
}

fn literal_modes(count_value: u64, cardinality: Cardinality, number: usize) -> Located<Node> {
    let mut node = mode_group(SpellCountTime::AtPlay, false);
    if let Node::Control {
        control:
            Control::Mode {
                count,
                chosen,
                modes,
                ..
            },
    } = &mut node.node
    {
        *count = Expr::Literal(Nat(count_value));
        modes.truncate(number);
        if let ChoiceDomain::Modes {
            cardinality: bounds,
            modes: available,
            ..
        } = &mut chosen.domain
        {
            *bounds = cardinality;
            available.truncate(number);
        }
    }
    node
}

#[test]
fn literal_mode_counts_clamp_and_only_any_number_allows_zero() {
    for minimum in [1, 2] {
        accept_card(playable(vec![literal_modes(
            3,
            bounds(minimum, Some(2)),
            2,
        )]));
    }
    accept_card(playable(vec![literal_modes(3, bounds(0, Some(3)), 3)]));
    accept_card(playable(vec![literal_modes(0, bounds(0, Some(0)), 3)]));
    for node in [
        literal_modes(2, bounds(0, Some(2)), 3),
        literal_modes(3, bounds(3, Some(3)), 2),
    ] {
        reject_card(playable(vec![node]), WellFormedReason::InvalidModes);
    }
    let Node::Control {
        control: Control::Mode { chosen, .. },
    } = literal_modes(2, bounds(0, Some(2)), 3).node
    else {
        panic!("mode")
    };
    assert!(!chosen.accepts_answer(Nat(2), &[]));
}

#[test]
fn resolution_alternative_is_exactly_one_and_has_no_play_parameters() {
    let mut node = literal_modes(1, bounds(1, Some(1)), 2);
    if let Node::Control {
        control:
            Control::Mode {
                chosen,
                choice_kind,
                ..
            },
    } = &mut node.node
    {
        *choice_kind = ModeChoiceKind::ResolutionAlternative;
        chosen.kind = ModeDecisionKind::ResolutionChoice {
            cardinality: bounds(1, Some(1)),
        };
    }
    accept(body(1, vec![node.clone()]));
    let mut wrong_count = node.clone();
    if let Node::Control {
        control: Control::Mode { count, .. },
    } = &mut wrong_count.node
    {
        *count = Expr::Literal(Nat(2));
    }
    reject(body(1, vec![wrong_count]), WellFormedReason::DecisionTiming);
    if let Node::Control {
        control: Control::Mode { modes, .. },
    } = &mut node.node
    {
        modes[0].parameters.push(play_target());
    }
    reject(body(1, vec![node]), WellFormedReason::DecisionTiming);
}

fn static_card() -> CardProgram {
    let mut card = playable(vec![]);
    let ability = &mut card.abilities[0].node;
    ability.kind = AbilityKind::Static;
    ability.play = None;
    ability.body = None;
    ability.declarations = vec![continuous(
        CharacteristicPatch::Numeric {
            stat: Stat::Attack,
            change: NumericChange::Add {
                amount: Expr::Literal(Int(1)),
            },
        },
        ContinuousLayer::Numeric,
    )];
    card
}

#[test]
fn static_effects_follow_the_ability_lifetime_and_source() {
    accept_card(static_card());
    for end in [
        DurationEnd::EndResolution {},
        DurationEnd::CurrentTurnEnd {},
    ] {
        let mut card = static_card();
        if let Effect::Continuous { duration, .. } = &mut card.abilities[0].node.declarations[0] {
            duration.end = end;
        }
        reject_card(card, WellFormedReason::UnsupportedShape);
    }
    let mut wrong_zone = static_card();
    if let Effect::Continuous { active_zones, .. } =
        &mut wrong_zone.abilities[0].node.declarations[0]
    {
        *active_zones = vec![ZoneKind::Ex];
    }
    reject_card(wrong_zone, WellFormedReason::InvalidEntry);
    let mut wrong_source = static_card();
    if let Effect::Continuous { source, .. } = &mut wrong_source.abilities[0].node.declarations[0] {
        source.controller = PlayerRef::Active {};
    }
    reject_card(wrong_source, WellFormedReason::InvalidEntry);
    let mut wrong_body = playable(vec![draw(1, Expr::Literal(Int(1)))]);
    if let Node::Instruction { instruction } =
        &mut wrong_body.abilities[0].node.body.as_mut().unwrap().steps[0].node
    {
        instruction.source.controller = PlayerRef::Active {};
    }
    reject_card(wrong_body, WellFormedReason::InvalidEntry);
}
