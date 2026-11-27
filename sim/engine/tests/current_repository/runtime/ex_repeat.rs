use super::event_cemetery::finish_checks;
use super::union_burst_batch1::finish;
use super::*;

fn position(number: &str, old: usize, deck: usize) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["ex"] = json!(
        (0..old)
            .map(|i| json!({"id":format!("old-{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0..deck)
            .map(|i| json!({"id":format!("deck-{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial
}
fn build(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ex-repeat",
    )
    .unwrap()
}
fn cast(game: &mut Game) -> Activation {
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    let result = resolved(finish(game, play));
    finish_checks(game);
    result
}

#[test]
fn wish_fills_ex_sequentially_and_only_one_successful_new_member_is_free() {
    let mut game = build(&position("BP14-046", 2, 6));
    let result = cast(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["ex"],
        json!(["old-0", "old-1", "deck-0", "deck-1", "deck-2"])
    );
    assert_eq!(state["P1"]["deck"], json!(["deck-3", "deck-4", "deck-5"]));
    assert_eq!(result.progress.facts.iter().filter(|fact| matches!(fact, Fact::Movement(m) if m.from.kind == ZoneKind::Deck && m.movement.actual_to.is_some_and(|to| to.kind == ZoneKind::Ex))).count(), 3);
    let old = game
        .play_internal_card("old-0", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, old);
    finish_checks(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        2_u8
    );
    let first = game
        .play_internal_card("deck-0", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, first);
    finish_checks(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        2_u8
    );
    let second = game
        .play_internal_card("deck-1", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, second);
    finish_checks(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        1_u8
    );
}

#[test]
fn wish_stops_at_capacity_or_empty_deck_without_drawing_or_losing() {
    for (old, deck, moved) in [(5, 3, 0), (0, 2, 2), (2, 0, 0), (0, 7, 5)] {
        let mut game = build(&position("BP14-046", old, deck));
        cast(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), old + moved);
        assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), deck - moved);
        assert_ne!(state["game"]["ended"], true);
        if old == 5 {
            assert!(
                game.projection(View::P1).unwrap()["objects"]
                    .get("deck-0")
                    .is_none(),
                "a full EX never observes the deck top"
            );
        }
    }
}

#[test]
fn mystic_acquisition_banishes_all_ex_at_the_next_own_end_including_old_cards_and_tokens() {
    let mut initial = position("BP01-056", 1, 6);
    initial["players"]["P1"]["zones"]["ex"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"token","card":"BP01-T01"}));
    let mut game = build(&initial);
    cast(&mut game);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let play = union_burst_batch1::pending(&mut game, "BP01-056");
    resolved(finish(&mut game, play));
    finish_checks(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!([]));
    assert_eq!(state["P1"]["banish"].as_array().unwrap().len(), 4);
    assert!(
        state["objects"].get("token").is_none(),
        "banished tokens are erased"
    );
}

#[test]
fn mystic_acquisition_cast_as_quick_ignores_the_opponents_end_and_survives_into_its_own_turn() {
    let mut initial = position("BP01-056", 5, 5);
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P2"]["zones"]["deck"] = json!([{"card":"BP01-171","id":"enemy-draw"}]);
    let mut game = build(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let q = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &q.domain else {
        panic!("Quick window")
    };
    let answer = game
        .answer_internal_card(q.actor, q.token, Answer::Quick(Some(options[0].handle)))
        .unwrap();
    resolved(finish(&mut game, answer));
    finish_checks(&mut game);
    assert!(game.internal_pending_choice().unwrap().is_none());
    damage_prevent::pass_quick(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P1"
    );
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let play = union_burst_batch1::pending(&mut game, "BP01-056");
    resolved(finish(&mut game, play));
    finish_checks(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"],
        json!([])
    );
}

#[test]
fn the_wish_discount_expires_at_turn_end() {
    let mut initial = position("BP14-046", 0, 7);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"card":"BP01-171","id":"enemy-draw"}]);
    let mut game = build(&initial);
    cast(&mut game);
    next_turn(&mut game);
    next_turn(&mut game);
    let before = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
        .as_i64()
        .unwrap();
    let play = game
        .play_internal_card("deck-0", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, play);
    finish_checks(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        before - 1
    );
}

#[test]
fn an_illegal_field_entry_does_not_spend_the_shared_free_play() {
    let mut initial = position("BP14-046", 0, 6);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"reclaim","card":"BP01-109"},
        {"id":"a","card":"BP01-171"},{"id":"b","card":"BP01-171"},
        {"id":"c","card":"BP01-171"},{"id":"d","card":"BP01-171"}
    ]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"returned","card":"BP22-025"}]);
    let mut game = build(&initial);
    cast(&mut game);
    game.play_internal_card("deck-0", PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        3_u8
    );
    let ability = selection().current["BP01-109"]
        .program
        .card()
        .abilities
        .iter()
        .find(|entry| entry.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let reclaim = game
        .activate_internal_card("reclaim", ability, PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(&mut game, reclaim));
    finish_checks(&mut game);
    let entry = game
        .play_internal_card("deck-0", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, entry);
    finish_checks(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        3_u8
    );
}
