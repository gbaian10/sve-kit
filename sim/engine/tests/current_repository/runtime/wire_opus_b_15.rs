#![expect(
    clippy::panic,
    clippy::unwrap_in_result,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

/// Answers targets with the opponent leader (`leader`) or the first follower, and modes by position.
fn answer(game: &mut Game, mut step: CardPlay, leader: bool, mode: usize) -> Option<usize> {
    let mut offered = None;
    while let CardPlay::Awaiting(q) = step {
        let answer = match &q.domain {
            InputDomain::Targets(d) => {
                let member = d
                    .members
                    .iter()
                    .find(|m| m.card.is_none() == leader)
                    .unwrap();
                Answer::Targets(vec![member.handle])
            }
            InputDomain::Modes { modes, .. } => {
                offered = Some(modes.len());
                Answer::Modes(vec![modes[mode]])
            }
            other => panic!("unexpected printed decision: {other:?}"),
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    finish_checks(game);
    offered
}

/// Casts one own-leader-damaging spell at the opponent leader, then resolves Laura's trigger.
fn cut(game: &mut Game, id: &str, mode: usize) -> Option<usize> {
    let cast = game.play_internal_card(id, actor()).unwrap();
    answer(game, cast, true, 0);
    let q = game.internal_pending_choice().unwrap()?;
    let InputDomain::Pending(options) = &q.domain else {
        panic!("Laura's trigger");
    };
    assert_eq!(options.len(), 1, "only the own leader's life loss triggers");
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap();
    let offered = answer(game, step, false, mode);
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "one trigger per loss"
    );
    offered
}

#[test]
fn bp13_073_all_printings_offer_each_mode_once_per_turn() {
    for printing in ["BP13-073", "BP13-SL17", "BP13-U05"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":printing}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"cut0","card":"BP01-125"},
            {"id":"cut1","card":"BP01-125"},
            {"id":"cut2","card":"BP01-125"}
        ]);
        initial["players"]["P1"]["pp"] = json!({"current":10_u8,"max":10_u8});
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        let before = game.projection(View::Referee).unwrap();
        // First loss: all three modes; the damage equals one loss so far.
        assert_eq!(cut(&mut game, "cut0", 0), Some(3), "{printing}");
        let first = game.projection(View::Referee).unwrap();
        assert_eq!(first["objects"]["enemy"]["damage"], 1_u8, "{printing}");
        // Second loss: the chosen damage mode is gone; take +1/+1.
        assert_eq!(cut(&mut game, "cut1", 0), Some(2), "{printing}");
        let second = game.projection(View::Referee).unwrap();
        assert_eq!(second["objects"]["enemy"]["damage"], 1_u8, "{printing}");
        for stat in ["power", "max_hp"] {
            assert_eq!(
                second["objects"]["source"][stat].as_u64().unwrap(),
                before["objects"]["source"][stat].as_u64().unwrap() + 1,
                "{printing} {stat}"
            );
        }
        // Third loss: only Storm remains.
        assert_eq!(cut(&mut game, "cut2", 0), Some(1), "{printing}");
        let third = game.projection(View::Referee).unwrap();
        assert!(
            third["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走")),
            "{printing}: {}",
            third["objects"]["source"]
        );
    }
}
