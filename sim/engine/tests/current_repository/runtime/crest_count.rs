#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed count scenarios."
)]
use super::*;

fn position(card: &str, count: usize) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"BP01-009"}]);
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    let crests = ["BP20-P60", "BP20-T05", "BP20-P70"];
    let mut ex: Vec<_> = crests
        .iter()
        .take(count)
        .enumerate()
        .map(|(i, crest_card)| json!({"id":format!("crest-{i}"),"card":crest_card}))
        .collect();
    ex.push(json!({"id":"ordinary-ex","card":"BP01-009"}));
    initial["players"]["P1"]["zones"]["ex"] = json!(ex);
    initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"foreign-crest","card":"BP20-P60"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{
        "id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}
    }]);
    initial
}

fn finish(board: &mut Game, mut play: CardPlay) {
    loop {
        while let CardPlay::Awaiting(q) = play {
            let answer = match &q.domain {
                InputDomain::Targets(_) => Answer::Targets(vec![0]),
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                _ => panic!("unexpected count decision"),
            };
            play = board
                .answer_internal_card(q.actor, q.token, answer)
                .unwrap();
        }
        assert!(
            matches!(play, CardPlay::Entered(_) | CardPlay::Resolved(_)),
            "the printed entry or ability completes before checking pending triggers"
        );
        if board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            board.resume_internal_card().unwrap();
        }
        let Some(q) = board.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("pending")
        };
        play = board
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
    }
}

#[test]
fn bp20_107_all_printings_damage_by_own_ex_crests_only() {
    for card in ["BP20-107", "BP20-P53"] {
        let mut board = game(&position(card, 2));
        let play = board
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut board, play);
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["enemy"]["hp"], 8_u8);
    }
}

#[test]
fn bp20_101_all_printings_heal_at_three_crests_but_not_two() {
    for card in ["BP20-101", "BP20-P50"] {
        for (count, life) in [(2_usize, 10_u8), (3, 12)] {
            let mut board = game(&position(card, count));
            let play = board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut board, play);
            assert_eq!(
                board.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                life
            );
        }
    }
}

#[test]
fn bp20_100_all_printings_refuse_below_three_and_pay_source_before_damage() {
    for card in ["BP20-100", "BP20-P49"] {
        let definition = selection().current[card]
            .program
            .card()
            .abilities
            .iter()
            .find(|a| a.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        for count in [2_usize, 3] {
            let mut board = game(&position(card, count));
            let play = board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut board, play);
            assert_eq!(
                board.projection(View::Referee).unwrap()["P1"]["hand"],
                json!(["top"])
            );
            let before = board.projection(View::Referee).unwrap();
            let attempt = board.activate_internal_card("source", definition, PlayerId { seat: 1 });
            if count == 2 {
                attempt.unwrap_err();
                assert_eq!(board.projection(View::Referee).unwrap(), before);
            } else {
                finish(&mut board, attempt.unwrap());
                let view = board.projection(View::Referee).unwrap();
                assert_eq!(view["P1"]["cemetery"], json!(["source"]));
                assert_eq!(view["objects"]["enemy"]["hp"], 6_u8);
            }
        }
    }
}
