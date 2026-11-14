use super::*;

#[test]
fn bp13_093_whole_card_combines_three_discounts_and_destroys_the_printed_target() {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"]["current"] = json!(0_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP13-093"},{"id":"other-1","card":"BP01-009"},
        {"id":"other-2","card":"BP01-009"}
    ]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..15_u8)
            .map(|i| json!({"id":format!("grave-{i}"),"card":"BP01-009"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "all three printed reductions make the 15 PP card free"
    );
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed Fanfare");
    };
    let CardPlay::Awaiting(target) = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("Fanfare target");
    };
    let InputDomain::Targets(domain) = &target.domain else {
        panic!("target domain");
    };
    resolved(
        game.answer_internal_card(
            target.actor,
            target.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["pp"]["current"], 0_u8,
        "a zero cost play never restores PP"
    );
    assert_eq!(
        view["P1"]["hand"],
        json!(["other-1", "other-2"]),
        "only the source leaves the hand"
    );
    assert_eq!(
        view["P2"]["cemetery"],
        json!(["enemy"]),
        "the native Fanfare destroys its selected target"
    );
}
