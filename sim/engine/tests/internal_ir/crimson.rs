use super::*;

fn change(id: u64, amount: i64) -> Located<Node> {
    instruction(
        id,
        Action::Life {
            player: PlayerRef::Controller {},
            change: NumericChange::Add {
                amount: Expr::Literal(Int(amount)),
            },
        },
    )
}

fn conditional_draw() -> Located<Node> {
    node(Node::Control {
        control: Control::If {
            condition: Expr::Live(Box::new(BoolRead::Crimson {
                player: PlayerRef::Controller {},
            })),
            then_body: Program {
                scope: ScopeId(2),
                steps: vec![draw(3, 1)],
            },
            else_body: None,
        },
    })
}

#[test]
fn crimson_remembers_a_decrease_after_healing_but_requires_own_turn() {
    for seat in [1, 2] {
        for own_turn in [false, true] {
            let actor = if seat == 1 { "P1" } else { "P2" };
            let opponent = if seat == 1 { "P2" } else { "P1" };
            let mut initial = setup(seat, json!([{"id":"d","card":"unit-follower"}]));
            initial["turn"]["active"] = json!(if own_turn { actor } else { opponent });
            let mut game =
                Game::new(catalog(), &initial, &Value::Null, &Value::Null, "crimson").unwrap();
            let result = game
                .execute_internal(
                    &checked(vec![change(1, -1), change(2, 1), conditional_draw()]),
                    &source("synthetic"),
                    context(seat),
                )
                .unwrap();
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state[actor]["leader"]["life"], 20);
            assert_eq!(
                result
                    .facts
                    .iter()
                    .filter(|fact| matches!(fact, Fact::Draw(_)))
                    .count(),
                usize::from(own_turn)
            );
        }
    }
}

#[test]
fn crimson_does_not_carry_a_decrease_into_the_next_own_turn() {
    let mut initial = setup(
        1,
        json!([
            {"id":"d0","card":"unit-follower"},{"id":"d1","card":"unit-follower"},{"id":"d2","card":"unit-follower"}
        ]),
    );
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"other","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "crimson-reset",
    )
    .unwrap();
    game.execute_internal(
        &checked(vec![change(1, -1)]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    for _ in 0..2 {
        game.decide(&json!({"do":"end-phase"}), "end").unwrap();
        game.decide(&json!({"do":"pass"}), "quick-pass").unwrap();
    }
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P1"
    );
    let result = game
        .execute_internal(
            &checked(vec![conditional_draw()]),
            &source("synthetic"),
            context(1),
        )
        .unwrap();
    assert!(
        !result
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Draw(_)))
    );
}
