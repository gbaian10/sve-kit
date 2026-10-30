#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the scenario."
)]
use super::*;
use alloc::collections::BTreeMap;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::DecisionView;

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected choice")
    };
    question
}
fn accept(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = &play {
        let answer = if matches!(question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if matches!(question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            break;
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}
fn choose(game: &mut Game, play: CardPlay, card: &str) -> CardPlay {
    let question = awaiting(play);
    let (InputDomain::Targets(domain) | InputDomain::CostMaterials(domain)) = &question.domain
    else {
        panic!("objects")
    };
    let members = &domain.members;
    let material = matches!(question.domain, InputDomain::CostMaterials(_));
    let chosen = members
        .iter()
        .find(|member| member.card.as_deref() == Some(card))
        .unwrap()
        .handle;
    game.answer_internal_card(
        question.actor,
        question.token,
        if material {
            Answer::CostMaterials(vec![chosen])
        } else {
            Answer::Targets(vec![chosen])
        },
    )
    .unwrap()
}
fn trigger(game: &mut Game, card: &str) -> CardPlay {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = pending.domain else {
        panic!("pending trigger")
    };
    let option = options.iter().find(|option| option.card == card).unwrap();
    let play = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(option.handle))
        .unwrap();
    accept(game, play)
}
fn position(card: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    initial
}
fn enter(game: &mut Game, card: &str) -> CardPlay {
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    let play = accept(game, play);
    assert!(
        matches!(play, CardPlay::Entered(_)),
        "the follower enters before its trigger"
    );
    trigger(game, card)
}
fn evolution_fixture() -> &'static (SelectedEffects, BTreeMap<String, WellFormedCardProgram>) {
    static PREPARED: OnceLock<(SelectedEffects, BTreeMap<String, WellFormedCardProgram>)> =
        OnceLock::new();
    PREPARED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let form = snapshot
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|row| row["number"] == "BP02-007")
            .unwrap();
        let number = "evolution-BP02-007";
        let face = json!({
            "name": form["faces"][0]["name"],
            "card_class": "ニュートラル",
            "card_type": "フォロワー",
            "cost": "1", "power": "1", "hp": "1",
            "traits": [], "text": null, "sections": []
        });
        let row = json!({"number": number, "faces": [face]});
        snapshot.push_str(&row.to_string());
        snapshot.push('\n');
        let document = json!({
            "format": 1_u8,
            "kind": "effect_set",
            "cards": {number: {"status": "complete", "abilities": []}}
        });
        input["documents"]
            .as_array_mut()
            .unwrap()
            .push(json!([format!("{number}.yaml"), document.to_string()]));
        let mut tree = expanded::tree().clone();
        tree["cards"] = json!({number: tree["cards"]["synthetic-evolution"]});
        let program = lowerer().lower(&tree).unwrap().remove(number).unwrap();
        let rules_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../authored/rules/engine/index.yaml");
        let mut rules: Value =
            serde_saphyr::from_str(&read_to_string(rules_path).unwrap()).unwrap();
        rules["input"] = rule_fixtures::rules(&snapshot, json!([]), json!([]))["input"].clone();
        let selected = Catalog::from_current_selection_with_rules(
            &snapshot,
            &keywords,
            &input,
            &rules.to_string(),
            &EngineIdentityInput::LegacyJp,
        )
        .unwrap();
        (selected, BTreeMap::from([(number.to_owned(), program)]))
    })
}
fn evolution(initial: &mut Value, base: &str, form: &str, cost: u8) -> Game {
    let (selected, programs) = evolution_fixture();
    let synthetic = format!("evolution-{form}");
    let base = if programs.contains_key(&synthetic) {
        synthetic.as_str()
    } else {
        base
    };
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    let mut game = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "filter-evolution",
    )
    .unwrap();
    if let Some(program) = programs.get(base) {
        game.load_internal_card(base, program).unwrap();
    }
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}}),
    )
    .unwrap();
    game
}

#[test]
fn bp02_007_evolution_destroys_a_cheap_amulet_and_excludes_expensive_followers() {
    let mut initial = draw_position();
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"cheap","card":"BP09-102"},{"id":"expensive","card":"BP01-037"}]);
    let mut game = evolution(&mut initial, "BP02-006", "BP02-007", 1);
    let play = trigger(&mut game, "BP02-007");
    let question = awaiting(play);
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("targets")
    };
    assert_eq!(domain.members.len(), 1);
    resolved(choose(&mut game, CardPlay::Awaiting(question), "BP09-102"));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["field"],
        json!(["expensive"])
    );
}
#[test]
fn bp02_060_evolution_destroys_a_damaged_follower_with_three_life() {
    let mut initial = draw_position();
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"victim","card":"BP01-037","state":{"damage":2_u8}}]);
    let mut game = evolution(&mut initial, "BP02-059", "BP02-060", 1);
    let play = trigger(&mut game, "BP02-060");
    resolved(choose(&mut game, play, "BP01-037"));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["field"],
        json!([])
    );
}
#[test]
fn bp02_102_fanfare_banishes_a_four_or_less_cost_follower() {
    let mut initial = position("BP02-102", "hand");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"victim","card":"BP01-046"},{"id":"expensive","card":"BP01-037"}]);
    let mut game = game(&initial);
    let play = enter(&mut game, "BP02-102");
    let question = awaiting(play);
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("targets")
    };
    assert_eq!(domain.members.len(), 1);
    resolved(choose(&mut game, CardPlay::Awaiting(question), "BP01-046"));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["banish"],
        json!(["victim"])
    );
}
#[test]
fn bp06_120_evolution_destroys_a_two_cost_amulet() {
    let mut initial = draw_position();
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"victim","card":"BP09-102"}]);
    let mut game = evolution(&mut initial, "BP06-119", "BP06-120", 2);
    let play = trigger(&mut game, "BP06-120");
    resolved(choose(&mut game, play, "BP09-102"));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["field"],
        json!([])
    );
}
#[test]
fn bp08_098_opponent_main_trigger_destroys_a_four_life_follower() {
    let mut initial = position("BP08-098", "field");
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"draw","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"victim","card":"BP01-037","state":{"damage":1_u8}}]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    loop {
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        if game.internal_pending_choice().unwrap().is_some() {
            break;
        }
        let Some(question) = game.internal_quick_choice().unwrap() else {
            break;
        };
        game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
    }
    let play = trigger(&mut game, "BP08-098");
    resolved(choose(&mut game, play, "BP01-037"));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["field"],
        json!([])
    );
}
#[test]
fn bp08_109_destroys_a_cheap_card_and_damages_its_leader_with_two_or_less_hand_cards() {
    let mut initial = position("BP08-109", "hand");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"victim","card":"BP09-102"}]);
    let mut game = game(&initial);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    resolved(choose(&mut game, play, "BP09-102"));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!([]));
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
}
#[test]
fn bp09_016_returns_a_cheap_enemy_then_creates_a_puppet_in_ex() {
    let mut initial = position("BP09-016", "hand");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"victim","card":"BP01-046"},{"id":"expensive","card":"BP01-037"}]);
    let mut game = game(&initial);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    resolved(choose(&mut game, play, "BP01-046"));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["hand"], json!(["victim"]));
    assert_eq!(state["P2"]["field"], json!(["expensive"]));
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 1);
}
