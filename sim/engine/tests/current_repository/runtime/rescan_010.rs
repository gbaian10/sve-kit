#![expect(
    clippy::panic,
    reason = "Unexpected decisions indicate a broken game fixture."
)]

use super::*;
use core::iter;

const P1: PlayerId = PlayerId { seat: 1 };

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&question.domain, InputDomain::Modes { .. }) {
            Answer::Modes(vec![ModeId(1)])
        } else {
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("expected a target decision");
            };
            Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(1)
                    .map(|member| member.handle)
                    .collect(),
            )
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn resolve_pending(game: &mut Game, card: &str, line: u64) {
    let pending = game.internal_pending_choice().unwrap().unwrap_or_else(|| {
        panic!(
            "missing pending choice; state: {:?}",
            game.projection(View::Referee)
        )
    });
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("expected a pending ability choice");
    };
    let option = options
        .iter()
        .find(|option| option.card == card && option.line == line)
        .unwrap();
    let play = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(option.handle))
        .unwrap();
    finish(game, play);
    finish_checks(game);
}

fn play(game: &mut Game) {
    let play = game.play_internal_card("source", P1).unwrap();
    finish(game, play);
    finish_checks(game);
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"draw-1","card":"BP01-037"},
        {"id":"draw-2","card":"BP01-037"}
    ]);
    initial
}

fn replace_face_id(value: &mut Value, old: &str, new: &str) {
    match value {
        Value::String(text) if text == old => new.clone_into(text),
        Value::Array(values) => {
            for nested in values {
                replace_face_id(nested, old, new);
            }
        }
        Value::Object(values) => {
            for nested in values.values_mut() {
                replace_face_id(nested, old, new);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn game_with_test_evolve_base(initial: &Value) -> Game {
    // Supply only the 086 base evolve action so this scenario can reach migrated face 087.
    let (snapshot, keywords, mut input) = inputs().clone();
    let authors = input["authors"].as_array_mut().unwrap();
    let mut base = authors
        .iter()
        .find(|author| author["input"]["card_no"] == "BP05-004")
        .unwrap()
        .clone();
    let base_id = "c:ff6943948ba651859820aac9b0879df2";
    base["input"]["card_no"] = json!("BP05-086");
    base["input"]["card_id"] = json!(base_id);
    base["input"]["source_payload"]["card_id"] = json!(base_id);
    base["input"]["source_payload"]["faces"][0]["cost"] = json!(4_u8);
    base["printings"] = json!(["BP05-086"]);
    let cards = base["input"]["tree"]["cards"].as_object_mut().unwrap();
    let mut card = cards.remove("BP05-004").unwrap();
    let faces = card["faces"].as_object_mut().unwrap();
    let old_face_id = faces.keys().next().unwrap().clone();
    let mut face = faces.remove(&old_face_id).unwrap();
    face["abilities"][0]["evolve"]["cost"]["pp"] = json!(4_u8);
    let face_id = "f:2342d87f3f9b52ab9f52e29718f75384";
    replace_face_id(&mut face, &old_face_id, face_id);
    card["faces"] = Value::Object(iter::once((face_id.to_owned(), face)).collect());
    cards.insert("BP05-086".to_owned(), card);
    base["input"]["source_payload"]["faces"][0]["face_id"] = json!(face_id);
    authors.push(base);

    game_from_selection(initial, &snapshot, &keywords, &input)
}

fn attack(game: &mut Game, attacker: &str, target: &str) -> String {
    game.internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":target}))
        .unwrap()
        .outcome
}

fn pass_quick(game: &mut Game) {
    let question = game
        .internal_decision(View::Referee)
        .filter(|question| matches!(question.domain, InputDomain::Quick { .. }))
        .map_or_else(|| game.internal_quick_choice().unwrap(), Some);
    if let Some(question) = question {
        let play = game
            .answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish(game, play);
    }
    finish_checks(game);
}

fn attack_and_pass(game: &mut Game, attacker: &str, target: &str, trigger: Option<(&str, u64)>) {
    assert_eq!(
        attack(game, attacker, target),
        "resolved",
        "the attack should resolve before any queued triggers"
    );
    finish_checks(game);
    pass_quick(game);
    if let Some((card, line)) = trigger {
        resolve_pending(game, card, line);
    }
    pass_quick(game);
}

fn pass_end_phase(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    loop {
        finish_checks(game);
        if game.internal_pending_choice().unwrap().is_some() {
            break;
        }
        let Some(question) = game.internal_quick_choice().unwrap() else {
            break;
        };
        game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
    }
}

#[test]
fn bp05_014_fanfare_creates_a_puppet_in_ex() {
    let mut game = game(&position("BP05-014"));
    play(&mut game);
    resolve_pending(&mut game, "BP05-014", 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 1);
}

#[test]
fn bp05_016_fanfare_creates_two_puppets_in_ex() {
    let mut game = game(&position("BP05-016"));
    play(&mut game);
    resolve_pending(&mut game, "BP05-016", 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 2);
}

#[test]
fn bp05_060_ward_protects_its_leader() {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"ward","card":"BP05-060","state":{"acted":true}}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"attacker","card":"BP05-065","state":{"acted":false,"entered_this_turn":false}}
    ]);
    let mut game = game(&initial);
    assert_eq!(attack(&mut game, "attacker", "P1.leader"), "cannot-attack");
}

#[test]
fn bp05_064_lastword_creates_two_puppets_in_ex() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"attacker","card":"BP05-T04","state":{"entered_this_turn":false}}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"lastword","card":"BP05-064","state":{"acted":true,"damage":3_i32}}
    ]);
    let mut game = game(&initial);
    attack_and_pass(&mut game, "attacker", "lastword", Some(("BP05-064", 2)));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["ex"].as_array().unwrap().len(), 2);
}

#[test]
fn bp05_065_evolves_for_one_pp() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"base","card":"BP05-065","state":{"entered_this_turn":false}}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP05-066"}]);
    initial["players"]["P1"]["pp"]["current"] = json!(1_i32);
    let mut game = game(&initial);
    let action = game
        .internal_rule_action(&json!({
            "do":"evolve",
            "source":"base",
            "evolve_card":"form",
            "pay":{"pp":1_i32}
        }))
        .unwrap();
    assert_eq!(action.outcome, "resolved");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        0_u64
    );
}

#[test]
fn bp05_087_ward_protects_its_leader() {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"ward","card":"BP05-087","state":{"acted":true}}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"attacker","card":"BP05-065","state":{"acted":false,"entered_this_turn":false}}
    ]);
    let mut game = game(&initial);
    assert_eq!(attack(&mut game, "attacker", "P1.leader"), "cannot-attack");
}

#[test]
fn bp05_087_evolve_time_increases_pp_max_restores_life_and_draws() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(15_u8);
    initial["players"]["P1"]["pp"] = json!({"current":4_u8,"max":5_u8});
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"base","card":"BP05-086"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] =
        json!([{"id":"form","card":"BP05-087"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"drawn","card":"BP01-037"}]);
    let mut game = game_with_test_evolve_base(&initial);

    let action = game
        .internal_rule_action(&json!({
            "do":"evolve",
            "source":"base",
            "evolve_card":"form",
            "pay":{"pp":4_u8}
        }))
        .unwrap();
    assert_eq!(action.outcome, "resolved", "the printed evolution should resolve");
    resolve_pending(&mut game, "BP05-087", 2);

    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["max"], 6_u64, "evolution adds one maximum PP");
    assert_eq!(state["P1"]["leader"]["life"], 18_u64, "the leader gains three life");
    assert_eq!(state["P1"]["hand"], json!(["drawn"]), "evolution draws one card");
}

#[test]
fn bp05_089_recovers_pp_at_the_opponents_main_start() {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":4_i32,"max":5_i32});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP05-089"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"draw","card":"BP01-037"}]);
    let mut game = game(&initial);
    pass_end_phase(&mut game);
    resolve_pending(&mut game, "BP05-089", 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["turn"]["active"], "P2");
    assert_eq!(state["P1"]["pp"]["current"], 5_u64);
}

#[test]
fn bp05_110_ward_protects_its_leader() {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"ward","card":"BP05-110","state":{"acted":true}}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"attacker","card":"BP05-065","state":{"acted":false,"entered_this_turn":false}}
    ]);
    let mut game = game(&initial);
    assert_eq!(attack(&mut game, "attacker", "P1.leader"), "cannot-attack");
}

#[test]
fn bp05_114_evolves_into_115_and_creates_an_ancient_artifact() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"base","card":"BP05-114","state":{"entered_this_turn":false}}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP05-115"}]);
    initial["players"]["P1"]["pp"]["current"] = json!(1_i32);
    let mut game = game(&initial);
    let action = game
        .internal_rule_action(&json!({
            "do":"evolve",
            "source":"base",
            "evolve_card":"form",
            "pay":{"pp":1_i32}
        }))
        .unwrap();
    assert_eq!(action.outcome, "resolved");
    finish_checks(&mut game);
    resolve_pending(&mut game, "BP05-115", 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 2);
    assert_eq!(state["objects"]["base"]["evolved"], true);
}

#[test]
fn bp05_t04_rush_allows_a_follower_attack_on_entry() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"storm","card":"BP05-T04","state":{"entered_this_turn":true}}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"target","card":"BP05-065","state":{"acted":true}}
    ]);
    let mut first_game = game(&initial);
    assert_eq!(attack(&mut first_game, "storm", "target"), "resolved");

    let mut rush_position = draw_position();
    rush_position["players"]["P1"]["zones"]["field"] = json!([
        {"id":"storm","card":"BP05-T04","state":{"entered_this_turn":true}}
    ]);
    let mut second_game = game(&rush_position);
    assert_eq!(
        attack(&mut second_game, "storm", "P2.leader"),
        "cannot-attack"
    );
}

#[test]
fn bp05_004_evolves_for_one_pp() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"base","card":"BP05-004","state":{"entered_this_turn":false}}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP05-005"}]);
    initial["players"]["P1"]["pp"]["current"] = json!(1_i32);
    let mut game = game(&initial);
    let action = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"base","evolve_card":"form","pay":{"pp":1_i32}}),
        )
        .unwrap();
    assert_eq!(action.outcome, "resolved");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        0_u64
    );
}
