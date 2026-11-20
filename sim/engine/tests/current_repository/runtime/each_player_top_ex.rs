#![expect(
    clippy::panic,
    reason = "The printed Fanfare must resolve as two movement rounds."
)]
use super::*;

#[test]
fn bp08_115_moves_both_tops_before_starting_the_second_round() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP08-115"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"own-first","card":"BP01-037"},{"id":"own-second","card":"BP01-006"}
    ]);
    initial["players"]["P2"]["zones"]["deck"] = json!([
        {"id":"opp-first","card":"BP01-037"},{"id":"opp-second","card":"CP03-101"}
    ]);
    initial["players"]["P2"]["zones"]["ex"] = json!(
        (0..4)
            .map(|n| json!({"id":format!("old-{n}"),"card":"BP01-037"}))
            .collect::<Vec<_>>()
    );
    let mut game = game(&initial);
    play_grant_source(&mut game, None);
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("printed Fanfare")
    };
    let result = resolved(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    let seats = result
        .progress
        .facts
        .iter()
        .filter_map(|fact| match fact {
            Fact::Movement(event)
                if event.from.kind == ZoneKind::Deck
                    && event
                        .movement
                        .actual_to
                        .is_some_and(|to| to.kind == ZoneKind::Ex) =>
            {
                Some(event.from.player.seat)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(seats, [1, 2, 1]);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["ex"], json!(["own-first", "own-second"]));
    assert_eq!(
        after["P2"]["ex"],
        json!(["old-0", "old-1", "old-2", "old-3", "opp-first"])
    );
    assert_eq!(after["P2"]["deck"], json!(["opp-second"]));
    let opponent_view = game.projection(View::P1).unwrap();
    assert!(
        !opponent_view["objects"]
            .as_object()
            .unwrap()
            .values()
            .any(|object| object["card"] == "CP03-101")
    );
}
