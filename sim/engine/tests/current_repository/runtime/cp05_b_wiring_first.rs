#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "An unexpected decision invalidates the printed-card scenario."
)]

use super::*;

fn settle(game: &mut Game, mut play: Option<CardPlay>) {
    loop {
        while let Some(CardPlay::Awaiting(question)) = play {
            let answer = match &question.domain {
                InputDomain::Targets(domain) => Answer::Targets(
                    domain.members.iter().take(1).map(|member| member.handle).collect(),
                ),
                InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                    domain.members.iter().map(|member| member.handle).collect(),
                ),
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

fn play(game: &mut Game) {
    let step = game.play_internal_card("source", actor()).unwrap();
    settle(game, Some(step));
}

fn attack(game: &mut Game, target: &str) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":target}))
            .unwrap().outcome,
        "resolved",
    );
    settle(game, None);
}

fn evolution(initial: &mut Value, form: &str) {
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
}

fn evolve(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}))
            .unwrap().outcome,
        "resolved",
    );
    settle(game, None);
}

#[test]
fn tweyen_acts_two_original_cost_five_granblue_followers_to_enter_without_pp() {
    let mut initial = position_with_source("CP05-001", "hand");
    initial["players"]["P1"]["pp"]["current"] = json!(0_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"first","card":"CP05-U01"}, {"id":"second","card":"CP05-001"}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    let board = state(&game);
    assert!(board["P1"]["field"].as_array().unwrap().contains(&json!("source")));
    assert_eq!(board["objects"]["first"]["acted"], true);
    assert_eq!(board["objects"]["second"]["acted"], true);
    assert_eq!(board["P1"]["pp"]["current"], 0_u8);
}

#[test]
fn evolved_cucouroux_targets_one_enemy_follower_and_deals_two() {
    let mut initial = position_with_source("synthetic-evolution", "field");
    evolution(&mut initial, "CP05-010");
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"hp":6_u8,"max_hp":6_u8}}
    ]);
    let mut game = super_evolution_pp_cost::synthetic_base("CP05-010", &initial);
    evolve(&mut game);
    assert_eq!(state(&game)["objects"]["enemy"]["hp"], 4_u8);
}

#[test]
fn chloe_pays_a_dark_card_and_returns_it_from_the_cemetery_to_the_bottom() {
    let mut initial = position_with_source("CP05-012", "hand");
    initial["players"]["P1"]["zones"]["hand"].as_array_mut().unwrap()
        .push(json!({"id":"paid","card":"CP05-016"}));
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"drawn","card":"BP01-037"},{"id":"rest","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    assert_eq!(state(&game)["P1"]["hand"], json!(["drawn"]));
    assert_eq!(state(&game)["P1"]["cemetery"], json!(["paid"]));
    let step = activate_printed(&mut game, "CP05-012");
    settle(&mut game, Some(step));
    let board = state(&game);
    assert_eq!(board["P1"]["deck"], json!(["rest","paid"]));
    assert_eq!(board["objects"]["source"]["acted"], true);
}

#[test]
fn kou_and_you_move_the_revealed_card_to_the_printed_destination() {
    for (number, destination) in [("CP05-001", "field"), ("BP01-037", "hand")] {
        let mut initial = position_with_source("CP05-016", "field");
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":number}]);
        let mut game = game(&initial);
        attack(&mut game, "P2.leader");
        let board = state(&game);
        assert!(board["P1"][destination].as_array().unwrap().contains(&json!("top")));
        assert_eq!(board["P1"]["deck"], json!([]));
    }
}

#[test]
fn eahta_counts_pairs_of_face_up_earth_evolutions_for_his_discount() {
    let mut initial = position_with_source("CP05-020", "hand");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"first","card":"CP05-032","face_up":true},
        {"id":"second","card":"CP05-P24","face_up":true},
        {"id":"face-down","card":"CP05-032","face_up":false},
        {"id":"other-trait","card":"CP05-010","face_up":true}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    assert_eq!(state(&game)["P1"]["pp"]["current"], 4_u8);
}

#[test]
fn evolved_fiorito_starts_with_ward_assail_and_bane() {
    let mut initial = position_with_source("synthetic-evolution", "field");
    evolution(&mut initial, "CP05-032");
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}}
    ]);
    let mut game = super_evolution_pp_cost::synthetic_base("CP05-032", &initial);
    evolve(&mut game);
    let board = state(&game);
    for keyword in ["守護", "指定攻撃", "必殺"] {
        assert!(has_granted_keyword(&board, "source", keyword));
    }
    attack(&mut game, "enemy");
    pass_quick(&mut game);
    assert_eq!(state(&game)["P2"]["cemetery"], json!(["enemy"]));
}

#[test]
fn cumuyu_uses_assail_in_combat_then_deals_two_from_lastword() {
    let mut initial = position_with_source("CP05-T01", "field");
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"entered_this_turn":true});
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"power":3_u8,"hp":6_u8,"max_hp":6_u8}}
    ]);
    let mut game = game(&initial);
    attack(&mut game, "enemy");
    let quick = game.internal_quick_choice().unwrap().unwrap();
    let play = game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None)).unwrap();
    settle(&mut game, Some(play));
    assert_eq!(state(&game)["P2"]["leader"]["life"], 18_u8);
}

#[test]
fn fil_cast_from_ex_increases_only_the_formal_granblue_target() {
    let mut initial = position_with_source("CP05-T02", "ex");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"granblue","card":"CP05-012"}, {"id":"other","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    let before = state(&game);
    let step = game.play_internal_card("source", actor()).unwrap();
    let question = await_card_choice(step);
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("Fil formal target");
    };
    assert_eq!(domain.members.len(), 1);
    let step = game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![domain.members[0].handle])).unwrap();
    settle(&mut game, Some(step));
    let board = state(&game);
    for stat in ["power", "hp"] {
        assert_eq!(
            board["objects"]["granblue"][stat].as_i64().unwrap(),
            before["objects"]["granblue"][stat].as_i64().unwrap().saturating_add(2),
        );
    }
}
