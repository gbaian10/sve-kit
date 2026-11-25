use super::ability_combinations::{
    activate_card, evolve, evolve_position, finish, loaded, play, position, view,
};
use super::*;

fn support(card: &str, zone: &str) -> Value {
    let mut initial = position(card, zone);
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"top","card":"CP05-110"},
        {"id":"second","card":"CP05-099"},
        {"id":"third","card":"BP01-171"},
        {"id":"fourth","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"reply","card":"BP01-171"}]);
    initial
}

fn token_card(game: &Game, index: usize) -> Value {
    let state = view(game);
    let id = state["P1"]["ex"][index].as_str().unwrap();
    state["objects"][id]["card"].clone()
}

#[test]
fn uno_quick_ability_acts_and_heals_for_one_pp() {
    let initial = support("CP05-SL21", "field");
    let mut game = loaded(&initial);
    activate_card(&mut game, "CP05-SL21", 4, &[]);
    let state = view(&game);
    assert_eq!(state["P1"]["leader"]["life"], 11_u8);
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    assert_eq!(state["objects"]["source"]["acted"], true);
}

#[test]
fn vira_enters_with_one_light_material() {
    let mut game = loaded(&support("CP05-093", "hand"));
    play(&mut game, &[]);
    assert_eq!(token_card(&game, 0), "CP05-T13");
}

#[test]
fn le_fiee_banishes_the_formal_target_and_creates_a_light_material() {
    let mut game = loaded(&support("CP05-095", "hand"));
    play(&mut game, &[]);
    assert_eq!(view(&game)["P2"]["banish"], json!(["enemy"]));
    assert_eq!(token_card(&game, 0), "CP05-T13");
}

#[test]
fn luminiera_reveals_the_top_light_card_and_only_increases_maximum_pp() {
    let mut initial = support("CP05-096", "hand");
    initial["players"]["P1"]["pp"] = json!({"current":3_u8,"max":5_u8});
    let mut game = loaded(&initial);
    play(&mut game, &[]);
    let state = view(&game);
    assert_eq!(state["P1"]["pp"], json!({"current":0_u8,"max":6_u8}));
    assert_eq!(state["P1"]["deck"][0], "top");
}

fn place_cost_five(initial: &mut Value) {
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"support","card":"CP05-105"}
    ]);
}

#[test]
fn tikoh_entry_heals_with_a_cost_five_granblue_follower() {
    let mut initial = support("CP05-097", "hand");
    place_cost_five(&mut initial);
    let mut game = loaded(&initial);
    play(&mut game, &[]);
    assert_eq!(view(&game)["P1"]["leader"]["life"], 12_u8);
    assert_eq!(token_card(&game, 0), "CP05-T13");
}

#[test]
fn tikoh_evolution_creates_light_material_without_healing_when_alone() {
    let mut initial = evolve_position("CP05-097", "CP05-098");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    let mut game = loaded(&initial);
    evolve(&mut game, 1);
    assert_eq!(view(&game)["P1"]["leader"]["life"], 10_u8);
    assert_eq!(token_card(&game, 0), "CP05-T13");
}

#[test]
fn noa_uses_current_maximum_pp_for_both_stats_after_paying_its_cost() {
    let mut initial = support("CP05-099", "hand");
    initial["players"]["P1"]["pp"] = json!({"current":5_u8,"max":6_u8});
    let mut game = loaded(&initial);
    play(&mut game, &[]);
    let state = view(&game);
    assert_eq!(state["objects"]["source"]["power"], 7_u8);
    assert_eq!(state["objects"]["source"]["hp"], 7_u8);
}

#[test]
fn petra_stops_only_the_targets_next_start_phase_stand() {
    let mut initial = support("CP05-100", "hand");
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"acted":true}},
        {"id":"untargeted","card":"BP01-037","state":{"acted":true}}
    ]);
    let mut game = loaded(&initial);
    play(&mut game, &[]);
    assert_eq!(token_card(&game, 0), "CP05-T12");
    attack_batch::end_turn(&mut game);
    let state = view(&game);
    assert_eq!(state["objects"]["enemy"]["acted"], true);
    assert_eq!(state["objects"]["untargeted"]["acted"], false);
}

#[test]
fn sophia_can_discard_itself_from_hand_to_draw_two() {
    let mut game = loaded(&support("CP05-101", "hand"));
    activate_card(&mut game, "CP05-101", 4, &[]);
    let state = view(&game);
    assert_eq!(state["P1"]["hand"], json!(["top", "second"]));
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P1"]["pp"]["current"], 7_u8);
}

#[test]
fn trou_creates_water_material_then_draws() {
    let mut game = loaded(&support("CP05-103", "hand"));
    play(&mut game, &[]);
    assert_eq!(token_card(&game, 0), "CP05-T10");
    assert_eq!(view(&game)["P1"]["hand"], json!(["top"]));
}

#[test]
fn sandalphon_enters_evolved_with_zero_sep_and_keeps_the_fanfare_heal() {
    let mut initial = support("CP05-105", "hand");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP05-106"}]);
    let mut game = loaded(&initial);
    play(&mut game, &[]);
    let state = view(&game);
    assert_eq!(state["objects"]["source"]["evolved_with"], "form");
    assert_eq!(state["P1"]["leader"]["life"], 12_u8);
}

#[test]
fn sandalphon_evolution_repeats_damage_with_three_cost_five_allies() {
    for (third_ally, expected_hp, expected_life) in [(false, 28_u8, 18_u8), (true, 26_u8, 16_u8)] {
        let mut initial = evolve_position("CP05-105", "CP05-106");
        let mut field = vec![
            json!({"id":"source","card":"CP05-105"}),
            json!({"id":"one","card":"CP05-095"}),
        ];
        if third_ally {
            field.push(json!({"id":"two","card":"CP05-105"}));
        }
        initial["players"]["P1"]["zones"]["field"] = json!(field);
        let mut game = loaded(&initial);
        evolve(&mut game, 1);
        let state = view(&game);
        assert_eq!(state["objects"]["enemy"]["hp"], expected_hp);
        assert_eq!(state["P2"]["leader"]["life"], expected_life);
    }
}

#[test]
fn gran_and_djeeta_pay_the_fanfare_with_a_granblue_card_to_draw_two() {
    let mut initial = support("CP05-107", "hand");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"CP05-107"},
        {"id":"offering","card":"CP05-110"},
        {"id":"other","card":"BP01-171"}
    ]);
    let mut game = loaded(&initial);
    play(&mut game, &[]);
    let state = view(&game);
    assert_eq!(state["P1"]["cemetery"], json!(["offering"]));
    assert_eq!(state["P1"]["hand"], json!(["other", "top", "second"]));
}

#[test]
fn vyrn_evolves_from_fanfare_with_gran_and_djeeta_present() {
    let mut initial = support("CP05-108", "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"partner","card":"CP05-107"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP05-109"}]);
    let mut game = loaded(&initial);
    play(&mut game, &[]);
    assert_eq!(view(&game)["objects"]["source"]["evolved_with"], "form");
}

#[test]
fn vyrn_evolution_puts_gold_in_ex_even_without_gran_and_djeeta() {
    let mut game = loaded(&evolve_position("CP05-108", "CP05-109"));
    evolve(&mut game, 1);
    assert_eq!(token_card(&game, 0), "CP05-T15");
}

#[test]
fn lyria_keeps_one_granblue_card_and_orders_the_others_below_an_unseen_card() {
    let mut game = loaded(&support("CP05-110", "hand"));
    play(&mut game, &[]);
    let state = view(&game);
    assert_eq!(state["P1"]["hand"], json!(["top"]));
    assert_eq!(state["P1"]["deck"], json!(["fourth", "second", "third"]));
}

#[test]
fn le_fiee_heals_one_or_three_at_the_end_phase_by_cost_five_allies() {
    for (allies, expected) in [(0_usize, 11_u8), (2, 13_u8)] {
        let mut initial = support("CP05-095", "field");
        let mut field = vec![json!({"id":"source","card":"CP05-095"})];
        field.extend((0..allies).map(|index| json!({"id":format!("ally{index}"),"card":"CP05-105"})));
        initial["players"]["P1"]["zones"]["field"] = json!(field);
        let mut game = loaded(&initial);
        game.internal_rule_action(&json!({"do":"end-phase"})).unwrap();
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("the printed end-phase trigger is pending")
        };
        let play = game
            .answer_internal_card(question.actor, question.token, Answer::Pending(options[0].handle))
            .unwrap();
        finish(&mut game, Some(play), &[]);
        assert_eq!(view(&game)["P1"]["leader"]["life"], expected);
    }
}

#[test]
fn luminiera_keeps_maximum_pp_when_the_revealed_card_is_not_light() {
    let mut initial = support("CP05-096", "hand");
    initial["players"]["P1"]["pp"] = json!({"current":3_u8,"max":5_u8});
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"BP01-171"}]);
    let mut game = loaded(&initial);
    play(&mut game, &[]);
    assert_eq!(view(&game)["P1"]["pp"], json!({"current":0_u8,"max":5_u8}));
}

#[test]
fn trou_activated_ability_acts_and_creates_a_water_material() {
    let mut game = loaded(&support("CP05-103", "field"));
    activate_card(&mut game, "CP05-103", 3, &[]);
    assert_eq!(token_card(&game, 0), "CP05-T10");
    assert_eq!(view(&game)["objects"]["source"]["acted"], true);
}

#[test]
fn tikoh_evolution_heals_two_with_a_cost_five_granblue_follower() {
    let mut initial = evolve_position("CP05-097", "CP05-098");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"CP05-097"},
        {"id":"support","card":"CP05-105"}
    ]);
    let mut game = loaded(&initial);
    evolve(&mut game, 1);
    assert_eq!(view(&game)["P1"]["leader"]["life"], 12_u8);
}
