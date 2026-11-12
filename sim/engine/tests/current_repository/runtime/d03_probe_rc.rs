#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real-card probes."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn loaded(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "d03-probe",
    )
    .unwrap()
}

fn source(number: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    initial
}

fn ability(number: &str, line: u32) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.source.line == line)
        .unwrap()
        .node
        .id
}

fn settle(game: &mut Game, mut step: Option<CardPlay>) {
    for _ in 0_u8..80_u8 {
        if let Some(CardPlay::Awaiting(q)) = step.take() {
            let answer = match &q.domain {
                InputDomain::Pending(options) => Answer::Pending(options[0].handle),
                InputDomain::Targets(d) => Answer::Targets(
                    d.members
                        .iter()
                        .take(usize::try_from(d.cardinality.maximum.unwrap_or(Nat(1)).0).unwrap())
                        .map(|m| m.handle)
                        .collect(),
                ),
                InputDomain::CostMaterials(d) => Answer::CostMaterials(
                    d.members
                        .iter()
                        .take(usize::try_from(d.cardinality.minimum.0).unwrap())
                        .map(|m| m.handle)
                        .collect(),
                ),
                InputDomain::Order(cards) => {
                    Answer::Order(cards.iter().map(|m| m.handle).collect())
                }
                InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(2)]),
                InputDomain::Integer(bounds) => Answer::Integer(Nat(2
                    .min(bounds.maximum.unwrap_or(Nat(2)).0)
                    .max(bounds.minimum.0))),
                InputDomain::OptionalCost => Answer::OptionalCost(true),
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                InputDomain::Search(d) => {
                    assert!(d.groups.is_none(), "probe expects an ungrouped search");
                    Answer::Search(SearchAnswer::Ungrouped(
                        d.cards
                            .iter()
                            .take(
                                usize::try_from(
                                    d.cardinality.maximum.unwrap_or(d.cardinality.minimum).0,
                                )
                                .unwrap(),
                            )
                            .map(|m| m.handle)
                            .collect(),
                    ))
                }
                _ => panic!("unexpected probe decision"),
            };
            step = Some(game.answer_internal_card(q.actor, q.token, answer).unwrap());
            continue;
        }
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
            continue;
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("expected pending ability");
        };
        step = Some(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
    }
    panic!("probe did not settle");
}

fn play(game: &mut Game) {
    let step = game.play_internal_card("source", PLAYER).unwrap();
    settle(game, Some(step));
}
fn activate(game: &mut Game, number: &str, line: u32) {
    let step = game
        .activate_internal_card("source", ability(number, line), PLAYER)
        .unwrap();
    settle(game, Some(step));
}
fn state(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()
}

#[test]
fn bp01_001_recovers_pp_for_real_ex_rose_strikes() {
    let mut initial = source("BP01-001", "field");
    initial["players"]["P1"]["pp"]["current"] = json!(0_u8);
    initial["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"rose1","card":"BP01-T01"},{"id":"rose2","card":"BP01-T01"}]);
    let mut game = loaded(&initial);
    activate(&mut game, "BP01-001", 2);
    assert_eq!(state(&game)["P1"]["pp"]["current"], 2_u8);
    game.activate_internal_card("source", ability("BP01-001", 2), PLAYER)
        .expect_err("the printed payment or per-turn quota must reject this activation");
}

#[test]
fn bp01_072_moves_two_observed_spells_to_distinct_destinations() {
    let mut initial = source("BP01-072", "hand");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"spell1","card":"SD03-015"},{"id":"spell2","card":"SD03-015"},{"id":"rest1","card":"BP01-171"},{"id":"rest2","card":"BP01-171"}]);
    let mut game = loaded(&initial);
    play(&mut game);
    let view = state(&game);
    assert_eq!(view["P1"]["hand"], json!(["spell1"]));
    assert!(
        view["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("spell2"))
    );
    assert_eq!(view["P1"]["deck"], json!(["rest1", "rest2"]));
}

#[test]
fn bp01_154_real_attack_damages_opposing_followers() {
    let mut initial = source("BP01-154", "field");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = loaded(&initial);
    game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
        .unwrap();
    settle(&mut game, None);
    assert_eq!(state(&game)["objects"]["enemy"]["damage"], 3_u8);
}

#[test]
fn bp02_037_fanfare_creates_an_earth_sigil() {
    let mut game = loaded(&source("BP02-037", "hand"));
    play(&mut game);
    assert_eq!(state(&game)["P1"]["field"].as_array().unwrap().len(), 2);
}

#[test]
fn bp03_038_fanfare_draws_after_creating_a_stacked_sigil() {
    let mut initial = source("BP03-038", "hand");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"BP01-171"}]);
    let mut game = loaded(&initial);
    play(&mut game);
    let view = state(&game);
    assert_eq!(view["P1"]["field"].as_array().unwrap().len(), 2);
    assert_eq!(view["P1"]["hand"], json!(["top"]));
}

#[test]
fn bp04_041_fanfare_partitions_all_four_observed_cards() {
    let mut initial = source("BP04-041", "hand");
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (1_u8..=4_u8)
            .map(|i| json!({"id":format!("card{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    let mut game = loaded(&initial);
    play(&mut game);
    let view = state(&game);
    assert_eq!(view["P1"]["hand"], json!(["card1"]));
    assert_eq!(view["P1"]["cemetery"], json!(["card2"]));
    assert_eq!(view["P1"]["deck"], json!(["card3", "card4"]));
}

#[test]
fn bp05_105_observes_a_neutral_follower_entering() {
    let mut initial = source("BP05-105", "field");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"neutral","card":"BP01-171"}]);
    let mut game = loaded(&initial);
    let step = game.play_internal_card("neutral", PLAYER).unwrap();
    settle(&mut game, Some(step));
    assert_eq!(state(&game)["P2"]["leader"]["life"], 19_u8);
}

#[test]
fn bp09_055_recovers_pp_from_dragon_spells_in_cemetery() {
    let mut initial = source("BP09-055", "hand");
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"spell1","card":"SD04-002"},{"id":"spell2","card":"SD04-002"}]);
    let mut game = loaded(&initial);
    play(&mut game);
    assert_eq!(state(&game)["P1"]["pp"]["current"], 2_u8);
}

#[test]
fn bp10_004_pays_act_to_damage_the_opponents_played_follower() {
    let mut initial = source("BP10-004", "field");
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = loaded(&initial);
    let step = game
        .play_internal_card("enemy", PlayerId { seat: 2 })
        .unwrap();
    settle(&mut game, Some(step));
    assert_eq!(state(&game)["P2"]["cemetery"], json!(["enemy"]));
}

#[test]
fn bp10_085_fanfare_counts_the_remaining_hand() {
    let mut initial = source("BP10-085", "hand");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"spare1","card":"BP01-171"}),
            json!({"id":"spare2","card":"BP01-171"}),
        ]);
    let mut game = loaded(&initial);
    play(&mut game);
    assert_eq!(
        state(&game)["objects"]["source"]["counters"]["恩寵カウンター"],
        2_u8
    );
}

#[test]
fn bp11_032_declares_and_pays_x_to_create_that_many_knights() {
    let mut game = loaded(&source("BP11-032", "hand"));
    play(&mut game);
    let view = state(&game);
    assert_eq!(view["P1"]["field"].as_array().unwrap().len(), 3);
}

#[test]
fn bp13_006_real_evolution_partitions_the_observed_elf_spells() {
    let mut initial = source("BP13-005", "field");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP13-006"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"spell1","card":"BP01-023"},{"id":"spell2","card":"BP01-023"},{"id":"rest1","card":"BP01-171"},{"id":"rest2","card":"BP01-171"},{"id":"rest3","card":"BP01-171"}]);
    let mut game = loaded(&initial);
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
    )
    .unwrap();
    settle(&mut game, None);
    assert_eq!(
        state(&game)["P1"]["deck"],
        json!(["rest1", "rest2", "rest3"])
    );
    assert_eq!(state(&game)["P1"]["hand"], json!(["spell1"]));
    assert_eq!(state(&game)["P1"]["cemetery"], json!(["spell2"]));
    assert_eq!(state(&game)["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn bp13_056_creates_a_dragon_once_per_turn() {
    let mut game = loaded(&source("BP13-056", "field"));
    activate(&mut game, "BP13-056", 3);
    assert_eq!(state(&game)["P1"]["field"].as_array().unwrap().len(), 2);
    game.activate_internal_card("source", ability("BP13-056", 3), PLAYER)
        .expect_err("the printed payment or per-turn quota must reject this activation");
}

#[test]
fn bp14_066_removes_two_hot_spring_counters_before_its_buff() {
    let mut initial = source("BP14-066", "field");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"spa","card":"BP14-059","state":{"counters":{"神湯カウンター":2_u8}}}));
    let mut game = loaded(&initial);
    let before = state(&game)["objects"]["source"]["power"].as_i64().unwrap();
    activate(&mut game, "BP14-066", 2);
    let view = state(&game);
    assert_eq!(view["objects"]["source"]["power"], before + 1);
    game.activate_internal_card("source", ability("BP14-066", 2), PLAYER)
        .expect_err("the printed payment or per-turn quota must reject this activation");
}

#[test]
fn bp15_077_evolves_and_resolves_the_draw_mode() {
    let mut initial = source("BP15-076", "field");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP15-077"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"BP01-171"}]);
    let mut game = loaded(&initial);
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
    )
    .unwrap();
    settle(&mut game, None);
    assert_eq!(state(&game)["P1"]["hand"], json!(["top"]));
}
