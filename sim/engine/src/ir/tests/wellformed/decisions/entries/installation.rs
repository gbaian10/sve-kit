use super::*;

fn affected() -> Selector {
    Selector::Bound {
        reference: Binding::new(1, ScopeId(2)),
    }
}

fn lasting() -> Duration {
    Duration {
        end: DurationEnd::NoTemporalExpiry {},
        ..turn()
    }
}

fn buff() -> Effect {
    Effect::Continuous {
        id: EffectDefinitionId(1),
        source: ability_source(),
        target: EffectTarget::Dynamic {
            selector: affected(),
        },
        condition: None,
        patch: CharacteristicPatch::Combined {
            patches: vec![
                CharacteristicPatch::Numeric {
                    stat: Stat::Attack,
                    change: NumericChange::Add {
                        amount: Expr::Literal(Int(1)),
                    },
                },
                CharacteristicPatch::Numeric {
                    stat: Stat::Life,
                    change: NumericChange::Add {
                        amount: Expr::Literal(Int(1)),
                    },
                },
            ],
        },
        layer: ContinuousLayer::Numeric,
        timestamp: TimestampBasis::ProperPlay {
            execution: ExecutionRef::Current {},
        },
        active_zones: vec![],
        duration: lasting(),
    }
}

fn discount() -> Effect {
    Effect::CostModifier {
        kind: CostKind::Play,
        subjects: affected(),
        change: CostChange::Delta {
            amount: Expr::Literal(Int(-1)),
        },
        duration: lasting(),
        usage: None,
    }
}

fn permission(banish: bool) -> Effect {
    let objects = affected();
    let cause = CausePattern::Effect {
        source: EffectSource::Any {},
    };
    Effect::Permission {
        subject: PermissionSubject::Objects {
            objects: objects.clone(),
        },
        action: if banish {
            ActionPattern::Banish { objects, cause }
        } else {
            ActionPattern::Destroy { objects, cause }
        },
        polarity: Polarity::Forbid,
        condition: Expr::Literal(Bool(true)),
        duration: lasting(),
        rule: RuleId(1),
    }
}

fn installed(effect: Effect, duration: Duration) -> Located<Node> {
    instruction(
        Action::InstallEffect {
            subjects: InstallationSubjects::Objects {
                objects: Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
                affected: Binding::new(1, ScopeId(2)),
            },
            capture: InstallationTime::InstructionStart,
            effect: Box::new(effect),
            duration,
        },
        1,
        "synthetic",
    )
}

fn installed_for(mut effect: Effect, duration: Duration) -> Located<Node> {
    if let Effect::Continuous {
        duration: inner, ..
    }
    | Effect::Permission {
        duration: inner, ..
    }
    | Effect::CostModifier {
        kind: CostKind::Play,
        duration: inner,
        ..
    } = &mut effect
    {
        *inner = duration.clone();
    }
    installed(effect, duration)
}

fn closed(effect: Effect, duration: Duration) -> Program {
    body(1, vec![installed_for(effect, duration)])
}

fn installed_card(effect: Effect) -> CardProgram {
    playable(vec![installed(effect, lasting())])
}

fn reject_effect(card: CardProgram, reason: WellFormedReason) {
    let error = WellFormedCardProgram::check(card).expect_err("invalid card");
    assert_eq!(error.reason, reason, "{error}");
    assert!(
        error.ir_path.ends_with("/instruction/action/effect"),
        "{error}"
    );
}

#[test]
fn installed_numeric_targets_layers_sources_and_timestamps_are_checked() {
    for change in 0_u8..8 {
        let mut effect = buff();
        let Effect::Continuous {
            source,
            target,
            layer,
            timestamp,
            duration,
            ..
        } = &mut effect
        else {
            panic!("continuous")
        };
        match change {
            0 => {
                *target = EffectTarget::Dynamic {
                    selector: Selector::Objects { objects: vec![] },
                };
            }
            1 => *layer = ContinuousLayer::Ability,
            2 => {
                *timestamp = TimestampBasis::ZoneEntry {
                    object: ObjectOperand::SourceHolder {},
                }
            }
            3 => {
                *timestamp = TimestampBasis::ProperPlay {
                    execution: ExecutionRef::Bound {
                        binding: Binding::new(99, ScopeId(1)),
                    },
                };
            }
            4 => duration.end = DurationEnd::EndResolution {},
            5 => source.controller = PlayerRef::Owner {},
            6 => {
                *target = EffectTarget::Captured {
                    objects: CaptureRef {
                        binding: Binding::new(1, ScopeId(2)),
                    },
                };
            }
            _ => *layer = ContinuousLayer::NonNumeric,
        }
        reject_card(
            installed_card(effect),
            match change {
                3 => WellFormedReason::UnavailableBinding,
                5 => WellFormedReason::InvalidEntry,
                _ => WellFormedReason::UnsupportedShape,
            },
        );
    }
}

#[test]
fn bp02_089_destroy_seed_and_a_synthetic_banish_install_on_a_fixed_set() {
    // BP02-089 is protected only from ability destruction and can still be banished (Q315).
    for banish in [false, true] {
        accept_card(installed_card(permission(banish)));
        let mut wrong_target = permission(banish);
        if let Effect::Permission {
            action: ActionPattern::Destroy { objects, .. } | ActionPattern::Banish { objects, .. },
            ..
        } = &mut wrong_target
        {
            *objects = Selector::Objects { objects: vec![] };
        }
        reject_effect(
            installed_card(wrong_target),
            WellFormedReason::UnsupportedShape,
        );
    }
    let mut missing_condition = permission(false);
    if let Effect::Permission { condition, .. } = &mut missing_condition {
        *condition = Expr::Bound(Binding::new(99, ScopeId(1)));
    }
    reject_card(
        installed_card(missing_condition),
        WellFormedReason::UnavailableBinding,
    );
}

#[test]
fn duration_rejects_missing_anchor_usage_and_interval_inputs() {
    let missing_player = PlayerRef::Bound {
        binding: Binding::new(99, ScopeId(1)),
    };
    let limited = |scope| Duration {
        usage: Some(UseLimit {
            count: Nat(1),
            scope,
        }),
        ..lasting()
    };
    for (effect, duration) in [
        (
            buff(),
            Duration {
                anchor: TimeAnchor::Bound {
                    binding: Binding::new(99, ScopeId(1)),
                },
                ..lasting()
            },
        ),
        (
            discount(),
            limited(UsageScope::PerObject {
                object: ObjectOperand::Bound {
                    binding: Binding::new(99, ScopeId(1)),
                },
            }),
        ),
        (
            discount(),
            limited(UsageScope::SharedPlayer {
                player: missing_player.clone(),
                effect: EffectReference::CurrentInstallation {},
            }),
        ),
        (
            discount(),
            limited(UsageScope::SharedPlayer {
                player: PlayerRef::Controller {},
                effect: EffectReference::Bound {
                    binding: Binding::new(99, ScopeId(1)),
                },
            }),
        ),
        (
            buff(),
            Duration {
                intervals: vec![Interval::DuringNextTurn {
                    player: missing_player.clone(),
                }],
                ..lasting()
            },
        ),
        (
            buff(),
            Duration {
                intervals: vec![Interval::DuringNextPhase {
                    player: missing_player,
                    phase: Phase::Main,
                }],
                ..lasting()
            },
        ),
    ] {
        reject(
            closed(effect, duration),
            WellFormedReason::UnavailableBinding,
        );
    }
}

#[test]
fn installation_scope_does_not_escape_and_duplicate_effects_are_rejected() {
    let first = installed(buff(), lasting());
    reject(
        body(
            1,
            vec![
                first.clone(),
                instruction(
                    Action::Reveal {
                        objects: affected(),
                    },
                    2,
                    "synthetic",
                ),
            ],
        ),
        WellFormedReason::UnavailableBinding,
    );
    let mut second = installed(buff(), lasting());
    if let Node::Instruction { instruction } = &mut second.node {
        instruction.id = InstructionId(2);
        instruction.receipt = ReceiptId(2);
        if let Action::InstallEffect {
            subjects, effect, ..
        } = &mut instruction.action
        {
            let binding = Binding::new(1, ScopeId(3));
            *subjects = InstallationSubjects::Objects {
                objects: Selector::Objects { objects: vec![] },
                affected: binding,
            };
            if let Effect::Continuous { target, .. } = effect.as_mut() {
                *target = EffectTarget::Dynamic {
                    selector: Selector::Bound { reference: binding },
                };
            }
        }
    }
    let error = reject(body(1, vec![first, second]), WellFormedReason::InvalidEntry);
    assert!(
        error
            .ir_path
            .ends_with("/steps/1/instruction/action/effect"),
        "{}",
        error.ir_path
    );
}
