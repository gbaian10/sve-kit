#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

/// Casts 堕天 on the follower controlled by seat `chosen` and returns the projection.
fn fall(chosen: u32) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP06-112"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"own_top","card":"BP01-037"}]);
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"enemy_top","card":"BP01-037"}]);
    initial["players"]["P2"]["leader"]["life"] = json!(10_u8);
    let mut game = game(&initial);
    let mut step = game.play_internal_card("source", actor()).unwrap();
    while let CardPlay::Awaiting(q) = step {
        let InputDomain::Targets(d) = &q.domain else {
            panic!("unexpected printed decision: {:?}", q.domain);
        };
        let member = d.members.iter().find(|m| m.player.seat == chosen).unwrap();
        step = game
            .answer_internal_card(q.actor, q.token, Answer::Targets(vec![member.handle]))
            .unwrap();
    }
    game.projection(View::Referee).unwrap()
}

#[test]
fn bp06_112_banishes_a_follower_and_its_controller_gains_two_and_draws() {
    let view = fall(2);
    assert_eq!(view["P2"]["banish"], json!(["enemy"]));
    assert_eq!(view["P2"]["leader"]["life"], 12_u8);
    assert_eq!(view["P2"]["hand"], json!(["enemy_top"]));
    assert_eq!(view["P1"]["leader"]["life"], 10_u8);
    assert_eq!(view["P1"]["hand"], json!([]));
}

#[test]
fn bp06_112_on_an_own_follower_heals_and_draws_for_its_caster() {
    let view = fall(1);
    assert_eq!(view["P1"]["banish"], json!(["ally"]));
    assert_eq!(view["P1"]["leader"]["life"], 12_u8);
    assert_eq!(view["P1"]["hand"], json!(["own_top"]));
    assert_eq!(view["P2"]["leader"]["life"], 10_u8);
    assert_eq!(view["P2"]["hand"], json!([]));
}
