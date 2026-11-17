use crate::EngineFailure;
use crate::game::internal::player_stages::tests::{execution, fixture};
use crate::ir::{Int, PlayerId};

#[test]
fn negative_pp_payments_are_invalid_without_changing_any_authority() {
    for seat in [1, 2] {
        for amount in [-1_i64, i64::MIN] {
            let mut game = fixture();
            let mut execution = execution(&mut game);
            let board = format!("{game:?}");
            let state = format!("{execution:?}");
            let result = game.internal_pay_pp(PlayerId { seat }, Int(amount), &mut execution);
            assert!(
                matches!(result, Err(EngineFailure::Invalid(_))),
                "negative PP must be invalid: {result:?}"
            );
            assert_eq!(format!("{game:?}"), board);
            assert_eq!(format!("{execution:?}"), state);
        }
    }
}
