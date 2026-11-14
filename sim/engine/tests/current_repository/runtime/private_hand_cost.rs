use super::*;

#[expect(
    clippy::panic,
    reason = "Each observer is a separate pending ability in the printed procedure."
)]
fn resolve_observer(game: &mut Game, number: &str) {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("one printed observer");
    };
    assert_eq!(
        options.len(),
        1,
        "only one printed observer should remain pending"
    );
    assert_eq!(
        options[0].card, number,
        "the expected card must own the pending observer"
    );
    resolved(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    event_cemetery::finish_checks(game);
}

#[test]
fn elvira_uses_remaining_pp_for_hand_entry_and_buffs_other_followers_twice_per_turn() {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":6_u8,"max":10_u8});
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP21-093"},
        {"id":"same","card":"BP21-093"},
        {"id":"academy","card":"BP21-033"},
        {"id":"expensive","card":"BP21-020"},
        {"id":"other-trait","card":"BP01-173"}
    ]);
    let mut game = with_producer(
        &initial,
        event_combat_life::life(PlayerRef::Controller {}, 1),
    );
    assert!(matches!(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    event_cemetery::finish_checks(&mut game);
    let fanfare = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &fanfare.domain else {
        panic!("Elvira fanfare");
    };
    assert_eq!(
        options.len(),
        1,
        "only one printed observer should remain pending"
    );
    let CardPlay::Awaiting(choice) = game
        .answer_internal_card(
            fanfare.actor,
            fanfare.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("remaining PP hand choice");
    };
    let InputDomain::Targets(domain) = &choice.domain else {
        panic!("private Academy candidates");
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card.as_deref(), Some("BP21-033"));
    assert!(game.internal_decision(View::P2).is_none());
    resolved(
        game.answer_internal_card(
            choice.actor,
            choice.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    event_cemetery::finish_checks(&mut game);
    resolve_observer(&mut game, "BP21-033");
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(before["P1"]["pp"]["current"], 3_u8);
    for healing in 1_u8..=3 {
        activate(&mut game, "P1");
        event_cemetery::finish_checks(&mut game);
        if healing <= 2 {
            resolve_observer(&mut game, "BP21-093");
        } else {
            assert!(game.internal_pending_choice().unwrap().is_none());
        }
    }
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["leader"]["life"], 23_u8);
    for id in ["academy", "producer"] {
        for stat in ["power", "hp", "max_hp"] {
            assert_eq!(
                after["objects"][id][stat].as_i64().unwrap(),
                before["objects"][id][stat]
                    .as_i64()
                    .unwrap()
                    .checked_add(2)
                    .unwrap()
            );
        }
    }
    for stat in ["power", "hp", "max_hp"] {
        assert_eq!(
            after["objects"]["source"][stat],
            before["objects"]["source"][stat]
        );
    }
}
