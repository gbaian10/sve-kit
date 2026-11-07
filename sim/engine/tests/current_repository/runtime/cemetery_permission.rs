#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed cemetery permission scenarios."
)]
use super::*;

const ACTOR: PlayerId = PlayerId { seat: 1 };

fn position(number: &str, origin: &str, enemy: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"first","card":"BP01-171"},
        {"id":"second","card":"BP01-171"},
        {"id":"third","card":"BP01-171"}
    ]);
    let may = json!({"id":"source","card":number});
    if origin == "hand" {
        initial["players"]["P1"]["zones"][origin]
            .as_array_mut()
            .unwrap()
            .push(may);
    } else {
        initial["players"]["P1"]["zones"][origin] = json!([may]);
    }
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"reserve","card":"BP01-171"}]);
    if enemy {
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy","card":"BP01-171","state":{"hp":2_u8,"max_hp":2_u8}}
        ]);
    }
    initial
}

fn finish(board: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            other => panic!("unexpected May decision {other:?}"),
        };
        play = board
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
}

fn drain(board: &mut Game) {
    loop {
        while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            board.resume_internal_card().unwrap();
        }
        let Some(question) = board.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("May trigger ordering")
        };
        let play = board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(board, play);
    }
}

fn play_cards(board: &mut Game, holders: &[&str]) {
    for holder in holders {
        let entry = board.play_internal_card(holder, ACTOR).unwrap();
        finish(board, entry);
        drain(board);
    }
}

#[test]
fn bp18_007_all_prints_play_from_cemetery_after_three_cards_then_bottom_and_trigger_leave() {
    // Both printings reuse the expensive full public card selection.
    for number in ["BP18-007", "BP18-P03"] {
        let mut board = game(&position(number, "cemetery", true));
        play_cards(&mut board, &["first", "second", "third"]);
        let before_wrong_actor = format!("{board:?}");
        board
            .play_internal_card("source", PlayerId { seat: 2 })
            .unwrap_err();
        assert_eq!(format!("{board:?}"), before_wrong_actor);
        play_cards(&mut board, &["source"]);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 6_u8);
        assert_eq!(state["P1"]["field"], json!(["first", "second", "third"]));
        assert_eq!(state["P1"]["deck"], json!(["reserve", "source"]));
        assert_eq!(state["P2"]["field"], json!([]));
        assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    }
}

#[test]
fn bp18_007_all_prints_keep_hand_and_ex_plays_legal_before_the_cemetery_threshold() {
    for number in ["BP18-007", "BP18-P03"] {
        for origin in ["hand", "ex"] {
            let mut board = game(&position(number, origin, true));
            play_cards(&mut board, &["source"]);
            let state = board.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["pp"]["current"], 9_u8);
            if origin == "hand" {
                assert_eq!(state["P1"]["field"], json!(["source"]));
                assert_eq!(state["P1"]["deck"], json!(["reserve"]));
                assert_eq!(state["objects"]["enemy"]["hp"], 1_u8);
            } else {
                assert_eq!(state["P1"]["field"], json!([]));
                assert_eq!(state["P1"]["deck"], json!(["reserve", "source"]));
                assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
            }
        }
    }
}

#[test]
fn bp18_007_all_prints_stay_on_field_when_the_fanfare_has_no_formal_target() {
    for number in ["BP18-007", "BP18-P03"] {
        let mut board = game(&position(number, "cemetery", false));
        play_cards(&mut board, &["first", "second", "third", "source"]);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["field"],
            json!(["first", "second", "third", "source"])
        );
        assert_eq!(state["P1"]["deck"], json!(["reserve"]));
        assert_eq!(state["P1"]["pp"]["current"], 6_u8);
    }
}

#[test]
fn bp18_007_all_prints_do_not_count_the_pending_card_as_the_third_committed_play() {
    for number in ["BP18-007", "BP18-P03"] {
        let mut board = game(&position(number, "cemetery", true));
        play_cards(&mut board, &["first", "second"]);
        let before = format!("{board:?}");
        board.play_internal_card("source", ACTOR).unwrap_err();
        assert_eq!(format!("{board:?}"), before);
    }
}
