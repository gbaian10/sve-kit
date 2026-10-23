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

#[test]
fn activated_limit_requires_play_committed() {
    let mut card = playable(vec![]);
    card.abilities[0].node.kind = AbilityKind::Activated;
    let limit = LimitSpec::activated_per_turn(Nat(1));
    play_mut(&mut card).limit = Some(limit);
    accept_card(card.clone());
    play_mut(&mut card).limit = Some(LimitSpec {
        consume_at: ConsumptionPoint::PendingAdmission,
        ..limit
    });
    reject_card(card, WellFormedReason::UnsupportedShape);
}

#[test]
fn mode_exclusion_requires_one_exact_play_choice() {
    let mut node = literal_modes(1, bounds(1, Some(1)), 2);
    let Node::Control {
        control: Control::Mode { chosen, .. },
    } = &mut node.node
    else {
        panic!("mode");
    };
    let ChoiceDomain::Modes { exclude_used, .. } = &mut chosen.domain else {
        panic!("modes");
    };
    *exclude_used = Some(ModeExclusion::CurrentTurnAbilityInstance);
    accept_card(playable(vec![node.clone()]));
    let mut alternative = node.clone();
    let Node::Control {
        control:
            Control::Mode {
                chosen: alternative_choice,
                choice_kind,
                ..
            },
    } = &mut alternative.node
    else {
        panic!("mode");
    };
    *choice_kind = ModeChoiceKind::ResolutionAlternative;
    alternative_choice.kind = ModeDecisionKind::ResolutionChoice {
        cardinality: bounds(1, Some(1)),
    };
    reject(body(1, vec![alternative]), WellFormedReason::InvalidModes);
    let Node::Control {
        control:
            Control::Mode {
                count,
                chosen: upto_choice,
                ..
            },
    } = &mut node.node
    else {
        panic!("mode");
    };
    *count = Expr::Literal(Nat(2));
    let ChoiceDomain::Modes { cardinality, .. } = &mut upto_choice.domain else {
        panic!("modes");
    };
    *cardinality = bounds(1, Some(2));
    reject_card(playable(vec![node]), WellFormedReason::InvalidModes);
}
