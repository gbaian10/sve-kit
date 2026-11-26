
use super::*;

const ACTOR: PlayerId = PlayerId { seat: 1 };

fn game_for(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "cp05-dragon",
    )
    .unwrap()
}

fn finish(game: &mut Game, mut play: Option<CardPlay>, picks: &[&str]) {
    loop {
        while let Some(CardPlay::Awaiting(question)) = play {
            let answer = match &question.domain {
                InputDomain::Targets(domain) => Answer::Targets(
                    domain
                        .members
                        .iter()
                        .filter(|member| {
                            member
                                .card
                                .as_deref()
                                .is_some_and(|card| picks.contains(&card))
                        })
                        .map(|member| member.handle)
                        .collect(),
                ),
                InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                    domain
                        .members
                        .iter()
                        .filter(|member| {
                            member
                                .card
                                .as_deref()
                                .is_some_and(|card| picks.contains(&card))
                        })
                        .map(|member| member.handle)
                        .collect(),
                ),
                InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                    domain.cards.iter().map(|card| card.handle).collect(),
                )),
                InputDomain::OptionalCost => Answer::OptionalCost(true),
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                _ => panic!("unexpected dragon effect question"),
            };
            play = Some(
                game.answer_internal_card(question.actor, question.token, answer)
                    .unwrap(),
            );
        }
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        play = next_pending(game);
        if play.is_none() {
            break;
        }
    }
}

fn next_pending(game: &mut Game) -> Option<CardPlay> {
    let question = game.internal_pending_choice().unwrap()?;
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected printed ability");
    };
    Some(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    )
}

fn play(game: &mut Game, picks: &[&str]) {
    let step = game.play_internal_card("source", ACTOR).unwrap();
    finish(game, Some(step), picks);
}

fn declare_evolution(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the card must evolve before its effect is tested"
    );
}

fn evolve(game: &mut Game, picks: &[&str]) {
    declare_evolution(game);
    finish(game, None, picks);
}

fn drawn_fire(printing: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"fire","card":"CP05-066"}, {"id":"dark","card":"CP05-065"}
    ]);
    let mut game = game_for(&initial);
    play(&mut game, &["CP05-066"]);
    game.projection(View::Referee).unwrap()
}

#[test]
fn cp05_058_discards_a_dark_card_and_draws_only_after_paying() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"CP05-058"}, {"id":"dark","card":"CP05-P42"},
        {"id":"fire","card":"CP05-066"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    let mut game = game_for(&initial);
    play(&mut game, &["CP05-P42"]);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["cemetery"], json!(["dark"]));
    assert_eq!(view["P1"]["hand"], json!(["fire", "drawn"]));
}

#[test]
fn cp05_059_evolution_recalls_a_different_low_cost_dark_follower() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP05-058"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"PR-645"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"same","card":"CP05-P42"}, {"id":"fire","card":"CP05-066"},
        {"id":"friend","card":"CP05-065"}
    ]);
    let mut game = game_for(&initial);
    declare_evolution(&mut game);
    let step = next_pending(&mut game).unwrap();
    let CardPlay::Awaiting(question) = &step else {
        panic!("the evolution ability must ask for a formal target");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected cemetery targets");
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card.as_deref(), Some("CP05-065"));
    finish(&mut game, Some(step), &["CP05-065"]);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"], json!(["source", "friend"]));
    assert_eq!(view["P1"]["cemetery"], json!(["same", "fire"]));
}

#[test]
fn cp05_062_evolves_for_one_pp_into_its_same_name_form() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP05-P46"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP05-063"}]);
    let mut game = game_for(&initial);
    evolve(&mut game, &[]);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["source"]["evolved_with"], "form");
    assert_eq!(view["P1"]["pp"]["current"], 9_u8);
    assert_token(&view, "CP05-T13");
}

#[test]
fn cp05_063_destroys_both_selected_followers_and_creates_the_light_material() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP05-062"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP05-P47"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"first","card":"BP01-037"}, {"id":"second","card":"CP05-066"},
        {"id":"amulet","card":"BP01-016"}
    ]);
    let mut game = game_for(&initial);
    evolve(&mut game, &["BP01-037", "CP05-066"]);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P2"]["field"], json!(["amulet"]));
    assert_eq!(view["P2"]["cemetery"], json!(["first", "second"]));
    assert_token(&view, "CP05-T13");
}

#[test]
fn cp05_065_observes_discard_by_own_dark_ability_and_leaves_the_card_in_cemetery() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"CP05-058"}, {"id":"friend","card":"CP05-P49"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"}, {"id":"second","card":"BP01-037"}
    ]);
    let mut game = game_for(&initial);
    play(&mut game, &["CP05-P49"]);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"], json!(["first", "second"]));
    assert_eq!(view["P1"]["cemetery"], json!(["friend"]));
    assert_token(&view, "CP05-T14");
}

#[test]
fn cp05_066_can_take_its_top_fire_card_and_preserves_the_remaining_deck() {
    let view = drawn_fire("CP05-P50");
    assert_eq!(view["P1"]["hand"], json!(["fire"]));
    assert_eq!(view["P1"]["deck"], json!(["dark"]));
}

#[test]
fn cp05_067_acts_its_evolution_target_and_adds_fire_material_on_attack() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP05-066"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP05-P51"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game_for(&initial);
    evolve(&mut game, &["BP01-037"]);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["acted"],
        true
    );
    game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
        .unwrap();
    finish(&mut game, None, &[]);
    assert_token(&game.projection(View::Referee).unwrap(), "CP05-T09");
}

#[test]
fn cp05_070_searches_for_zoey_at_ten_pp_after_a_nonlight_reveal() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP05-P54"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"dark","card":"CP05-065"}, {"id":"zoey","card":"CP05-055"}
    ]);
    let mut game = game_for(&initial);
    play(&mut game, &[]);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"], json!(["zoey"]));
    assert_eq!(view["P1"]["deck"], json!(["dark"]));
    assert_eq!(view["P1"]["ex"], json!([]));
}

#[test]
fn cp05_055_observes_both_release_stages_at_their_own_check_timing() {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":6_u8,"max":6_u8});
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"treasure","card":"CP05-T15"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"CP05-GBF35"}, {"id":"ally","card":"CP05-062"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game_for(&initial);
    let id = selection_with_rules().current["CP05-T15"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let action = game.activate_internal_card("treasure", id, ACTOR).unwrap();
    finish(&mut game, Some(action), &["CP05-GBF35", "BP01-037"]);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["pp"]["max"], 7_u8);
    assert_eq!(view["P1"]["pp"]["current"], 5_u8);
    assert_eq!(view["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(view["objects"]["ally"]["hp"], 7_u8);
}

fn assert_token(view: &Value, printing: &str) {
    let ex = view["P1"]["ex"].as_array().unwrap();
    let [token] = ex.as_slice() else {
        panic!("one printed elemental material in EX");
    };
    assert_eq!(
        view["objects"][token.as_str().unwrap()]["card"],
        printing,
        "the EX zone must contain the expected elemental material"
    );
}
