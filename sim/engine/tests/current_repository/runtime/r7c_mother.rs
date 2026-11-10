#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed Mother choices."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn initial(number: &str, count: usize) -> Value {
    let mut position = draw_position();
    position["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    position["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"draw","card":"BP01-171"},{"id":"tail","card":"BP01-171"}
    ]);
    let mut forms = (0..count)
        .map(|n| json!({"id":format!("f{n}"),"card":"BP07-105","face_up":true}))
        .collect::<Vec<_>>();
    forms.extend([
        json!({"id":"down","card":"BP07-002","face_up":false}),
        json!({"id":"other","card":"BP01-156","face_up":true}),
        json!({"id":"advance","card":"BP13-039","face_up":true}),
    ]);
    position["players"]["P1"]["zones"]["evolve_deck"] = json!(forms);
    position["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
    position
}

fn finish(board: &mut Game, mut play: CardPlay, modes: &[ModeId], accept: bool, count: usize) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Modes { .. } => Answer::Modes(modes.to_vec()),
            InputDomain::Integer(_) => Answer::Integer(Nat(u64::from(accept))),
            InputDomain::Targets(domain) => {
                let number = usize::try_from(domain.cardinality.minimum.0).unwrap();
                if number == 2 {
                    assert_eq!(
                        domain.members.len(),
                        count,
                        "only face-up Nature evolve followers can be flipped"
                    );
                    assert_eq!(
                        domain.cardinality.maximum,
                        Some(Nat(2)),
                        "the printed optional action turns exactly two forms"
                    );
                }
                Answer::Targets(
                    domain
                        .members
                        .iter()
                        .take(number)
                        .map(|member| member.handle)
                        .collect(),
                )
            }
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain.cards.iter().map(|card| card.handle).collect(),
            )),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            other => panic!("unexpected Mother choice {other:?}"),
        };
        play = board
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
}

#[test]
fn mother_all_printings_flip_exactly_two_or_nothing_then_draw_and_recover_pp() {
    for number in ["BP17-112", "BP17-SL28"] {
        for (count, accept) in [(0_usize, true), (1, true), (2, true), (3, true), (2, false)] {
            let mut board = game(&initial(number, count));
            let play = board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut board, play, &[ModeId(3)], accept, count);
            let state = board.projection(View::Referee).unwrap();
            let successful = accept && count >= 2;
            assert_eq!(
                state["P1"]["pp"]["current"],
                if successful { 7_u8 } else { 5 }
            );
            assert_eq!(
                state["P1"]["hand"],
                if successful {
                    json!(["draw"])
                } else {
                    json!([])
                }
            );
            assert_eq!(state["P1"]["cemetery"], json!(["source"]));
            for n in 0..count {
                let id = format!("f{n}");
                assert_eq!(state["objects"][&id]["face_up"], !(successful && n < 2));
                if successful && n < 2 {
                    assert!(
                        board.projection(View::P2).unwrap()["objects"]
                            .get(&id)
                            .is_none(),
                        "flipped forms no longer have public location handles"
                    );
                }
            }
            assert_eq!(state["objects"]["down"]["face_up"], false);
            assert_eq!(state["objects"]["other"]["face_up"], true);
            assert_eq!(state["objects"]["advance"]["face_up"], true);
        }
    }
}

#[test]
fn mother_destroy_and_flip_modes_choose_formal_target_before_optional_resolution() {
    for number in ["BP17-112", "BP17-SL28"] {
        let mut board = game(&initial(number, 2));
        let CardPlay::Awaiting(mode) = board
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("printed modes")
        };
        let CardPlay::Awaiting(target) = board
            .answer_internal_card(
                mode.actor,
                mode.token,
                Answer::Modes(vec![ModeId(1), ModeId(3)]),
            )
            .unwrap()
        else {
            panic!("formal target precedes the body")
        };
        assert!(
            matches!(target.domain, InputDomain::Targets(_)),
            "the target is chosen before optional flipping"
        );
        let before = board.projection(View::Referee).unwrap();
        assert_eq!(before["P2"]["field"], json!(["enemy"]));
        assert_eq!(before["P1"]["deck"], json!(["draw", "tail"]));
        finish(&mut board, CardPlay::Awaiting(target), &[], true, 2);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(after["P2"]["cemetery"], json!(["enemy"]));
        assert_eq!(after["P1"]["hand"], json!(["draw"]));
        assert_eq!(after["P1"]["pp"]["current"], 7_u8);
    }
}

#[test]
fn mother_search_mode_enters_its_fully_admitted_named_dependency() {
    for number in ["BP17-112", "BP17-SL28"] {
        let mut position = initial(number, 0);
        position["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"mother","card":"BP07-104"},{"id":"decoy","card":"BP01-171"}]);
        let mut board = game(&position);
        let play = board
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut board, play, &[ModeId(2)], false, 0);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["mother"]));
        assert_eq!(state["P1"]["deck"], json!(["decoy"]));
        assert_eq!(state["P1"]["pp"]["current"], 5_u8);
    }
}
