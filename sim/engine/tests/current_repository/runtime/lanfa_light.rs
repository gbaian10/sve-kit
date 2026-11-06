use super::final_light::{ranpha_entry, ranpha_modes};
use super::*;

#[test]
fn lanfa_all_printings_require_a_ub_target_refund_only_nonhand_entry_and_track_modes_per_instance() {
    let selected = selection_with_rules();
    let identity = &selected.current["CP04-075"].card_id;
    for (card, _) in selected.current.iter().filter(|(_, card)| &card.card_id == identity) {
        ranpha_entry(card, "hand");
        ranpha_entry(card, "ex");
        ranpha_modes(card);

        let mut initial = draw_position();
        initial["players"]["P1"]["construction"] = json!("title");
        initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
        initial["players"]["P1"]["leader"]["life"] = json!(6_u8);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":card}]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"observer","card":card}]);
        let mut board = Game::from_selected_effects(selected.clone(), &initial, &Value::Null, &Value::Null, "lanfa-no-target").unwrap();
        assert!(matches!(board.play_internal_card("source", PlayerId { seat: 1 }).unwrap(), CardPlay::Entered(_)));
        while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            board.resume_internal_card().unwrap();
        }
        let unplayable = union_burst_batch1::pending(&mut board, card);
        assert!(matches!(unplayable, CardPlay::Cancelled), "without a legal enemy follower the pending fanfare cannot be played");
        while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            board.resume_internal_card().unwrap();
        }
        assert!(board.internal_pending_choice().unwrap().is_none(), "the cancelled fanfare produces no observed UB");
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["pp"]["current"], 6_u8, "Q2452: the four-PP EX play does not refund PP without the required target");
        assert_eq!(after["P1"]["leader"]["life"], 6_u8);
        assert_eq!(after["P2"]["leader"]["life"], 20_u8, "Q2435: the other Lanfa does not observe an unplayed UB");
    }
}
