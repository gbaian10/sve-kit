#![expect(
    clippy::panic,
    reason = "Unexpected answers invalidate the real variable-reveal scenarios."
)]
use super::*;
use sve_engine::game::internal::DamageShare;

fn evolved(super_evolution: bool, enemy_count: usize) -> Game {
    let mut position = draw_position();
    position["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"CP05-056"}]);
    position["players"]["P1"]["zones"]["evolve_deck"] =
        json!([{"id":"form","card":"CP05-057"}]);
    position["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"fire1","card":"CP05-056"}, {"id":"fire2","card":"CP05-056"},
        {"id":"fire3","card":"CP05-056"}, {"id":"other","card":"BP01-037"}
    ]);
    position["players"]["P1"]["sep"] = json!(u8::from(super_evolution));
    position["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    position["players"]["P2"]["zones"]["field"] = json!(
        (0..enemy_count)
            .map(|index| json!({"id":format!("enemy{index}"),"card":"BP04-117"}))
            .collect::<Vec<_>>()
    );
    let mut game = game(&position);
    assert_eq!(
        game.internal_rule_action(&json!({
            "do":"evolve","source":"source","evolve_card":"form",
            "pay":{"pp":1_u8,"sep":u8::from(super_evolution)}
        })).unwrap().outcome,
        "resolved"
    );
    game
}

fn pending_line(game: &mut Game, line: u64) -> CardPlay {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed evolution trigger");
    };
    let option = options.iter().find(|option| option.line == line).unwrap();
    game.answer_internal_card(q.actor, q.token, Answer::Pending(option.handle)).unwrap()
}

fn accept(game: &mut Game, play: CardPlay) -> CardPlay {
    let CardPlay::Awaiting(fee) = play else {
        panic!("head cost declaration");
    };
    assert_eq!(fee.domain, InputDomain::OptionalCost);
    game.answer_internal_card(fee.actor, fee.token, Answer::OptionalCost(true)).unwrap()
}

fn reveal(game: &mut Game, play: CardPlay, count: usize) -> CardPlay {
    let CardPlay::Awaiting(question) = play else {
        panic!("private reveal materials");
    };
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("private reveal materials");
    };
    assert_eq!(domain.members.len(), 3);
    assert!(domain.members.iter().all(|member| member.card.as_deref() == Some("CP05-056")));
    assert!(game.internal_decision(View::P2).is_none());
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::CostMaterials(domain.members.iter().take(count).map(|member| member.handle).collect()),
    ).unwrap()
}

fn target(game: &mut Game, play: CardPlay, count: usize) -> CardPlay {
    let CardPlay::Awaiting(question) = play else {
        panic!("formal target question");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("formal target domain");
    };
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Targets(domain.members.iter().take(count).map(|member| member.handle).collect()),
    ).unwrap()
}

#[test]
fn cp05_057_uses_selected_fire_card_count_for_formal_damage_allocation() {
    let mut game = evolved(false, 3);
    let step = pending_line(&mut game, 1);
    let step = accept(&mut game, step);
    let step = reveal(&mut game, step, 3);
    let CardPlay::Awaiting(ref question) = step else {
        panic!("formal target question");
    };
    let InputDomain::Targets(targets) = &question.domain else {
        panic!("formal targets");
    };
    assert_eq!(targets.cardinality.maximum, Some(Nat(2)));
    let CardPlay::Awaiting(allocation) = target(&mut game, step, 2) else {
        panic!("variable damage distribution");
    };
    let InputDomain::Distribution(domain) = &allocation.domain else {
        panic!("distribution domain");
    };
    assert_eq!(domain.total, Nat(3));
    let shots = domain.members.iter().zip([Nat(1), Nat(2)])
        .map(|(member, amount)| DamageShare { handle: member.handle, amount })
        .collect();
    resolved(game.answer_internal_card(allocation.actor, allocation.token, Answer::Distribution(shots)).unwrap());
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["enemy0"]["damage"], 1_u8);
    assert_eq!(view["objects"]["enemy1"]["damage"], 2_u8);
    assert_eq!(view["objects"]["enemy2"]["damage"], 0_u8);
    assert_eq!(view["P1"]["hand"], json!(["fire1","fire2","fire3","other"]));
}

#[test]
fn cp05_057_super_evolution_damages_the_chosen_follower_and_leader_by_actual_payment() {
    let mut game = evolved(true, 1);
    let step = pending_line(&mut game, 2);
    let step = accept(&mut game, step);
    let step = target(&mut game, step, 1);
    resolved(reveal(&mut game, step, 2));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["enemy0"]["damage"], 2_u8);
    assert_eq!(view["P2"]["leader"]["life"], 18_u8);
    assert!(game.projection(View::P2).unwrap()["objects"]["fire1"].is_null());
}

#[test]
fn cp05_056_adds_only_the_matching_top_fire_card_without_moving_the_next_card() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP05-SL15"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"fire","card":"CP05-056"}, {"id":"tail","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    assert!(matches!(
        game.play_internal_card("source", PlayerId { seat: 1 }).unwrap(),
        CardPlay::Entered(_)
    ));
    let look = pending_line(&mut game, 2);
    resolved(target(&mut game, look, 1));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"], json!(["fire"]));
    assert_eq!(view["P1"]["deck"], json!(["tail"]));
    let public = game.projection(View::P2).unwrap();
    assert!(public["known_cards"].as_array().unwrap().iter().any(|card| card["id"] == "fire"));
}

#[test]
fn cp05_057_may_pay_with_zero_ordinary_targets_but_requires_one_for_super_evolution() {
    let mut unpaid = evolved(false, 0);
    let normal = pending_line(&mut unpaid, 1);
    let normal = accept(&mut unpaid, normal);
    let normal = reveal(&mut unpaid, normal, 0);
    resolved(target(&mut unpaid, normal, 0));
    let public = unpaid.projection(View::P2).unwrap();
    assert!(public["known_cards"].as_array().unwrap().iter().all(|card| card["id"] != "fire1"));

    let mut ordinary = evolved(false, 0);
    let normal = pending_line(&mut ordinary, 1);
    let normal = accept(&mut ordinary, normal);
    let normal = reveal(&mut ordinary, normal, 1);
    resolved(target(&mut ordinary, normal, 0));
    let public = ordinary.projection(View::P2).unwrap();
    assert!(public["known_cards"].as_array().unwrap().iter().any(|card| card["id"] == "fire1"));
    assert_eq!(ordinary.projection(View::Referee).unwrap()["P2"]["leader"]["life"], 20_u8);

    let mut super_evolved = evolved(true, 0);
    let ability = pending_line(&mut super_evolved, 2);
    assert!(matches!(accept(&mut super_evolved, ability), CardPlay::Cancelled));
    let public = super_evolved.projection(View::P2).unwrap();
    assert!(public["known_cards"].as_array().unwrap().iter().all(|card| card["id"] != "fire1"));
    assert_eq!(super_evolved.projection(View::Referee).unwrap()["P2"]["leader"]["life"], 20_u8);
}
