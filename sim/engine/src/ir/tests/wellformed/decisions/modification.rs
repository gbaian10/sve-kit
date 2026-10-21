use super::*;

fn modify(patch: CharacteristicPatch, card_no: &str) -> Located<Node> {
    instruction(
        Action::Modify {
            subjects: Selector::Bound {
                reference: Binding::new(1, ScopeId(1)),
            },
            patch,
            expires: Duration {
                end: DurationEnd::NoTemporalExpiry {},
                ..turn()
            },
            source: EffectReference::CurrentInstallation {},
        },
        1,
        card_no,
    )
}

fn numeric(set: bool, amount: &Expr<Int>) -> CharacteristicPatch {
    CharacteristicPatch::Combined {
        patches: [Stat::Attack, Stat::Life]
            .into_iter()
            .map(|stat| CharacteristicPatch::Numeric {
                stat,
                change: if set {
                    NumericChange::Set {
                        amount: amount.clone(),
                    }
                } else {
                    NumericChange::Add {
                        amount: amount.clone(),
                    }
                },
            })
            .collect(),
    }
}

fn selected(step: Located<Node>) -> Program {
    body(
        1,
        vec![
            decision(chosen_objects(
                1,
                1,
                zone(PlayerRef::Controller {}, ZoneKind::Field, 10),
            )),
            step,
        ],
    )
}

#[test]
fn cp02_007_buff_bp16_049_set_and_bp05_001_kind_are_action_seeds() {
    // These fragments do not validate the cards' costs, other abilities, or lowering.
    for (patch, card_no) in [
        (numeric(false, &Expr::Literal(Int(1))), "CP02-007"),
        (numeric(true, &Expr::Literal(Int(1))), "BP16-049"),
        (
            CharacteristicPatch::SetKind {
                kind: CardKind::Amulet,
            },
            "BP05-001",
        ),
    ] {
        accept(selected(modify(patch, card_no)));
    }
    for amount in [-2, 0, 2] {
        accept(selected(modify(
            numeric(false, &Expr::Literal(Int(amount))),
            "synthetic",
        )));
    }
}

#[test]
fn modify_checks_inputs_and_never_accepts_ability_or_mixed_patches() {
    let valid = numeric(false, &Expr::Literal(Int(1)));
    for patch in [
        CharacteristicPatch::Combined { patches: vec![] },
        CharacteristicPatch::Combined {
            patches: vec![
                valid.clone(),
                CharacteristicPatch::SetKind {
                    kind: CardKind::Amulet,
                },
            ],
        },
        CharacteristicPatch::GrantAbility { abilities: vec![] },
        CharacteristicPatch::AddName {
            name: CardName("synthetic".into()),
        },
    ] {
        let error = reject(
            selected(modify(patch, "synthetic")),
            WellFormedReason::UnsupportedShape,
        );
        assert!(error.ir_path.contains("/patch"), "{error}");
    }
    reject(
        selected(modify(
            numeric(true, &Expr::Bound(Binding::new(99, ScopeId(1)))),
            "synthetic",
        )),
        WellFormedReason::UnavailableBinding,
    );
    reject(
        body(1, vec![modify(valid.clone(), "synthetic")]),
        WellFormedReason::UnavailableBinding,
    );
    for source in [
        EffectReference::Bound {
            binding: Binding::new(99, ScopeId(1)),
        },
        EffectReference::Bound {
            binding: Binding::new(1, ScopeId(1)),
        },
    ] {
        let mut step = modify(valid.clone(), "synthetic");
        if let Node::Instruction { instruction } = &mut step.node
            && let Action::Modify { source: into, .. } = &mut instruction.action
        {
            *into = source;
        }
        let error = WellFormedProgram::check(selected(step)).expect_err("invalid source");
        assert!(matches!(
            error.reason,
            WellFormedReason::UnavailableBinding | WellFormedReason::BindingType
        ));
        assert!(error.ir_path.ends_with("/source"), "{error}");
    }
}

#[test]
fn modify_duration_is_checked_and_usage_is_not_silently_ignored() {
    for expires in [
        Duration {
            usage: Some(UseLimit {
                count: Nat(1),
                scope: UsageScope::PerObject {
                    object: ObjectOperand::SourceHolder {},
                },
            }),
            ..turn()
        },
        Duration {
            end: DurationEnd::NextTurnEnd {
                player: PlayerRef::Bound {
                    binding: Binding::new(99, ScopeId(1)),
                },
            },
            ..turn()
        },
    ] {
        let usage = expires.usage.is_some();
        let mut step = modify(numeric(false, &Expr::Literal(Int(1))), "synthetic");
        if let Node::Instruction { instruction } = &mut step.node
            && let Action::Modify { expires: into, .. } = &mut instruction.action
        {
            *into = expires;
        }
        reject(
            selected(step),
            if usage {
                WellFormedReason::UnsupportedShape
            } else {
                WellFormedReason::UnavailableBinding
            },
        );
    }
    let step = modify(
        CharacteristicPatch::Combined { patches: vec![] },
        "synthetic",
    );
    let mut program = selected(step);
    let bad = program.steps.pop().unwrap();
    program.steps.push(control(Control::If {
        condition: Expr::Literal(Bool(false)),
        then_body: body(2, vec![bad]),
        else_body: None,
    }));
    reject(program, WellFormedReason::UnsupportedShape);
}

#[test]
fn numeric_modify_accepts_standalone_and_nested_numeric_patches() {
    let single = CharacteristicPatch::Numeric {
        stat: Stat::Attack,
        change: NumericChange::Add {
            amount: Expr::Literal(Int(1)),
        },
    };
    for patch in [
        single.clone(),
        CharacteristicPatch::Combined {
            patches: vec![single, numeric(true, &Expr::Literal(Int(1)))],
        },
    ] {
        accept(selected(modify(patch, "synthetic")));
    }
}
