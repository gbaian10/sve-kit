#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str, pp: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":pp,"max":10_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

/// Answers each printed target question with the member showing `chosen`.
fn answer(game: &mut Game, mut step: CardPlay, chosen: &str) {
    while let CardPlay::Awaiting(q) = step {
        let InputDomain::Targets(d) = &q.domain else {
            panic!("unexpected printed decision: {:?}", q.domain);
        };
        let member = d
            .members
            .iter()
            .find(|m| m.card.as_deref() == Some(chosen))
            .unwrap();
        step = game
            .answer_internal_card(q.actor, q.token, Answer::Targets(vec![member.handle]))
            .unwrap();
    }
    finish_checks(game);
}

fn fanfare(game: &mut Game, chosen: &str) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed pending ability");
    };
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap();
    answer(game, step, chosen);
}

fn banished(seat: &str, count: usize) -> Value {
    (0..count)
        .map(|i| json!({"id":format!("{seat}-gone{i}"),"card":"BP01-037"}))
        .collect()
}

#[test]
fn bp17_051_costs_less_for_each_own_banished_card_then_draws_and_banishes() {
    for (count, paid) in [(0_usize, 4_u8), (3, 1), (6, 0)] {
        let mut initial = position("BP17-051", 10);
        initial["players"]["P1"]["zones"]["banish"] = banished("P1", count);
        initial["players"]["P2"]["zones"]["banish"] = banished("P2", 2);
        let mut game = game(&initial);
        let step = game.play_internal_card("source", actor()).unwrap();
        assert!(matches!(step, CardPlay::Entered(_)), "the follower enters");
        finish_checks(&mut game);
        let paid_view = game.projection(View::Referee).unwrap();
        // The opponent's banish zone does not count.
        assert_eq!(
            paid_view["P1"]["pp"]["current"],
            10 - paid,
            "{count} banished"
        );
        fanfare(&mut game, "BP01-037");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["hand"], json!([]));
        assert_eq!(view["P1"]["banish_count"], count + 1);
    }
}
