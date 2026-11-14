use super::*;

#[test]
fn thief_ambush_orders_the_remainder_then_buffs_and_grants_rush_to_the_entered_thief() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP11-033"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"thief","card":"BP11-027"},
        {"id":"first","card":"BP01-173"},
        {"id":"middle","card":"BP01-046"},
        {"id":"last","card":"BP01-042"}
    ]);
    let mut game = game(&initial);
    let before = game.projection(View::Referee).unwrap();
    let CardPlay::Awaiting(choice) = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("Look selection");
    };
    let InputDomain::Targets(domain) = &choice.domain else {
        panic!("one card among the four looked cards");
    };
    assert_eq!(domain.members.len(), 4);
    assert!(game.internal_decision(View::P2).is_none());
    let thief = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP11-027"))
        .unwrap()
        .handle;
    let CardPlay::Awaiting(order) = game
        .answer_internal_card(choice.actor, choice.token, Answer::Targets(vec![thief]))
        .unwrap()
    else {
        panic!("remainder ordering");
    };
    let InputDomain::Order(cards) = &order.domain else {
        panic!("bottom order");
    };
    assert_eq!(cards.len(), 3);
    resolved(
        game.answer_internal_card(
            order.actor,
            order.token,
            Answer::Order(cards.iter().rev().map(|card| card.handle).collect()),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["deck"], json!(["last", "middle", "first"]));
    assert_eq!(view["P1"]["field"], json!(["thief"]));
    for stat in ["power", "hp", "max_hp"] {
        assert_eq!(
            view["objects"]["thief"][stat].as_i64().unwrap(),
            before["objects"]["thief"][stat]
                .as_i64()
                .unwrap()
                .checked_add(1)
                .unwrap()
        );
    }
    assert!(
        view["objects"]["thief"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("突進"))
    );
    assert_eq!(view["P1"]["cemetery"], json!(["source"]));
}

#[test]
fn heroic_entry_counts_the_resolved_spell_before_the_entered_guardian_fanfare() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP03-037"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"old-hero","card":"BP03-023"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"guard","card":"BP03-036"},
        {"id":"first","card":"BP01-173"},
        {"id":"middle","card":"BP03-033"},
        {"id":"last","card":"BP03-020"}
    ]);
    let mut game = game(&initial);
    let before = game.projection(View::Referee).unwrap();
    let CardPlay::Awaiting(choice) = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("Heroic Entry selection");
    };
    let InputDomain::Targets(domain) = &choice.domain else {
        panic!("four looked cards");
    };
    let guard = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP03-036"))
        .unwrap()
        .handle;
    let CardPlay::Awaiting(order) = game
        .answer_internal_card(choice.actor, choice.token, Answer::Targets(vec![guard]))
        .unwrap()
    else {
        panic!("remaining cards");
    };
    let InputDomain::Order(cards) = &order.domain else {
        panic!("bottom order");
    };
    resolved(
        game.answer_internal_card(
            order.actor,
            order.token,
            Answer::Order(cards.iter().map(|card| card.handle).collect()),
        )
        .unwrap(),
    );
    meal_race::settle(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["pp"]["current"], 7_u8);
    assert_eq!(after["P1"]["field"], json!(["guard"]));
    assert_eq!(after["P1"]["cemetery"], json!(["old-hero", "source"]));
    assert_eq!(after["P1"]["deck"], json!(["first", "middle", "last"]));
    for (stat, delta) in [("power", 1), ("hp", 2), ("max_hp", 2)] {
        assert_eq!(
            after["objects"]["guard"][stat].as_i64().unwrap(),
            before["objects"]["guard"][stat]
                .as_i64()
                .unwrap()
                .checked_add(delta)
                .unwrap()
        );
    }
    assert!(
        after["objects"]["guard"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("守護"))
    );
}
