#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
//! Cards needing only the result-card, mode-condition and opponent-discard wiring.
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn seat(n: u32) -> PlayerId {
    PlayerId { seat: n }
}

fn cards(prefix: &str, numbers: &[&str]) -> Value {
    Value::Array(
        numbers
            .iter()
            .enumerate()
            .map(|(i, card)| json!({"id":format!("{prefix}{i}"),"card":card}))
            .collect(),
    )
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "wire-batch-01",
    )
    .unwrap()
}

/// Answers every decision with the first legal members, or the given modes.
fn finish(game: &mut Game, mut step: CardPlay, modes: &[u64]) {
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Modes { .. } => Answer::Modes(modes.iter().map(|m| ModeId(*m)).collect()),
            InputDomain::Targets(domain) => {
                let most = domain
                    .cardinality
                    .maximum
                    .map_or(domain.members.len(), |m| usize::try_from(m.0).unwrap());
                let least = usize::try_from(domain.cardinality.minimum.0).unwrap();
                Answer::Targets(
                    domain
                        .members
                        .iter()
                        .take(most.max(least).min(domain.members.len()))
                        .map(|m| m.handle)
                        .collect(),
                )
            }
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap().max(1))
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain.cards.iter().take(1).map(|card| card.handle).collect(),
            )),
            InputDomain::Order(cards) => Answer::Order(cards.iter().map(|c| c.handle).collect()),
            other => panic!("unexpected decision: {other:?}"),
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn triggers(game: &mut Game, modes: &[u64]) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending ability expected")
        };
        let step = game
            .answer_internal_card(question.actor, question.token, Answer::Pending(options[0].handle))
            .unwrap();
        finish(game, step, modes);
    }
}

fn play(initial: &Value, modes: &[u64]) -> Game {
    let mut game = game(initial);
    let step = game.play_internal_card("source", seat(1)).unwrap();
    finish(&mut game, step, modes);
    triggers(&mut game, modes);
    game
}

fn in_hand(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = cards("d", &["BP01-171", "BP01-171", "BP01-171"]);
    initial
}

fn names(state: &Value, seat: &str, zone: &str) -> Vec<String> {
    state[seat][zone]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| state["objects"][id.as_str().unwrap()]["name"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn bp15_024_reads_the_opponents_milled_card_kind() {
    for (top, token) in [("BP03-066", "黄金の杯"), ("BP01-171", "黄金の小剣")] {
        let mut initial = in_hand("BP15-024");
        initial["players"]["P2"]["zones"]["deck"] = cards("o", &[top, "BP01-171"]);
        let state = play(&initial, &[]).projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["cemetery"], json!(["o0"]));
        assert_eq!(names(&state, "P1", "ex"), vec![token.to_owned()]);
    }
}

#[test]
fn bp18_084_makes_a_bat_only_for_a_revealed_vampy() {
    for (top, bats) in [("BP02-069", 1_usize), ("BP02-070", 0)] {
        let mut initial = in_hand("BP18-084");
        initial["players"]["P1"]["zones"]["deck"] = cards("d", &[top, "BP01-171", "BP01-171"]);
        let state = play(&initial, &[]).projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["d0"]));
        let field = names(&state, "P1", "field");
        assert_eq!(field.iter().filter(|n| *n == "フォレストバット").count(), bats);
    }
}

#[test]
fn bp15_086_heals_only_for_a_revealed_vanarek_follower() {
    let mut initial = in_hand("BP15-086");
    initial["players"]["P1"]["zones"]["deck"] =
        cards("d", &["BP15-076", "BP01-171", "BP01-171", "BP01-171"]);
    let state = play(&initial, &[]).projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["d0"]));
    assert_eq!(state["P1"]["leader"]["life"], 21_u8, "1 damage then 2 healing");
}

#[test]
fn bp14_099_damages_its_target_only_after_milling_a_non_bishop_card() {
    for (top, damage) in [("BP01-171", 4_u8), ("BP01-127", 0)] {
        let mut initial = in_hand("BP14-099");
        initial["players"]["P1"]["zones"]["deck"] = cards("d", &[top, "BP01-171"]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-126"}]);
        let state = play(&initial, &[]).projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!(["d0"]));
        assert_eq!(state["objects"]["enemy"]["damage"], damage);
    }
}

#[test]
fn bp03_086_draws_after_discarding_a_dead_card() {
    for (discard, drawn) in [("BP01-117", 1_usize), ("BP01-171", 0)] {
        let mut initial = in_hand("BP03-086");
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"source","card":"BP03-086"},{"id":"fuel","card":discard}]);
        let state = play(&initial, &[]).projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!(["fuel"]));
        assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), drawn);
        assert!(names(&state, "P1", "field").contains(&"ゴースト".to_owned()));
    }
}

#[test]
fn cp01_057_needs_twenty_umamusume_and_makes_the_opponent_discard_three() {
    let setup = |count: usize| {
        let mut initial = in_hand("CP01-057");
        initial["players"]["P1"]["zones"]["cemetery"] = cards("u", &vec!["BP22-117"; count]);
        initial["players"]["P2"]["zones"]["field"] = cards("e", &["BP01-171", "BP01-126"]);
        initial["players"]["P2"]["zones"]["hand"] =
            cards("h", &["BP01-171", "BP01-171", "BP01-171", "BP01-171"]);
        initial
    };
    let mut short = game(&setup(19));
    assert!(short.play_internal_card("source", seat(1)).is_err(), "19 cards forbid the play");
    let state = play(&setup(20), &[]).projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!([]), "every opposing follower is destroyed");
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 3);
    assert_eq!(state["P2"]["hand"].as_array().unwrap().len(), 1, "the opponent discards three");
}

#[test]
fn bp16_082_has_storm_only_with_five_cemetery_cards() {
    for (count, storm) in [(4_usize, false), (5, true)] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP16-082"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = cards("c", &vec!["BP01-171"; count]);
        let state = game(&initial).projection(View::Referee).unwrap();
        let keywords = state["objects"]["source"]["keywords"].as_array().unwrap();
        assert_eq!(keywords.contains(&json!("疾走")), storm);
        assert!(keywords.contains(&json!("必殺")));
    }
}

#[test]
fn cp01_056_fanfare_weakens_an_opposing_follower() {
    let mut initial = in_hand("CP01-056");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-126"}]);
    let state = play(&initial, &[]).projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["power"], 3_u8);
    assert_eq!(state["objects"]["enemy"]["hp"], 4_u8);
}

#[test]
fn dsd01a_001_takes_both_searches_with_ten_academy_cards() {
    let mut initial = in_hand("DSD01a-001");
    initial["players"]["P1"]["zones"]["deck"] =
        cards("d", &["DSD01a-008", "DSD01a-002", "BP01-171"]);
    initial["players"]["P1"]["zones"]["cemetery"] = cards("c", &["BP02-038"; 10]);
    let state = play(&initial, &[1, 2]).projection(View::Referee).unwrap();
    assert_eq!(names(&state, "P1", "ex"), vec!["アンの大魔法".to_owned()]);
    assert!(names(&state, "P1", "field").contains(&"マナリアの竜姫・グレア".to_owned()));
}

fn evolved_first(card: &str) -> Value {
    let mut initial = in_hand(card);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"base","card":"BP01-092"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP01-093"}]);
    initial
}

fn evolve_then_play(initial: &Value, modes: &[u64]) -> Game {
    let mut game = game(initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"base","evolve_card":"form","pay":{"pp":0_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the base pays its printed evolution cost"
    );
    triggers(&mut game, &[]);
    let step = game.play_internal_card("source", seat(1)).unwrap();
    finish(&mut game, step, modes);
    triggers(&mut game, modes);
    game
}

#[test]
fn bp16_018_offers_both_modes_after_an_evolution() {
    let state = evolve_then_play(&evolved_first("BP16-018"), &[1, 2])
        .projection(View::Referee)
        .unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
    // The awakened evolution draws one and mode two draws one more.
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 2);
}

#[test]
fn bp16_036_mode_one_makes_three_soldiers() {
    let state = play(&in_hand("BP16-036"), &[1]).projection(View::Referee).unwrap();
    let mut field = names(&state, "P1", "field");
    field.sort();
    assert_eq!(field, vec!["シールドガーディアン", "スティールナイト", "ナイト"]);
}

#[test]
fn bp16_054_mode_one_makes_an_earth_essence_and_heals() {
    let state = play(&in_hand("BP16-054"), &[1]).projection(View::Referee).unwrap();
    assert_eq!(names(&state, "P1", "field"), vec!["大地の魔片".to_owned()]);
    assert_eq!(state["P1"]["leader"]["life"], 21_u8);
}

fn attack(initial: &Value, target: &str) -> Value {
    let mut game = game(initial);
    game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":target}))
        .unwrap();
    // Neither player responds in the attack's Quick windows.
    loop {
        triggers(&mut game, &[]);
        let Some(window) = game.internal_quick_choice().unwrap() else {
            break;
        };
        let step = game
            .answer_internal_card(window.actor, window.token, Answer::Quick(None))
            .unwrap();
        finish(&mut game, step, &[]);
    }
    triggers(&mut game, &[]);
    game.projection(View::Referee).unwrap()
}

#[test]
fn bp16_082_damaging_the_opposing_leader_makes_them_discard_one() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP16-082"}]);
    initial["players"]["P2"]["zones"]["hand"] = cards("h", &["BP01-171", "BP01-171"]);
    let state = attack(&initial, "P2.leader");
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
    assert_eq!(state["P2"]["hand"].as_array().unwrap().len(), 1, "the opponent discards one");
    assert_eq!(state["P2"]["cemetery"].as_array().unwrap().len(), 1);
}

#[test]
fn cp01_056_lastword_makes_the_opponent_discard_one() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP01-056"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-126","state":{"acted":true}}]);
    initial["players"]["P2"]["zones"]["hand"] = cards("h", &["BP01-171", "BP01-171"]);
    let state = attack(&initial, "enemy");
    assert_eq!(state["P1"]["cemetery"], json!(["source"]), "the 3/3 dies attacking a 4/5");
    assert_eq!(state["P2"]["hand"].as_array().unwrap().len(), 1, "its lastword makes them discard");
}
