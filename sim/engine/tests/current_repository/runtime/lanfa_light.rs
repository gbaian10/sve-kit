use super::final_light::{aliases, checks, game, princess, ranpha_entry, ranpha_modes};
use super::*;

#[test]
fn lanfa_all_printings_require_a_ub_target_refund_only_nonhand_entry_and_track_modes_per_instance() {
    let printings = aliases("CP04-075");
    assert_eq!(printings, ["CP04-075", "CP04-SL19"]);
    for card in printings {
        ranpha_entry(&card, "hand");
        ranpha_entry(&card, "ex");
        ranpha_modes(&card);

        let mut initial = draw_position();
        princess(&mut initial);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":card}]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"observer","card":card}]);
        let mut board = game(&initial);
        assert!(matches!(board.play_internal_card("source", PlayerId { seat: 1 }).unwrap(), CardPlay::Entered(_)));
        checks(&mut board);
        let unplayable = union_burst_batch1::pending(&mut board, &card);
        assert!(matches!(unplayable, CardPlay::Cancelled), "without a legal enemy follower the pending fanfare cannot be played");
        checks(&mut board);
        assert!(board.internal_pending_choice().unwrap().is_none(), "the cancelled fanfare produces no observed UB");
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["pp"]["current"], 6_u8, "Q2452: the four-PP EX play does not refund PP without the required target");
        assert_eq!(after["P1"]["leader"]["life"], 6_u8);
        assert_eq!(after["P2"]["leader"]["life"], 20_u8, "Q2435: the other Lanfa does not observe an unplayed UB");
    }
}
