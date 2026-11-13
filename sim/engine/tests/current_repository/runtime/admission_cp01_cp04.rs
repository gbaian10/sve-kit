use super::admission_fixture::{enter, finish, position, settle};
use super::*;

#[test]
fn cp01_017_checks_both_named_cemetery_cards_and_keeps_meal() {
    for qualified in [false, true] {
        let mut initial = position("CP01-017");
        if qualified {
            initial["players"]["P1"]["zones"]["cemetery"] =
                json!([{"id":"verify","card":"CP01-021"},{"id":"taishin","card":"CP01-023"}]);
        }
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"carrot","card":"CSD01-032"}]);
        let mut game = game(&initial);
        enter(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["source"]["power"],
            if qualified { 5_i32 } else { 2_i32 }
        );
        assert!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走"))
        );
        meal_race::feed(&mut game, "CP01-017", 0, false);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["race"],
            json!(["carrot"])
        );
    }
}

#[test]
fn cp01_077_searches_mejiro_and_gains_its_named_cemetery_bonus() {
    let mut initial = position("CP01-077");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"festival","card":"CP01-031"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"mejiro","card":"CP01-077"},{"id":"wrong","card":"BP01-037"}]);
    let mut game = game(&initial);
    enter(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert!(
        state["P1"]["hand"]
            .as_array()
            .unwrap()
            .contains(&json!("mejiro"))
    );
    assert!(
        state["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("突進"))
    );
}

#[test]
fn cp02_082_unqualified_history_keeps_full_cost_and_crimson_damage_is_conditional() {
    let mut game = game(&position("CP02-082"));
    enter(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 0_i32);
    assert_eq!(state["P1"]["pp"]["current"], 8_i32);
}

#[test]
fn cp03_007_destroys_a_target_and_uses_its_actual_destroy_count() {
    let mut game = game(&position("CP03-007"));
    enter(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P2"]["leader"]["life"], 19_i32);
    assert!(
        state["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("ツインドライブ"))
    );
}

#[test]
fn cp03_028_named_grave_condition_draws_then_discards() {
    let mut initial = position("CP03-028");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"blaster","card":"CSD03a-003"}]);
    let mut game = game(&initial);
    enter(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 1);
    assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 1);
}

#[test]
fn cp03_072_whole_card_disallows_both_enemy_attack_targets() {
    let mut initial = position("CP03-072");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"CP03-072","state":{"entered_this_turn":false}}]);
    let mut game = game(&initial);
    for target in ["enemy", "P2.leader"] {
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":target}))
                .unwrap()
                .outcome,
            "cannot-attack"
        );
    }
}

#[test]
fn cp03_012_ride_buffs_then_reveals_its_printed_look_choice() {
    let number = "CP03-012";
    let mut initial = position(number);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"point","card":"CSD03a-017"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"CP03-011"},{"id":"middle","card":"BP01-037"},{"id":"tail","card":"BP01-037"}]);
    let mut game = game(&initial);
    let ability = game.internal_activated_sources("source").unwrap()[0].definition;
    let play = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, play);
    settle(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 3_i32);
    assert_eq!(state["objects"]["source"]["hp"], 3_i32);
    assert_eq!(state["P1"]["hand"], json!(["top"]));
    assert_eq!(state["P1"]["leader"]["life"], 12_i32);
}

#[test]
fn cp03_012_look_can_be_declined_and_an_unqualified_pick_does_not_heal() {
    for (top, take) in [("CP03-011", false), ("BP01-037", true)] {
        let mut initial = position("CP03-012");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP03-012"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"point","card":"CSD03a-017"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":top},{"id":"middle","card":"BP01-037"},{"id":"tail","card":"BP01-037"}]);
        let mut game = game(&initial);
        let ability = game.internal_activated_sources("source").unwrap()[0].definition;
        let activation = game
            .activate_internal_card("source", ability, PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, activation);
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("drive trigger");
        };
        let CardPlay::Awaiting(choice) = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap()
        else {
            panic!("optional private Look choice");
        };
        let InputDomain::Targets(domain) = &choice.domain else {
            panic!("private Look choice");
        };
        assert_eq!(domain.members.len(), 1);
        let answer = if take {
            vec![domain.members[0].handle]
        } else {
            vec![]
        };
        let resolution = game
            .answer_internal_card(choice.actor, choice.token, Answer::Targets(answer))
            .unwrap();
        finish(&mut game, resolution);
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["power"], 3_i32);
        assert_eq!(state["objects"]["source"]["hp"], 3_i32);
        assert_eq!(state["P1"]["leader"]["life"], 10_i32);
        assert_eq!(
            state["P1"]["hand"],
            if take { json!(["top"]) } else { json!([]) }
        );
        assert_eq!(
            state["P1"]["deck"],
            if take {
                json!(["middle", "tail"])
            } else {
                json!(["top", "middle", "tail"])
            }
        );
    }
}

#[test]
fn cp04_112_whole_ub_requires_distinct_names() {
    let mut initial = position("CP04-112");
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"goblin","card":"BP01-171"},{"id":"duplicate","card":"BP01-171"},{"id":"maid","card":"BP01-034"}]);
    let mut game = game(&initial);
    let entry = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, entry);
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("UB fanfare");
    };
    let CardPlay::Awaiting(target) = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap()
    else {
        panic!("cemetery targets");
    };
    let InputDomain::Targets(domain) = &target.domain else {
        panic!("formal targets");
    };
    assert!(domain.distinct_names);
    assert_eq!(domain.cardinality.maximum, Some(Nat(3)));
    let before = game.projection(View::Referee).unwrap();
    game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![0, 1]))
        .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    let resolution = game
        .answer_internal_card(target.actor, target.token, Answer::Targets(vec![0, 2]))
        .unwrap();
    finish(&mut game, resolution);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source", "goblin", "maid"]));
}

#[test]
fn cp04_112_observer_buffs_only_the_other_ub_holder() {
    let mut initial = position("CP04-089");
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"observer","card":"CP04-112"}]);
    let mut game = game(&initial);
    let before = game.projection(View::Referee).unwrap();
    enter(&mut game);
    let after = game.projection(View::Referee).unwrap();
    for stat in ["power", "hp"] {
        assert_eq!(
            after["objects"]["source"][stat].as_i64().unwrap(),
            before["objects"]["source"][stat].as_i64().unwrap() + 1
        );
        assert_eq!(
            after["objects"]["observer"][stat],
            before["objects"]["observer"][stat]
        );
    }
}

#[test]
fn cp02_082_real_evolution_reduces_cost_and_real_life_loss_enables_crimson() {
    let mut initial = position("CP02-082");
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"burn","card":"BP01-125"},{"id":"source","card":"CP02-082"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP02-118"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP02-119"}]);
    let mut game = game(&initial);
    game.internal_rule_action(
        &json!({"do":"evolve","source":"ally","evolve_card":"form","pay":{"pp":3_i32}}),
    )
    .unwrap();
    settle(&mut game);
    let play = game
        .play_internal_card("burn", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, play);
    settle(&mut game);
    let before = game.projection(View::Referee).unwrap();
    enter(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["pp"]["current"], 4_i32);
    assert_eq!(
        after["objects"]["enemy"]["damage"].as_i64().unwrap(),
        before["objects"]["enemy"]["damage"].as_i64().unwrap() + 3
    );
}
