#![expect(
    clippy::panic,
    reason = "A different pending domain fails the scenario immediately."
)]

use super::*;

fn resolve_life(game: &mut Game, seat: &str, card: &str, amount: i64) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected pending life ability");
    };
    assert_eq!(
        options.len(),
        1,
        "exactly the migrated card's ability is pending"
    );
    assert_eq!(
        options[0].card, card,
        "the pending source keeps its card identity"
    );
    let handle = options[0].handle;
    let before = format!("{game:?}");
    game.answer_internal_card(
        PlayerId {
            seat: if question.actor.seat == 1 { 2 } else { 1 },
        },
        question.token,
        Answer::Pending(handle),
    )
    .unwrap_err();
    assert!(
        format!("{game:?}") == before,
        "wrong actor changed authority"
    );
    let result = resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Pending(handle))
            .unwrap(),
    );
    assert_eq!(
        result.source.controller.seat,
        seat_number(seat),
        "the original controller resolves the ability"
    );
    let changes = result
        .progress
        .facts
        .iter()
        .filter_map(|fact| {
            if let Fact::LifeChange(event) = fact {
                Some(event)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(
        matches!(changes.as_slice(), [event]
        if event.subject.seat == seat_number(seat) && event.delta == Int(amount)),
        "exactly one life change belongs to the resolving controller"
    );
    assert!(
        !result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Damage(_))),
        "direct life changes do not deal damage"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()[seat]["leader"]["life"],
        20_i64.checked_add(amount).unwrap(),
        "the controller's leader gains the printed amount"
    );
    let completed = format!("{game:?}");
    game.answer_internal_card(question.actor, question.token, Answer::Pending(handle))
        .unwrap_err();
    assert!(
        format!("{game:?}") == completed,
        "old pending answer changed authority"
    );
}

#[test]
fn bp19_108_shared_lastword_adds_life_only_after_its_departure() {
    for seat in ["P1", "P2"] {
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(seat);
        initial["players"][seat]["zones"]["field"] = json!([{"id":"healer","card":"BP19-108"}]);
        let mut game = with_producer(
            &initial,
            Action::Destroy {
                objects: subjects(),
            },
        );
        assert!(game.internal_pending_choice().unwrap().is_none());
        activate(&mut game, seat);
        game.resume_internal_card().unwrap();
        resolve_life(&mut game, seat, "BP19-108", 1);
        assert!(
            game.projection(View::Referee).unwrap()[seat]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("healer"))
        );
    }
}

#[test]
fn bp01_177_fanfare_adds_life_after_entry_and_rejects_the_other_actor() {
    for seat in ["P1", "P2"] {
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(seat);
        initial["players"][seat]["zones"]["hand"] = json!([{"id":"healer","card":"BP01-177"}]);
        let mut game = game(&initial);
        let before = format!("{game:?}");
        game.play_internal_card(
            "healer",
            PlayerId {
                seat: if seat == "P1" { 2 } else { 1 },
            },
        )
        .unwrap_err();
        assert!(
            format!("{game:?}") == before,
            "wrong actor changed authority"
        );
        assert!(matches!(
            game.play_internal_card(
                "healer",
                PlayerId {
                    seat: seat_number(seat)
                }
            )
            .unwrap(),
            CardPlay::Entered(_)
        ));
        resolve_life(&mut game, seat, "BP01-177", 1);
    }
}

#[test]
fn bp01_152_end_begin_adds_four_life_without_a_player_activation() {
    for seat in ["P1", "P2"] {
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(seat);
        initial["players"][seat]["zones"]["field"] = json!([{"id":"healer","card":"BP01-152"}]);
        let mut game = game(&initial);
        assert!(game.internal_pending_choice().unwrap().is_none());
        let end = game
            .internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        assert_eq!(end.outcome, "resolved");
        resolve_life(&mut game, seat, "BP01-152", 4);
    }
}
