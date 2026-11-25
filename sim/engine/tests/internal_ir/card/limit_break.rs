#![expect(
    clippy::panic,
    reason = "Synthetic activations must offer the expected target or pending question."
)]

use super::*;

#[path = "../../support/current_fixture.rs"]
mod current_fixture;
use sve_engine::catalog::SelectedEffects;
use sve_engine::game::internal::{InputDomain, InstructionOutcome};
use sve_engine::game::player::Viewer;
use sve_engine::game::player::events::{ActionKind, EventKind};
use sve_engine::game::player::observation::{ObservedZone, RelationshipKind};

fn setup_board(count: u8) -> Value {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower","state":{"damage":1}}]);
    initial["players"]["P1"]["zones"]["ex"] = json!(
        (1..=count)
            .map(|index| json!({"id":format!("m{index}"),"card":"unit-spell"}))
            .collect::<Vec<_>>()
    );
    initial
}

fn selection(treasure: Vec<Value>, follower: Vec<Value>) -> SelectedEffects {
    let (mut material, material_author) = current_fixture::author(
        "unit-spell",
        "11111111111111111111111111111111",
        "トレジャー・トークン",
        treasure,
    );
    material["faces"][0]["power"] = json!("-");
    material["faces"][0]["hp"] = json!("-");
    let (mut follower, follower_author) = current_fixture::author(
        "unit-follower",
        "22222222222222222222222222222222",
        "フォロワー",
        follower,
    );
    follower["faces"][0]["power"] = json!("2");
    follower["faces"][0]["hp"] = json!("3");
    follower["faces"][0]["traits"] = json!(["グラブル", "全属性", "〈Test・Group〉"]);
    Catalog::from_current_selection(
        &format!("{material}\n{follower}"),
        &json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string(),
        &json!({"documents":[],"authors":[material_author,follower_author]}),
    )
    .unwrap()
}

fn board(initial: &Value, treasure: Vec<Value>, follower: Vec<Value>) -> Game {
    Game::from_selected_effects(
        selection(treasure, follower),
        initial,
        &Value::Null,
        &Value::Null,
        "limit-break",
    )
    .unwrap()
}

fn activate_material(game: &mut Game, material: &str) -> Activation {
    let CardPlay::Awaiting(question) = game
        .activate_internal_card(material, AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("material chooses a field follower");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("formal target question");
    };
    assert_eq!(
        domain.members.len(),
        1,
        "the trait filter should leave one legal target"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    )
}

fn target_ability(trait_name: &str, steps: &Value) -> Value {
    json!({"activate":{
        "zone":"ex","cost":{"pp":0},
        "targets":[{"bind":"target","from":{"who":"self","zone":"field","kind":"follower","trait":trait_name},"n":1}],
        "do":steps,
    }})
}

fn element(trait_name: &str) -> Vec<Value> {
    vec![target_ability(
        trait_name,
        &json!([{"if":{
            "cond":{"all":[{"limit_broken":"target"},{"final_limit_break_possible":"target"}]},
            "then":[{"final_limit_break":{"on":"target"}}],
            "else":[{"limit_break":{"on":"target"}}],
        }}]),
    )]
}

fn follower_program(keyword: bool) -> Vec<Value> {
    let mut heads = vec![
        json!({"activate":{"cost":{"pp":0},"do":[{"move":{"ref":"this","to":"cemetery"}}]}}),
        json!({"auto":{"on":{"limit_broke":{"ref":"this"}},
            "do":[{"if":{"cond":{"limit_broken":"this"},"then":[{"life":{"who":"self","delta":1}}]}}]
        }}),
        json!({"auto":{"on":{"final_limit_broke":{"ref":"this"}},
            "do":[{"if":{"cond":{"final_limit_broken":"this"},"then":[{"life":{"who":"self","delta":2}}]}}]
        }}),
    ];
    if keyword {
        heads.push(json!({"keywords":["final_limit_break_possible"]}));
    }
    heads
}

fn drain_pending(game: &mut Game) -> usize {
    game.resume_internal_card().unwrap();
    let mut count = 0_usize;
    while let Some(question) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending limit-break trigger");
        };
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
        count = count.checked_add(1).unwrap();
    }
    count
}

fn assert_stats(game: &Game, attack: u8, life: u8, maximum: u8) {
    let view = game.projection(View::Referee).unwrap();
    let follower = &view["objects"]["s"];
    assert_eq!(
        [&follower["power"], &follower["hp"], &follower["max_hp"]],
        [&json!(attack), &json!(life), &json!(maximum)],
        "effective attack, remaining life and maximum life after the break"
    );
}

fn assert_public_attachments(game: &Game) {
    for perspective in [Viewer::P1, Viewer::P2] {
        let observed = game.observe(perspective).unwrap();
        let zones = &observed.players[0].zones;
        let materials = zones
            .iter()
            .find(|zone| zone.kind == ObservedZone::LimitBreak)
            .unwrap();
        let field = zones
            .iter()
            .find(|zone| zone.kind == ObservedZone::Field)
            .unwrap();
        assert_eq!(materials.count, 2, "both spent materials stay public");
        assert_eq!(
            materials.capacity, None,
            "attachments have no capacity limit"
        );
        assert_eq!(observed.relationships.len(), 2, "one link per material");
        for material in &materials.cards {
            assert_eq!(material.position, None, "the limit-break area is unordered");
            assert!(
                observed.relationships.iter().any(|link| {
                    link.kind == RelationshipKind::LimitBreak
                        && link.from == material.card.handle
                        && link.to == field.cards[0].card.handle
                }),
                "the material and its bearer must both be identifiable"
            );
        }
    }
}

fn assert_public_break_events(game: &mut Game) {
    game.advance_player_input();
    let breaks = game
        .events(Viewer::P2, None)
        .unwrap()
        .filter_map(|event| {
            let EventKind::Action(action) = &event.kind else {
                return None;
            };
            if !matches!(
                action.kind,
                ActionKind::LimitBreak | ActionKind::FinalLimitBreak
            ) {
                return None;
            }
            assert_eq!(
                action.player,
                Some(Viewer::P1),
                "the activator is the actor"
            );
            assert_eq!(
                action.subjects.len(),
                2,
                "the occurrence links two current objects"
            );
            assert!(
                action.subjects.iter().all(Option::is_some),
                "both participants are public"
            );
            Some(action.kind)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        breaks,
        vec![ActionKind::LimitBreak, ActionKind::FinalLimitBreak],
        "each successful instruction emits one event and the failed attempt emits none"
    );
}

fn run_element(trait_name: &str) {
    let mut game = board(&setup_board(3), element(trait_name), follower_program(true));
    game.advance_player_input();
    activate_material(&mut game, "m1");
    assert_stats(&game, 3, 2, 3);
    assert_eq!(
        drain_pending(&mut game),
        1,
        "the normal stage triggers once"
    );
    activate_material(&mut game, "m2");
    assert_stats(&game, 4, 3, 4);
    assert_eq!(
        drain_pending(&mut game),
        1,
        "the replacement final stage triggers once"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        23,
        "both triggers read their respective state and resolve"
    );
    activate_material(&mut game, "m3");
    assert_eq!(
        drain_pending(&mut game),
        0,
        "a repeated stage produces no trigger"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"],
        json!(["m3"]),
        "the failed instruction leaves its material in EX"
    );
    assert_public_attachments(&game);
    assert_public_break_events(&mut game);

    game.activate_internal_card("m1", AbilityId(1), PlayerId { seat: 1 })
        .unwrap_err();
    activate(&mut game, 1);
    assert_eq!(
        drain_pending(&mut game),
        0,
        "departure does not repeat either event"
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["P1"]["limit_break"],
        json!([]),
        "materials lose their field association"
    );
    assert_eq!(
        after["P1"]["cemetery"],
        json!(["s"]),
        "the bearer moves on its own"
    );
    assert!(
        after["objects"]["m1"].is_null(),
        "rule 11.12.1 erases the normal material"
    );
    assert!(
        after["objects"]["m2"].is_null(),
        "rule 11.12.1 erases the final material"
    );
}

#[test]
fn fire_element_limit_breaks_all_attributes() {
    run_element("火属性");
}
#[test]
fn water_element_limit_breaks_all_attributes() {
    run_element("水属性");
}
#[test]
fn earth_element_limit_breaks_all_attributes() {
    run_element("土属性");
}
#[test]
fn wind_element_limit_breaks_all_attributes() {
    run_element("風属性");
}
#[test]
fn light_element_limit_breaks_all_attributes() {
    run_element("光属性");
}
#[test]
fn dark_element_limit_breaks_all_attributes() {
    run_element("闇属性");
}

#[test]
fn gold_limit_breaks_twice_with_one_material_and_triggers_both_events() {
    let gold = vec![target_ability(
        "グラブル",
        &json!([
            {"limit_break":{"on":"target"}},
            {"if":{"cond":{"final_limit_break_possible":"target"},"then":[{"final_limit_break":{"on":"target"}}]}}
        ]),
    )];
    let mut game = board(&setup_board(1), gold, follower_program(true));
    let result = activate_material(&mut game, "m1");
    let breaks = result
        .progress
        .receipts
        .values()
        .filter_map(|receipt| {
            let InstructionOutcome::LimitBreak(Some(event)) = receipt else {
                return None;
            };
            Some(*event)
        })
        .collect::<Vec<_>>();
    assert_eq!(breaks.len(), 2);
    assert_eq!(breaks[0].material, breaks[1].material);
    assert_ne!(breaks[0].final_break, breaks[1].final_break);
    assert_stats(&game, 4, 3, 4);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["limit_break"],
        json!(["m1"])
    );
    assert_eq!(drain_pending(&mut game), 2);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        23
    );
}

#[test]
fn limit_break_requires_an_unused_stage_and_final_limit_break_requires_the_keyword() {
    let follower = follower_program(false);
    let mut game = board(&setup_board(2), element("火属性"), follower.clone());
    activate_material(&mut game, "m1");
    assert_eq!(drain_pending(&mut game), 1);
    activate_material(&mut game, "m2");
    assert_eq!(drain_pending(&mut game), 0);
    assert_stats(&game, 3, 2, 3);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"],
        json!(["m2"])
    );

    let final_only = vec![target_ability(
        "グラブル",
        &json!([
            {"final_limit_break":{"on":"target"}}
        ]),
    )];
    let mut game = board(&setup_board(1), final_only.clone(), follower);
    activate_material(&mut game, "m1");
    assert_eq!(drain_pending(&mut game), 0);
    assert_stats(&game, 2, 2, 3);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"],
        json!(["m1"])
    );

    let mut game = board(&setup_board(1), final_only, follower_program(true));
    activate_material(&mut game, "m1");
    assert_eq!(drain_pending(&mut game), 1);
    assert_stats(&game, 3, 3, 4);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        22
    );
}

#[test]
fn treasure_is_erased_after_moving_to_a_forbidden_zone() {
    let mut initial = setup_board(1);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"m2","card":"unit-spell"}]);
    Game::from_selected_effects(
        selection(vec![], vec![]),
        &initial,
        &Value::Null,
        &Value::Null,
        "invalid-treasure",
    )
    .unwrap_err();
    let mover = vec![json!({"activate":{
        "cost":{"pp":0},
        "targets":[{"bind":"target","from":{"who":"self","zone":"ex","kind":"treasure"},"n":1}],
        "do":[{"move":{"ref":"target","to":"cemetery"}}]
    }})];
    let mut game = board(&setup_board(1), vec![], mover);
    activate_material(&mut game, "s");
    assert_eq!(drain_pending(&mut game), 0);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["ex"], json!([]));
    assert_eq!(view["P1"]["cemetery"], json!([]));
    assert!(view["objects"]["m1"].is_null());
}

#[test]
fn illegal_limit_break_cleanup_discards_orphans_and_matches_the_bearers_controller() {
    let mut initial = setup_board(0);
    initial["players"]["P2"]["zones"]["limit_break"] = json!([
        {"id":"attached","card":"unit-spell","state":{"limit_break_of":"s","limit_broken":true}},
        {"id":"orphan","card":"unit-spell","state":{"limit_break_of":"missing","final_limit_broken":true}},
    ]);
    let mut game = board(
        &initial,
        element("光属性"),
        vec![json!({"activate":{"cost":{"pp":0},"do":[{"life":{"who":"self","delta":1}}]}})],
    );
    activate(&mut game, 1);
    assert_eq!(drain_pending(&mut game), 0);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["limit_break"], json!(["attached"]));
    assert_eq!(view["P2"]["limit_break"], json!([]));
    assert!(view["objects"]["orphan"].is_null());
    assert_stats(&game, 3, 2, 3);
}

#[test]
fn attribute_references_keep_other_types_and_angle_brackets_distinct() {
    for trait_name in ["〈Test・Group〉", "全属性"] {
        let mut game = board(&setup_board(1), element(trait_name), vec![]);
        activate_material(&mut game, "m1");
        assert_stats(&game, 3, 2, 3);
    }
    for trait_name in ["Test", "Group", "unrelated"] {
        let mut game = board(&setup_board(1), element(trait_name), vec![]);
        game.activate_internal_card("m1", AbilityId(1), PlayerId { seat: 1 })
            .unwrap_err();
    }
}

#[test]
fn limit_break_is_a_noop_if_its_selected_follower_has_left() {
    let token = target_ability(
        "グラブル",
        &json!([
            {"move":{"ref":"target","to":"cemetery"}},
            {"limit_break":{"on":"target"}}
        ]),
    );
    let mut game = board(&setup_board(1), vec![token], vec![]);
    let result = activate_material(&mut game, "m1");
    assert!(
        result
            .progress
            .receipts
            .values()
            .any(|receipt| { matches!(receipt, InstructionOutcome::LimitBreak(None)) })
    );
    assert_eq!(drain_pending(&mut game), 0);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["cemetery"], json!(["s"]));
    assert_eq!(view["P1"]["ex"], json!(["m1"]));
    assert_eq!(view["P1"]["limit_break"], json!([]));
}

#[test]
fn limit_break_commits_the_materials_ex_departure_for_other_observers() {
    let observer = json!({"auto":{
        "on":{"moved":{"from":{"who":"self","zone":"ex"},"during":{"turn":"self"}}},
        "aggregation":"batch","per_turn":1,"do":[{"life":{"who":"self","delta":{"event":"amount"}}}]
    }});
    let mut game = board(&setup_board(1), element("光属性"), vec![observer]);
    activate_material(&mut game, "m1");
    assert_eq!(drain_pending(&mut game), 1);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["leader"]["life"], 21);
    assert_eq!(after["P1"]["limit_break"], json!(["m1"]));
}

#[test]
fn limit_break_admission_requires_a_selected_target_and_a_treasure_source() {
    let missing_target = json!({"activate":{
        "zone":"ex","cost":{"pp":0},"do":[{"limit_break":{"on":"this"}}]
    }});
    lowerer()
        .lower(&equipment_played::tree(&missing_target, "unit-spell"))
        .unwrap_err();

    let mut invalid_source = target_ability(
        "グラブル",
        &json!([
            {"limit_break":{"on":"target"}}
        ]),
    );
    invalid_source["activate"]["zone"] = json!("field");
    let error = Game::from_selected_effects(
        selection(vec![], vec![invalid_source]),
        &setup_board(0),
        &Value::Null,
        &Value::Null,
        "invalid-limit-break-source",
    )
    .unwrap_err();
    assert!(error.to_string().contains("requires a treasure source"));
}

#[test]
fn all_attributes_counts_for_each_elemental_attack_reference() {
    let mut game = board(&setup_board(0), vec![], vec![]);
    game.internal_rule_action(&json!({"do":"attack","attacker":"s","target":"P2.leader"}))
        .unwrap();
    let view = game.projection(View::Referee).unwrap();
    let counters = &view["semantic_state"]["counters_this_turn"]["P1"];
    for attribute in [
        "火属性",
        "水属性",
        "土属性",
        "風属性",
        "光属性",
        "闇属性",
        "全属性",
    ] {
        assert_eq!(counters[format!("trait_attacks.{attribute}")], 1);
    }
}
