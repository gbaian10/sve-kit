#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "An unexpected decision invalidates the printed-card scenario."
)]

use super::*;

fn settle(game: &mut Game, mut play: Option<CardPlay>, modes: &[ModeId]) {
    loop {
        while let Some(CardPlay::Awaiting(question)) = play {
            let answer = match &question.domain {
                InputDomain::Targets(domain) => Answer::Targets(
                    domain.members.iter().take(1).map(|member| member.handle).collect(),
                ),
                InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                    domain.members.iter().map(|member| member.handle).collect(),
                ),
                InputDomain::Modes { .. } => Answer::Modes(modes.to_vec()),
                InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                    domain.cards.iter().take(1).map(|card| card.handle).collect(),
                )),
                InputDomain::OptionalCost => Answer::OptionalCost(true),
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                _ => panic!("unexpected printed-card choice"),
            };
            play = Some(game.answer_internal_card(question.actor, question.token, answer).unwrap());
        }
        finish_checks(game);
        let Some(question) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending printed ability");
        };
        play = Some(game.answer_internal_card(question.actor, question.token, Answer::Pending(options[0].handle)).unwrap());
    }
}

fn play(game: &mut Game, modes: &[ModeId]) {
    let step = game.play_internal_card("source", actor()).unwrap();
    settle(game, Some(step), modes);
}

fn evolved(initial: &mut Value, form: &str, super_evolve: bool) -> Game {
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["sep"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    let mut game = game(initial);
    assert_eq!(
        game.internal_rule_action(&json!({
            "do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8,"sep":u8::from(super_evolve)}
        }))
        .unwrap().outcome,
        "resolved",
        "the base enters the requested printed evolution for one PP",
    );
    settle(&mut game, None, &[]);
    game
}

fn has_ex(game: &Game, card: &str) -> bool {
    let board = state(game);
    board["P1"]["ex"].as_array().unwrap().iter().any(|id| {
        board["objects"][id.as_str().unwrap()]["card"] == card
    })
}

#[test]
fn ewiyar_places_wind_material_in_ex_on_entry() {
    let initial = position_with_source("CP05-003", "hand");
    let mut game = game(&initial);
    play(&mut game, &[]);
    assert!(has_ex(&game, "CP05-T12"));
}

#[test]
fn evolved_ewiyar_draws_twice_and_places_both_materials_on_super_evolution() {
    let mut initial = position_with_source("CP05-003", "field");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"}]);
    let game = evolved(&mut initial, "CP05-004", true);
    assert!(has_ex(&game, "CP05-T12"));
    assert!(has_ex(&game, "CP05-T15"));
    assert_eq!(state(&game)["P1"]["hand"], json!(["first","second"]));
}

#[test]
fn evolved_anthuria_damages_the_target_and_places_fire_material_in_ex() {
    let mut initial = position_with_source("synthetic-evolution", "field");
    initial["players"]["P1"]["zones"]["evolve_deck"] =
        json!([{"id":"form","card":"CP05-014"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":6_u8,"max_hp":6_u8}}]);
    let mut game = super_evolution_pp_cost::synthetic_base("CP05-014", &initial);
    assert_eq!(
        game.internal_rule_action(&json!({
            "do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}
        })).unwrap().outcome,
        "resolved",
        "Anthuria's evolved face enters after one PP",
    );
    settle(&mut game, None, &[]);
    assert_eq!(state(&game)["objects"]["enemy"]["hp"], 3_u8);
    assert!(has_ex(&game, "CP05-T09"));
}

#[test]
fn lu_woh_places_light_material_in_ex_on_entry() {
    let initial = position_with_source("CP05-021", "hand");
    let mut game = game(&initial);
    play(&mut game, &[]);
    assert!(has_ex(&game, "CP05-T13"));
}

#[test]
fn narmaya_can_trade_a_granblue_card_for_dark_material_and_a_draw() {
    let mut initial = position_with_source("CP05-023", "hand");
    initial["players"]["P1"]["zones"]["hand"].as_array_mut().unwrap()
        .push(json!({"id":"paid","card":"CP05-025"}));
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game, &[ModeId(2)]);
    assert_eq!(state(&game)["P1"]["hand"], json!(["drawn"]));
    assert_eq!(state(&game)["P1"]["cemetery"], json!(["paid"]));
    assert!(has_ex(&game, "CP05-T14"));
}

#[test]
fn evolved_narmaya_damages_one_enemy_and_creates_gold_brick() {
    let mut initial = position_with_source("CP05-023", "field");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":6_u8,"max_hp":6_u8}}]);
    let game = evolved(&mut initial, "CP05-024", false);
    assert_eq!(state(&game)["objects"]["enemy"]["hp"], 2_u8);
    assert!(has_ex(&game, "CP05-T15"));
}

#[test]
fn feather_can_decline_ward_posture_and_still_create_light_material() {
    let initial = position_with_source("CP05-035", "hand");
    let mut game = game(&initial);
    play(&mut game, &[]);
    assert_eq!(state(&game)["objects"]["source"]["acted"], false);
    assert!(has_ex(&game, "CP05-T13"));
}

fn knights() -> Value {
    let mut initial = position_with_source("CP05-026", "hand");
    initial["players"]["P1"]["leader"]["life"] = json!(15_u8);
    initial["players"]["P1"]["zones"]["hand"].as_array_mut().unwrap()
        .push(json!({"id":"old","card":"BP01-037"}));
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"acted":true,"hp":6_u8,"max_hp":6_u8}}
    ]);
    initial
}

#[test]
fn dragon_knights_pays_two_more_to_choose_four_modes() {
    let mut game = game(&knights());
    play(&mut game, &[ModeId(1), ModeId(2), ModeId(3), ModeId(4)]);
    let board = state(&game);
    assert_eq!(board["P1"]["pp"]["current"], 3_u8);
    assert_eq!(board["P1"]["field"].as_array().unwrap().len(), 4);
}

#[test]
fn lancelot_creates_water_material_and_exchanges_one_hand_card() {
    let mut game = game(&knights());
    play(&mut game, &[ModeId(1)]);
    let board = state(&game);
    assert!(has_ex(&game, "CP05-T10"));
    assert_eq!(board["P1"]["hand"].as_array().unwrap().len(), 1);
    assert_eq!(board["P1"]["deck"], json!([]));
}

#[test]
fn vane_creates_wind_material_and_heals_three() {
    let mut game = game(&knights());
    play(&mut game, &[ModeId(2)]);
    assert!(has_ex(&game, "CP05-T12"));
    assert_eq!(state(&game)["P1"]["leader"]["life"], 18_u8);
}

#[test]
fn siegfried_destroys_an_acted_follower_and_creates_earth_material() {
    let mut game = game(&knights());
    play(&mut game, &[ModeId(3)]);
    assert!(has_ex(&game, "CP05-T11"));
    assert_eq!(state(&game)["P2"]["cemetery"], json!(["enemy"]));
}

#[test]
fn percival_deals_four_and_creates_fire_material() {
    let mut game = game(&knights());
    play(&mut game, &[ModeId(4)]);
    assert!(has_ex(&game, "CP05-T09"));
    assert_eq!(state(&game)["objects"]["enemy"]["hp"], 2_u8);
}

fn gift(game: &mut Game, card: &str, mode: ModeId) {
    let step = game.activate_internal_card("material", activated(card), actor()).unwrap();
    settle(game, Some(step), &[mode]);
}

fn attack_leader(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"})).unwrap().outcome,
        "resolved",
        "Zeta can declare the leader attack after the material resolves",
    );
    settle(game, None, &[]);
}

#[test]
fn seofon_limit_break_draws_two_and_puts_one_back_on_the_bottom() {
    let mut initial = position_with_source("CP05-019", "field");
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"material","card":"CP05-T12"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"},{"id":"rest","card":"BP01-037"}]);
    let mut game = game(&initial);
    gift(&mut game, "CP05-T12", ModeId(1));
    let board = state(&game);
    assert_eq!(board["P1"]["hand"].as_array().unwrap().len(), 1);
    assert_eq!(board["P1"]["deck"].as_array().unwrap().len(), 2);
    assert_eq!(board["P1"]["deck"][0], "rest");
    assert_ne!(board["P1"]["deck"][1], board["P1"]["hand"][0]);
}


#[test]
fn zeta_gains_a_targeted_attack_ability_after_the_first_limit_break() {
    let mut initial = position_with_source("CP05-025", "field");
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"material","card":"CP05-T09"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    gift(&mut game, "CP05-T09", ModeId(1));
    attack_leader(&mut game);
    assert_eq!(state(&game)["P2"]["cemetery"], json!(["enemy"]));
}

#[test]
fn beatrix_can_search_zeta_onto_the_field_on_limit_break() {
    let mut initial = position_with_source("CP05-029", "field");
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"material","card":"CP05-T11"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"zeta","card":"CP05-P17"},{"id":"decoy","card":"CP05-029"}]);
    let mut game = game(&initial);
    gift(&mut game, "CP05-T11", ModeId(1));
    assert!(state(&game)["P1"]["field"].as_array().unwrap().contains(&json!("zeta")));
}
