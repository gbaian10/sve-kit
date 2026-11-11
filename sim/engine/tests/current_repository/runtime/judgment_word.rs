use super::*;

fn position(card: &str, sixth: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"] = json!({"class":"ウィッチ","life":15_u8});
    initial["players"]["P1"]["pp"]["current"] = json!(0_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    let cemetery = ["BP01-171", "BP01-170", "BP01-177", "BP01-169", "BP01-176", sixth]
        .into_iter()
        .enumerate()
        .map(|(index, printing)| json!({"id":format!("buried-{index}"),"card":printing}))
        .collect::<Vec<_>>();
    initial["players"]["P1"]["zones"]["cemetery"] = json!(cemetery);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-176"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-171"}]);
    initial
}

#[test]
fn judgment_word_both_printings_suppress_until_turn_end_and_add_life_at_six_original_costs() {
    for (card, sixth, life) in [
        ("BP11-041", "BP11-P11", 17_u8),
        ("BP11-P11", "BP01-179", 15_u8),
    ] {
        let mut board = game(&position(card, sixth));
        let before = board.projection(View::Referee).unwrap();
        assert_eq!(before["objects"]["enemy"]["keywords"], json!(["守護"]));

        let play = board
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        let CardPlay::Awaiting(question) = play else {
            panic!("Judgment Word must select an opponent follower to be played");
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("expected the spell's formal target");
        };
        assert_eq!(domain.cardinality.minimum, Nat(1));
        assert_eq!(domain.members.len(), 1);
        resolved(
            board
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
        );
        if board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            board.resume_internal_card().unwrap();
        }
        let after_spell = board.projection(View::Referee).unwrap();
        assert_eq!(after_spell["objects"]["enemy"]["keywords"], json!([]));
        assert_eq!(after_spell["P1"]["leader"]["life"], life, "{card}");
        assert_eq!(after_spell["P1"]["pp"]["current"], 0_u8);
        assert!(
            after_spell["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source"))
        );

        board
            .internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let at_end_phase = board.projection(View::Referee).unwrap();
        assert_eq!(at_end_phase["objects"]["enemy"]["keywords"], json!([]));
        let window = board.internal_quick_choice().unwrap().unwrap();
        board
            .answer_internal_card(window.actor, window.token, Answer::Quick(None))
            .unwrap();
        let next_turn = board.projection(View::Referee).unwrap();
        assert_eq!(next_turn["turn"]["active"], "P2");
        assert_eq!(next_turn["objects"]["enemy"]["keywords"], json!(["守護"]));
    }
}
