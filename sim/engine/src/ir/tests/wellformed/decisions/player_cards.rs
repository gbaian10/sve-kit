use super::*;

fn player() -> PlayerRef {
    PlayerRef::Bound {
        binding: Binding::new(1, ScopeId(2)),
    }
}
fn all_players(stages: Vec<PlayerStage>) -> Located<Node> {
    control(Control::EachPlayer {
        players: PlayerSet {
            relation: PlayerRelation::All,
            cardinality: bounds(1, None),
            text_quantifier: TextQuantifier::All,
            ..controller_set()
        },
        player: Binding::new(1, ScopeId(2)),
        stages,
        choice_order: PlayerOrder::TurnPlayerFirst,
    })
}

fn performed_by(mut node: Located<Node>, actor: PlayerRef) -> Located<Node> {
    if let Node::Instruction { instruction } = &mut node.node {
        instruction.actor = actor;
    }
    node
}

pub(super) fn mitra_body(draw_actor: PlayerRef, discard_actor: PlayerRef) -> Located<Node> {
    let roll = Binding::new(2, ScopeId(2));
    let draw_two = |actor| {
        performed_by(
            instruction(
                Action::Draw {
                    player: player(),
                    count: Expr::Literal(Int(2)),
                    mode: StepwiseMode::Exact,
                },
                2,
                "BP06-110",
            ),
            actor,
        )
    };
    let discard_two = |actor| {
        performed_by(
            instruction(
                Action::Discard {
                    player: player(),
                    mode: DiscardMode::Random {
                        population: zone(player(), ZoneKind::Hand, 12),
                        count: Expr::Literal(Int(2)),
                    },
                },
                3,
                "BP06-110",
            ),
            actor,
        )
    };
    let fanfare = |draw: Located<Node>, discard: Located<Node>| {
        all_players(vec![
            PlayerStage {
                mode: StageMode::NonChoiceSimultaneous {},
                body: body(
                    2,
                    vec![performed_by(
                        instruction(
                            Action::Dice {
                                operation: DiceOperation::Roll {},
                                result: roll,
                            },
                            1,
                            "BP06-110",
                        ),
                        player(),
                    )],
                ),
            },
            PlayerStage {
                mode: StageMode::NonChoiceSimultaneous {},
                body: body(
                    3,
                    vec![control(Control::If {
                        condition: Expr::Compute(Box::new(Predicate::CompareNat {
                            left: Expr::Bound(roll),
                            comparison: Comparison::LessEqual,
                            right: Expr::Literal(Nat(3)),
                        })),
                        then_body: body(4, vec![draw]),
                        else_body: Some(body(5, vec![discard])),
                    })],
                ),
            },
        ])
    };
    fanfare(draw_two(draw_actor), discard_two(discard_actor))
}

#[test]
fn bp06_110_fanfare_body_keeps_both_dice_branches_for_the_same_player() {
    let roll = Binding::new(2, ScopeId(2));
    let each = mitra_body(player(), player());
    accept(body(1, vec![each.clone()]));
    reject(
        body(1, vec![each, draw(4, signed(Expr::Bound(roll)))]),
        WellFormedReason::UnavailableBinding,
    );
    for wrong in [
        mitra_body(PlayerRef::Controller {}, player()),
        mitra_body(player(), PlayerRef::Controller {}),
    ] {
        reject(body(1, vec![wrong]), WellFormedReason::UnsupportedShape);
    }
}

#[test]
fn bp13_043_specifies_before_independent_transfer_prerequisites() {
    let selected = Binding::new(2, ScopeId(2));
    let mut followers = zone(player(), ZoneKind::Field, 12);
    if let Selector::Zone {
        binding, predicate, ..
    } = &mut followers
    {
        **predicate = Expr::Compute(Box::new(Predicate::KindEquals {
            left: Expr::Live(Box::new(KindRead::Kind {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::Bound { binding: *binding }),
                },
                view: CharacteristicView::Effective,
            })),
            right: Expr::Literal(CardKind::Follower),
        }));
    }
    let mut specify = chosen_objects(2, 2, followers);
    specify.actor = player();
    if let ChoiceDomain::Objects { constraint, .. } = &mut specify.domain {
        constraint.cardinality = bounds(1, Some(1));
        constraint.groups[0].cardinality = bounds(1, Some(1));
    }
    specify.kind = DecisionKind::ResolutionChoice {
        may: Declinable::Required,
        cardinality: bounds(1, Some(1)),
        precondition: None,
    };
    let recipient = PlayerRef::RequireOne {
        players: Box::new(PlayerSet {
            relation: PlayerRelation::Opponents,
            relative_to: player(),
            ..controller_set()
        }),
    };
    let guard = Precondition {
        kind: PreconditionKind::CardTextPrerequisite,
        predicate: positive(Expr::Live(Box::new(NatRead::ZoneCapacity {
            player: recipient.clone(),
            zone: CapacityZone::Field,
            property: CapacityProperty::Free,
        }))),
        protects: InstructionId(1),
        reason: PreconditionReason::CardText {},
    };
    let mut transfer = instruction(
        Action::Transfer {
            objects: Selector::Bound {
                reference: selected,
            },
            destination_controller: recipient,
        },
        1,
        "BP13-043",
    );
    if let Node::Instruction { instruction } = &mut transfer.node {
        instruction.actor = player();
        instruction.precondition = Some(guard.clone());
    }
    let stages = |selection| {
        vec![
            PlayerStage {
                mode: StageMode::ChoiceProcedure {},
                body: body(2, vec![decision(selection)]),
            },
            PlayerStage {
                mode: StageMode::ExplicitOrder { rule: RuleId(1766) },
                body: body(3, vec![transfer.clone()]),
            },
        ]
    };
    accept(body(1, vec![all_players(stages(specify.clone()))]));
    if let DecisionKind::ResolutionChoice { precondition, .. } = &mut specify.kind {
        *precondition = Some(guard);
    }
    reject(
        body(1, vec![all_players(stages(specify))]),
        WellFormedReason::PreconditionTiming,
    );
}
