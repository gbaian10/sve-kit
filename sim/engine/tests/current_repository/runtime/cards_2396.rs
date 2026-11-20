use super::*;

#[test]
fn phantom_blaster_overlord_replaces_original_pp_with_its_named_follower() {
    let mut initial = position_with_source("CP03-083", "hand");
    initial["players"]["P1"]["pp"] = json!({"current": 1, "max": 1});
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"fee","card":"CP03-084"}, {"id":"other","card":"CP03-086"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!(cards("paladin", "CP03-086", 15));
    let mut game = game(&initial);
    let play = game.play_internal_card("source", actor()).unwrap();
    let materials = accept(&mut game, play);
    let entered = choose_card_by_number(&mut game, materials, "CP03-084");
    assert!(matches!(accept(&mut game, entered), CardPlay::Entered(_)));

    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["pp"]["current"], 1_i64);
    assert!(after["P1"]["cemetery"].as_array().unwrap().contains(&json!("fee")));
    assert_eq!(after["P1"]["field"], json!(["other", "source"]));
}

#[test]
fn dragon_mountain_spell_pays_two_spring_counters_before_resolving() {
    let mut initial = position_with_source("BP14-063", "hand");
    initial["players"]["P1"]["pp"] = json!({"current": 2, "max": 2});
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"pool","card":"BP14-059","state":{"counters":{"神湯カウンター":2_u8}}},
        {"id":"shallow","card":"BP14-059","state":{"counters":{"神湯カウンター":1_u8}}}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!(cards("draw", "CP03-086", 1));
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"target","card":"CP03-086"},
        {"id":"foreign","card":"BP14-059","state":{"counters":{"神湯カウンター":3_u8}}}
    ]);
    let mut game = game(&initial);
    let play = game.play_internal_card("source", actor()).unwrap();
    let target = accept(&mut game, play);
    let material = await_card_choice(choose_card_by_number(&mut game, target, "CP03-086"));
    let InputDomain::CostMaterials(domain) = &material.domain else {
        panic!("exchange requires a single holder");
    };
    assert_eq!(domain.members.len(), 1_usize);
    let completed = game
        .answer_internal_card(
            material.actor,
            material.token,
            Answer::CostMaterials(vec![domain.members[0].handle]),
        )
        .unwrap();
    assert!(matches!(completed, CardPlay::Resolved(_)));

    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["pp"]["current"], 0_i64);
    assert_eq!(after["objects"]["pool"]["counters"]["神湯カウンター"], 0_u8);
    assert_eq!(after["objects"]["shallow"]["counters"]["神湯カウンター"], 1_u8);
    assert_eq!(after["objects"]["foreign"]["counters"]["神湯カウンター"], 3_u8);
    assert_eq!(after["P2"]["cemetery"], json!(["target"]));
    assert_eq!(after["P1"]["hand"], json!(["draw0"]));
}
