use super::*;

fn life(id: u64, change: NumericChange) -> Located<Node> {
    instruction(
        id,
        Action::Life {
            player: PlayerRef::Controller {},
            change,
        },
    )
}

fn add(id: u64, amount: i64) -> Located<Node> {
    life(
        id,
        NumericChange::Add {
            amount: Expr::Literal(Int(amount)),
        },
    )
}

fn set(id: u64, amount: i64) -> Located<Node> {
    life(
        id,
        NumericChange::Set {
            amount: Expr::Literal(Int(amount)),
        },
    )
}

#[test]
fn bp01_013_life_fragment_heals_three_then_draws_without_dealing_damage() {
    for seat in [1, 2] {
        let mut game = game(seat, 2);
        let result = game
            .execute_internal(
                &checked(vec![add(1, 3), draw(2, 1)]),
                &source("BP01-013"),
                context(seat),
            )
            .unwrap();
        let player = PlayerId { seat };
        assert_eq!(
            result.receipts[&ReceiptId(1)],
            InstructionOutcome::Life(LifeChangeEvent {
                subject: player,
                before: Int(20),
                after: Int(23),
                delta: Int(3),
            })
        );
        assert!(
            matches!(result.facts.as_slice(), [Fact::LifeChange(event), Fact::Draw(_)]
            if event.subject == player && event.delta == Int(3))
        );
        let actor = if seat == 1 { "P1" } else { "P2" };
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[actor]["leader"]["life"], 23);
        assert_eq!(state[actor]["hand"], json!(["s", "d0"]));
    }
}

#[test]
fn bp01_105_negative_life_fragment_and_bp05_092_set_fragment_keep_actual_delta() {
    for seat in [1, 2] {
        let mut game = game(seat, 0);
        let decreased = game
            .execute_internal(
                &checked(vec![add(1, -3)]),
                &source("BP01-105"),
                context(seat),
            )
            .unwrap();
        assert!(
            matches!(decreased.receipts[&ReceiptId(1)], InstructionOutcome::Life(event)
            if event.before == Int(20) && event.after == Int(17) && event.delta == Int(-3))
        );
        let restored = game
            .execute_internal(
                &checked(vec![set(1, 1)]),
                &source("BP05-092"),
                context(seat),
            )
            .unwrap();
        assert!(
            matches!(restored.receipts[&ReceiptId(1)], InstructionOutcome::Life(event)
            if event.before == Int(17) && event.after == Int(1) && event.delta == Int(-16))
        );
    }
}

#[test]
fn synthetic_life_below_zero_is_not_clamped_and_loses_at_check_timing() {
    let mut game = game(1, 0);
    let result = game
        .execute_internal(
            &checked(vec![add(1, -23)]),
            &source("synthetic"),
            context(1),
        )
        .unwrap();
    assert!(
        matches!(result.receipts[&ReceiptId(1)], InstructionOutcome::Life(event)
        if event.after == Int(-3) && event.delta == Int(-23))
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], -3);
    assert_eq!(state["game"]["ended"], true);
}

#[test]
fn synthetic_zero_and_unchanged_set_keep_receipts_but_emit_no_life_fact() {
    let mut game = game(1, 0);
    let result = game
        .execute_internal(
            &checked(vec![add(1, 0), set(2, 20)]),
            &source("synthetic"),
            context(1),
        )
        .unwrap();
    assert_eq!(result.facts, vec![]);
    for id in [1, 2] {
        assert!(
            matches!(result.receipts[&ReceiptId(id)], InstructionOutcome::Life(event)
            if event.before == Int(20) && event.after == Int(20) && event.delta == Int(0))
        );
    }
    let result = game
        .execute_internal(&checked(vec![set(1, 25)]), &source("synthetic"), context(1))
        .unwrap();
    assert!(matches!(result.facts.as_slice(), [Fact::LifeChange(event)] if event.delta == Int(5)));
}

#[test]
fn synthetic_life_overflow_restores_an_earlier_draw_and_life_change() {
    for change in [add(3, i64::MAX), set(3, i64::MIN)] {
        let mut game = game(1, 2);
        let before = exact_authority(&game);
        game.execute_internal(
            &checked(vec![draw(1, 1), add(2, 3), change]),
            &source("synthetic"),
            context(1),
        )
        .unwrap_err();
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn synthetic_wrong_actor_or_stale_source_rejects_before_life_mutation() {
    for wrong_actor in [true, false] {
        let mut game = game(1, 0);
        let mut context = context(1);
        if wrong_actor {
            context.controller = PlayerId { seat: 2 };
        } else {
            context.generation = Generation(1);
        }
        let before = exact_authority(&game);
        game.execute_internal(&checked(vec![add(1, 3)]), &source("BP01-013"), context)
            .unwrap_err();
        assert_eq!(exact_authority(&game), before);
    }
}
