use super::*;
use sve_engine::game::player::SubmissionKind;

#[test]
fn phantom_blaster_overlord_replaces_original_pp_with_its_named_follower() {
    let mut initial = position_with_source("CP03-083", "hand");
    initial["players"]["P1"]["pp"] = json!({"current": 1_u8, "max": 1_u8});
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"fee","card":"CP03-084","state":{"evolved":true,"evolved_with":"form"}},
        {"id":"other","card":"CP03-086"}
    ]);
    initial["players"]["P1"]["zones"]["evolution"] = json!([{"id":"form","card":"CP03-085"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!(cards("paladin", "CP03-086", 15));
    let mut game = game(&initial);
    let play = game.play_internal_card("source", actor()).unwrap();
    let materials = accept(&mut game, play);
    let entered = choose_card_by_number(&mut game, materials, "CP03-084");
    assert!(matches!(accept(&mut game, entered), CardPlay::Entered(_)));

    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["pp"]["current"], 1_i64);
    assert!(
        after["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("fee"))
    );
    assert_eq!(after["P1"]["field"], json!(["other", "source"]));
    assert_eq!(after["P1"]["evolve_deck"], json!(["form"]));
    assert_eq!(after["P1"]["evolution"], json!([]));
}

#[test]
fn dragon_mountain_spell_pays_two_spring_counters_before_resolving() {
    let mut initial = position_with_source("BP14-063", "hand");
    initial["players"]["P1"]["pp"] = json!({"current": 2_u8, "max": 2_u8});
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
    assert_eq!(
        after["objects"]["shallow"]["counters"]["神湯カウンター"],
        1_u8
    );
    assert_eq!(
        after["objects"]["foreign"]["counters"]["神湯カウンター"],
        3_u8
    );
    assert_eq!(after["P2"]["cemetery"], json!(["target"]));
    assert_eq!(after["P1"]["hand"], json!(["draw0"]));
}

#[test]
fn new_generations_pays_nine_disjoint_cards_across_three_traits() {
    let mut initial = position_with_source("CP02-SL19a", "hand");
    initial["players"]["P1"]["pp"] = json!({"current": 1_u8, "max": 1_u8});
    let cemetery = [
        cards("cute", "CP02-014", 4),
        cards("cool", "CP02-100", 3),
        cards("passion", "CP02-101", 2),
        cards("all", "CP02-104", 1),
    ]
    .concat();
    initial["players"]["P1"]["zones"]["cemetery"] = json!(cemetery);
    let mut game = game(&initial);
    let play = game.play_internal_card("source", actor()).unwrap();
    let materials = await_card_choice(accept(&mut game, play));
    let InputDomain::CostMaterials(domain) = &materials.domain else {
        panic!("exchange requires the full disjoint set");
    };
    let handles = |exclude| {
        domain
            .members
            .iter()
            .filter(|member| member.handle != exclude)
            .map(|member| member.handle)
            .collect()
    };
    let overlap = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("CP02-104"))
        .unwrap()
        .handle;
    let before_payment = game.projection(View::Referee).unwrap();
    game.answer_internal_card(
        materials.actor,
        materials.token,
        Answer::CostMaterials(handles(overlap)),
    )
    .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before_payment);

    let spare = domain
        .members
        .iter()
        .rfind(|member| member.card.as_deref() == Some("CP02-014"))
        .unwrap()
        .handle;
    let entered = game
        .answer_internal_card(
            materials.actor,
            materials.token,
            Answer::CostMaterials(handles(spare)),
        )
        .unwrap();
    assert!(matches!(accept(&mut game, entered), CardPlay::Entered(_)));
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["pp"]["current"], 0_i64);
    assert_eq!(after["P1"]["cemetery"].as_array().unwrap().len(), 1_usize);
    assert_eq!(after["P1"]["banish"].as_array().unwrap().len(), 9_usize);
    assert_eq!(after["P1"]["field"], json!(["source"]));
}

#[test]
fn blazing_core_evolves_for_one_pp_and_two_named_cemetery_materials() {
    let mut initial = evolution_payment::position("CP03-066", "CP03-067");
    initial["players"]["P1"]["pp"] = json!({"current": 1_u8, "max": 1_u8});
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"claw","card":"CP03-077"},
        {"id":"tail","card":"CP03-080"},
        {"id":"extra","card":"CP03-086"}
    ]);
    let mut game = evolution_payment::game(&initial);
    assert_eq!(
        player_evolution::evolve(&mut game, false),
        SubmissionKind::Applied
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["pp"]["current"], 0_i64);
    assert_eq!(after["P1"]["cemetery"], json!(["extra"]));
    assert_eq!(after["P1"]["banish"].as_array().unwrap().len(), 2_usize);
    assert_eq!(after["objects"]["source"]["evolved_with"], "form");
}
