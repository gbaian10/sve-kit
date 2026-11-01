#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed movement scenarios."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::DecisionView;

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        snapshot.push_str(&json!({"number":"named-writer","faces":[{
            "name":"真理の大文豪・ユキシマ","card_class":"ニュートラル","card_type":"フォロワー",
            "traits":[],"cost":"0","power":"1","hp":"1","text":null,"sections":[]
        }]}).to_string());
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!([
            "named-writer.yaml",
            json!({"format":1_u8,"kind":"effect_set","cards":{
                "named-writer":{"status":"complete","abilities":[]}
            }})
            .to_string()
        ]));
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../authored/rules/engine/index.yaml");
        let mut rules: Value = serde_saphyr::from_str(&read_to_string(path).unwrap()).unwrap();
        rules["input"] = rule_fixtures::rules(&snapshot, json!([]), json!([]))["input"].clone();
        repository::select_with_rules(
            &snapshot,
            &keywords,
            &input,
            &rules.to_string(),
            &EngineIdentityInput::LegacyJp,
        )
        .unwrap()
    })
}

fn game(state: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        state,
        &Value::Null,
        &Value::Null,
        "observed-move",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    game.load_internal_card("named-writer", &empty).unwrap();
    game
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed choice")
    };
    question
}

fn trigger(game: &mut Game, number: &str) -> DecisionView {
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(play, CardPlay::Entered(_)),
        "the printed permanent enters before its fanfare"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("fanfare")
    };
    let handle = options
        .iter()
        .find(|option| option.card == number)
        .unwrap()
        .handle;
    awaiting(
        game.answer_internal_card(pending.actor, pending.token, Answer::Pending(handle))
            .unwrap(),
    )
}

fn position(number: &str, card: &str) -> Value {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    state["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"picked","card":card}, {"id":"other","card":"BP01-037"},
        {"id":"tail","card":card}
    ]);
    state
}

#[test]
fn minigoblin_reveals_only_its_chosen_card_and_sends_the_observed_remainder_to_cemetery() {
    let mut game = game(&position("BP22-115", "BP22-115"));
    let question = trigger(&mut game, "BP22-115");
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("private goblin choice")
    };
    assert_eq!(domain.members.len(), 1);
    assert!(game.internal_decision(View::P2).is_none());
    let before = game.projection(View::Referee).unwrap();
    game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![999]))
        .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"], json!(["picked"]));
    assert_eq!(view["P1"]["cemetery"], json!(["other"]));
    assert_eq!(view["P1"]["deck"], json!(["tail"]));
    let public = game.projection(View::P2).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    assert!(known.iter().any(|card| card["id"] == "picked"));
    assert!(!known.iter().any(|card| card["id"] == "tail"));
}

#[test]
fn minigoblin_decline_sends_all_looked_cards_to_cemetery_without_touching_the_tail() {
    let mut game = game(&position("BP22-115", "BP22-115"));
    let q = trigger(&mut game, "BP22-115");
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![]))
            .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"], json!([]));
    assert_eq!(view["P1"]["cemetery"], json!(["picked", "other"]));
    assert_eq!(view["P1"]["deck"], json!(["tail"]));
}

#[test]
fn mio_puts_the_observed_passion_follower_on_field_and_only_the_remainder_in_cemetery() {
    let mut game = game(&position("CSD02c-001", "CSD02c-001"));
    let question = trigger(&mut game, "CSD02c-001");
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("private passion choice")
    };
    assert_eq!(domain.members.len(), 1);
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"], json!(["source", "picked"]));
    assert_eq!(view["P1"]["cemetery"], json!(["other"]));
    assert_eq!(view["P1"]["deck"], json!(["tail"]));
}

#[test]
fn mio_full_field_keeps_the_unsuccessful_choice_in_the_observed_remainder() {
    let mut state = position("CSD02c-001", "CSD02c-001");
    state["players"]["P1"]["zones"]["field"] = json!(
        (0_u8..4)
            .map(|i| json!({"id":format!("ally{i}"),"card":"BP01-037"}))
            .collect::<Vec<_>>()
    );
    let mut game = game(&state);
    let q = trigger(&mut game, "CSD02c-001");
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("private passion choice")
    };
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"].as_array().unwrap().len(), 5);
    assert_eq!(view["P1"]["cemetery"], json!(["picked", "other"]));
    assert_eq!(view["P1"]["deck"], json!(["tail"]));
}

#[test]
fn mio_activation_pays_two_pp_and_two_lesson_materials_before_the_private_look() {
    let mut state = position("CSD02c-001", "CSD02c-001");
    state["players"]["P1"]["zones"]["hand"] = json!([]);
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CSD02c-001"}]);
    state["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"item1","card":"PR-155"}, {"id":"item2","card":"PR-156"}
    ]);
    let mut game = game(&state);
    let ability = prepared().current["CSD02c-001"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == 3)
        .unwrap()
        .node
        .id;
    let fee = awaiting(
        game.activate_internal_card("source", ability, PlayerId { seat: 1 })
            .unwrap(),
    );
    assert!(matches!(fee.domain, InputDomain::CostMaterials(_)));
    let q = awaiting(
        game.answer_internal_card(fee.actor, fee.token, Answer::CostMaterials(vec![0, 1]))
            .unwrap(),
    );
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("private passion choice after payment")
    };
    assert!(game.internal_decision(View::P2).is_none());
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["pp"]["current"], 8_u8);
    assert_eq!(view["P1"]["ex"], json!([]));
    assert_eq!(view["P1"]["field"], json!(["source", "picked"]));
    assert_eq!(view["P1"]["cemetery"], json!(["other"]));
    assert_eq!(view["P1"]["deck"], json!(["tail"]));
}

#[test]
fn exploration_takes_two_revealed_cards_and_sends_two_to_cemetery() {
    let mut state = position("BP14-042", "BP22-115");
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"writer","card":"named-writer"}]);
    state["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"one","card":"BP22-115"}, {"id":"two","card":"BP01-037"},
        {"id":"three","card":"BP22-115"}, {"id":"four","card":"BP01-037"},
        {"id":"tail","card":"BP22-115"}
    ]);
    let mut game = game(&state);
    let q = awaiting(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("two-card observation")
    };
    assert!(game.internal_decision(View::P2).is_none());
    assert_eq!(domain.members.len(), 4);
    game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![]))
        .unwrap_err();
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle, domain.members[2].handle]),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"], json!(["one", "three"]));
    assert_eq!(view["P1"]["cemetery"], json!(["two", "four", "source"]));
    assert_eq!(view["P1"]["deck"], json!(["tail"]));
    assert_eq!(view["P1"]["pp"]["current"], 8_u8);
}
