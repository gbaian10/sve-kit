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
