use super::ability_combinations::{evolve_position, finish, loaded, play, position, view};
use super::*;

#[test]
fn evolved_aegis_prevents_damage_through_the_next_opposing_turn() {
    let mut initial = evolve_position("BP02-SL16", "BP02-SL17");
    initial["players"]["P2"]["zones"]["field"][0]["state"]["power"] = json!(1_u8);
    for player in ["P1", "P2"] {
        initial["players"][player]["zones"]["deck"] =
            json!([{"id":format!("{player}-draw"),"card":"BP01-171"}]);
    }
    let mut game = loaded(&initial);
    let definition = selection().current["BP02-SL16"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == 1)
        .unwrap()
        .node
        .id;
    game.internal_rule_action(&json!({
        "do":"evolve","source":"source","evolve_card":"form",
        "definition":definition.0,"pay":{"pp":2_u8}
    }))
    .unwrap();
    assert_eq!(view(&game)["objects"]["source"]["evolved_with"], "form");
    finish(&mut game, None, &[]);

    let attack = |battle: &mut Game, from: &str, to: &str| {
        battle.internal_rule_action(&json!({"do":"attack","attacker":from,"target":to}))
            .unwrap();
        finish(battle, None, &[]);
    };
    attack(&mut game, "source", "enemy");
    assert_eq!(view(&game)["objects"]["source"]["damage"], 0_u8);

    game.internal_rule_action(&json!({"do":"end-phase"})).unwrap();
    finish(&mut game, None, &[]);
    attack(&mut game, "enemy", "source");
    assert_eq!(view(&game)["objects"]["source"]["damage"], 0_u8);

    game.internal_rule_action(&json!({"do":"end-phase"})).unwrap();
    finish(&mut game, None, &[]);
    attack(&mut game, "source", "enemy");
    assert_eq!(view(&game)["objects"]["source"]["damage"], 1_u8);
}

#[test]
fn nephthys_destroys_the_followers_it_put_into_play() {
    let mut initial = position("BP08-SL15", "hand");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"present","card":"BP01-110"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-110"},
        {"id":"second","card":"BP01-105"},
        {"id":"leftover","card":"BP01-171"}
    ]);
    let mut game = loaded(&initial);
    play(&mut game, &[]);
    let state = view(&game);
    assert_eq!(state["P1"]["field"], json!(["present", "source"]));
    let cemetery = state["P1"]["cemetery"].as_array().unwrap();
    assert!(cemetery.contains(&json!("first")));
    assert!(cemetery.contains(&json!("second")));
    assert_eq!(state["P1"]["deck"], json!(["leftover"]));
}

#[test]
fn kuon_reveals_its_own_onmyoji_as_an_optional_play_cost() {
    let mut initial = position("BP15-SL13", "hand");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP15-SL13"},
        {"id":"first","card":"BP15-046"},
        {"id":"second","card":"BP15-046"},
        {"id":"unrelated","card":"BP01-171"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"drawn","card":"BP01-171"}]);
    let mut game = loaded(&initial);
    play(&mut game, &[]);
    let state = view(&game);
    assert_eq!(state["P1"]["field"], json!(["source"]));
    assert_eq!(state["P1"]["pp"]["current"], 6_u8);
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 3);
}

#[test]
fn rhino_repeats_after_successfully_milling_a_namesake() {
    let mut initial = position("BP05-SL12", "hand");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"rhino","card":"BP05-054"},
        {"id":"one","card":"BP01-171"},
        {"id":"two","card":"BP01-171"},
        {"id":"three","card":"BP01-171"},
        {"id":"four","card":"BP01-171"}
    ]);
    let mut game = loaded(&initial);
    play(&mut game, &[]);
    let state = view(&game);
    assert_eq!(state["objects"]["source"]["power"], 6_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["rhino", "one", "two", "three", "four"]));
    assert_eq!(state["P1"]["deck"], json!([]));
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
}

#[test]
fn mimis_target_limit_uses_the_declared_amulets() {
    let mut initial = evolve_position("CP04-103", "CP04-104");
    initial["turn"]["elapsed_turns"] = json!({"P1":8_u8,"P2":7_u8});
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title"] = json!("プリンセスコネクト！Re:Dive");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"CP04-103"},
        {"id":"amulet","card":"CP03-021"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-173"},
        {"id":"other","card":"BP01-171"}
    ]);
    let mut game = loaded(&initial);
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}}),
    )
    .unwrap();
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending evolve trigger");
    };
    let CardPlay::Awaiting(payment) = game
        .answer_internal_card(question.actor, question.token, Answer::Pending(options[0].handle))
        .unwrap()
    else {
        panic!("head payment");
    };
    let CardPlay::Awaiting(materials) = game
        .answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(true))
        .unwrap()
    else {
        panic!("material declaration");
    };
    let InputDomain::CostMaterials(amulets) = &materials.domain else {
        panic!("amulet material domain");
    };
    let CardPlay::Awaiting(targets) = game
        .answer_internal_card(materials.actor, materials.token, Answer::CostMaterials(vec![amulets.members[0].handle]))
        .unwrap()
    else {
        panic!("dependent target declaration");
    };
    let InputDomain::Targets(domain) = &targets.domain else {
        panic!("follower target domain");
    };
    assert_eq!(domain.members.len(), 2);
    assert_eq!(domain.cardinality.maximum, Some(Nat(1)));
    let enemy = domain.members.iter().find(|member| member.card.as_deref() == Some("BP01-173")).unwrap();
    let play = game
        .answer_internal_card(targets.actor, targets.token, Answer::Targets(vec![enemy.handle]))
        .unwrap();
    finish(&mut game, Some(play), &[]);
    let state = view(&game);
    assert_eq!(state["objects"]["amulet"]["acted"], true);
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(state["objects"]["other"]["damage"], 0_u8);
}

#[test]
fn earthen_fist_pays_earth_rite_after_the_first_hit() {
    let mut initial = position("BP14-050", "hand");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"soil","card":"BP01-075","state":{"counters":{"stack_counter":1_u8}}}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-173"}]);
    let mut game = loaded(&initial);
    let CardPlay::Awaiting(targets) = game.play_internal_card("source", PlayerId { seat: 1 }).unwrap() else {
        panic!("spell's formal target");
    };
    let InputDomain::Targets(domain) = targets.domain else {
        panic!("follower target domain");
    };
    let CardPlay::Awaiting(earth) = game
        .answer_internal_card(targets.actor, targets.token, Answer::Targets(vec![domain.members[0].handle]))
        .unwrap()
    else {
        panic!("resolution payment");
    };
    assert_eq!(view(&game)["objects"]["enemy"]["damage"], 5_u8);
    let InputDomain::Targets(stacks) = earth.domain else {
        panic!("optional Earth Rite material");
    };
    assert_eq!(stacks.cardinality.minimum, Nat(0));
    assert_eq!(stacks.members[0].card.as_deref(), Some("BP01-075"));
    let play = game.answer_internal_card(earth.actor, earth.token, Answer::Targets(vec![stacks.members[0].handle])).unwrap();
    finish(&mut game, Some(play), &[]);
    let state = view(&game);
    assert_eq!(state["P1"]["field"], json!([]));
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
}

#[test]
fn lance_lizard_triggers_only_once_when_chosen_by_two_abilities() {
    let mut initial = position("ETD01-011", "field");
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"first","card":"BP01-T13"},
        {"id":"second","card":"BP01-T13"}
    ]);
    let mut game = loaded(&initial);
    for spell in ["first", "second"] {
        let play = game.play_internal_card(spell, PlayerId { seat: 1 }).unwrap();
        finish(&mut game, Some(play), &[]);
    }
    let state = view(&game);
    assert_eq!(state["objects"]["source"]["power"], 8_u8);
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
}

#[test]
fn morton_lets_the_opponent_skip_only_the_next_rule_draw() {
    let mut initial = position("BP05-006", "field");
    initial["players"]["P2"]["zones"]["deck"] =
        json!([{"id":"top","card":"BP01-171"}]);
    initial["players"]["P2"]["pp"] = json!({"current":2_u8,"max":3_u8});
    let mut game = loaded(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"})).unwrap();
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = pending.domain else {
        panic!("end-phase trigger");
    };
    let CardPlay::Awaiting(choice) = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(options[0].handle))
        .unwrap()
    else {
        panic!("resolution mode question");
    };
    assert_eq!(choice.actor, PlayerId { seat: 2 });
    let result = game.answer_internal_card(choice.actor, choice.token, Answer::Modes(vec![ModeId(1)])).unwrap();
    finish(&mut game, Some(result), &[]);
    let state = view(&game);
    assert_eq!(state["P2"]["deck"], json!(["top"]));
    assert_eq!(state["P2"]["hand"], json!([]));
    assert_eq!(state["P2"]["pp"]["max"], 4_u8);
}
