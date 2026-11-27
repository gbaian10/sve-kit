use super::*;
use alloc::collections::BTreeMap;
use std::{fs::read_to_string, path::PathBuf};

fn evolution_fixture() -> &'static (SelectedEffects, BTreeMap<String, WellFormedCardProgram>) {
    static PREPARED: OnceLock<(SelectedEffects, BTreeMap<String, WellFormedCardProgram>)> =
        OnceLock::new();
    PREPARED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let mut programs = BTreeMap::new();
        for form in ["BP09-107", "BP10-010", "BP10-100", "BP11-090", "PR-013"] {
            let printed = snapshot
                .lines()
                .map(|line| serde_json::from_str::<Value>(line).unwrap())
                .find(|row| row["number"] == form)
                .unwrap();
            let number = format!("evolution-{form}");
            let face = json!({
                "name": printed["faces"][0]["name"], "card_class": "ニュートラル",
                "card_type": "フォロワー", "cost": "1", "power": "1", "hp": "1",
                "traits": [], "text": null, "sections": []
            });
            let companion = json!({"number": number, "faces": [face]});
            snapshot.push_str(&companion.to_string());
            snapshot.push('\n');
            let document = json!({
                "format": 1_u8, "kind": "fixture_cards",
                "cards": {number.clone(): {"status": "complete"}}
            });
            input["documents"]
                .as_array_mut()
                .unwrap()
                .push(json!([format!("{number}.yaml"), document.to_string()]));
            let mut tree = expanded::tree().clone();
            tree["cards"] = json!({number.clone(): tree["cards"]["synthetic-evolution"]});
            let program = lowerer().lower(&tree).unwrap().remove(&number).unwrap();
            programs.insert(number, program);
        }
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../authored/rules/engine/index.yaml");
        let mut rules: Value = serde_saphyr::from_str(&read_to_string(path).unwrap()).unwrap();
        rules["input"] = rule_fixtures::rules(&snapshot, json!([]), json!([]))["input"].clone();
        let selected = repository::select_with_rules(
            &snapshot,
            &keywords,
            &input,
            &rules.to_string(),
        )
        .unwrap();
        (selected, programs)
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

fn evolved_removal(form: &str, life: bool, destination: &str) {
    let mut initial = draw_position();
    let candidate = if life { "BP01-037" } else { "BP01-046" };
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"victim","card":candidate,"state":{"damage":if life{2_u8}else{0}}},
        {"id":"excluded","card":"BP04-117"}
    ]);
    let mut game = evolution(&mut initial, "unused", form, 1);
    let play = resolve_pending_card(&mut game, form);
    let question = await_card_choice(play);
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("targets")
    };
    assert_eq!(
        domain.members.len(),
        1,
        "the expensive or healthy follower is excluded"
    );
    resolved(choose_card_by_number(
        &mut game,
        CardPlay::Awaiting(question),
        candidate,
    ));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P2"]["field"],
        json!(["excluded"]),
        "only the eligible follower leaves"
    );
    assert_eq!(
        state["P2"][destination],
        json!(["victim"]),
        "the printed movement destination is used"
    );
}
#[test]
fn bp09_107_evolution_banishes_a_four_or_less_cost_follower() {
    evolved_removal("BP09-107", false, "banish");
}
#[test]
fn bp10_010_evolution_returns_a_three_or_less_cost_follower() {
    evolved_removal("BP10-010", false, "hand");
}
#[test]
fn bp10_100_evolution_destroys_a_three_or_less_life_follower() {
    evolved_removal("BP10-100", true, "cemetery");
}
#[test]
fn bp11_090_evolution_banishes_a_three_or_less_life_follower() {
    evolved_removal("BP11-090", true, "banish");
}
#[test]
fn bp18_112_fanfare_banishes_a_three_or_less_cost_follower() {
    let mut initial = position_with_source("BP18-112", "hand");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"victim","card":"BP01-046"},{"id":"excluded","card":"BP01-037"}]);
    let mut game = game(&initial);
    let play = enter_with_fanfare(&mut game, "BP18-112");
    resolved(choose_card_by_number(&mut game, play, "BP01-046"));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["banish"],
        json!(["victim"])
    );
}
#[test]
fn cp02_075_destroys_a_cheap_follower_then_mills_one_card() {
    let mut initial = position_with_source("CP02-075", "hand");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"milled","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"victim","card":"BP01-046"},{"id":"excluded","card":"BP04-117"}]);
    let mut game = game(&initial);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    resolved(choose_card_by_number(&mut game, play, "BP01-046"));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!(["excluded"]));
    assert_eq!(state["P1"]["cemetery"], json!(["milled", "source"]));
}
#[test]
fn cp02_096_banishes_by_life_on_entry_then_pays_five_to_banish_a_healthy_follower() {
    let mut initial = position_with_source("CP02-096", "hand");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"damaged","card":"BP04-117","state":{"damage":4_u8}},{"id":"healthy","card":"BP01-037"}]);
    let mut game = Game::from_selected_effects(
        selection().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "collaboration-amulet",
    )
    .unwrap();
    let play = enter_with_fanfare(&mut game, "CP02-096");
    resolved(choose_card_by_number(&mut game, play, "BP04-117"));
    game.resume_internal_card().unwrap();
    let ability = selection().current["CP02-096"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let activation = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap();
    resolved(choose_card_by_number(&mut game, activation, "BP01-037"));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!([]));
    assert_eq!(state["P1"]["pp"]["current"], 2_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
}

fn vanguard_game(initial: &Value) -> Game {
    let (mut snapshot, keywords, mut input) = inputs().clone();
    let mut cards = serde_json::Map::new();
    for (number, cost, trait_id) in [
        ("eligible", "4", "ヴァンガード"),
        ("too-small", "3", "ヴァンガード"),
        ("other-trait", "4", "傭兵"),
    ] {
        let face = json!({
            "name": number, "card_class": "ニュートラル", "card_type": "フォロワー",
            "cost": cost, "power": "2", "hp": "2", "traits": [trait_id],
            "text": null, "sections": []
        });
        snapshot.push_str(&json!({"number": number, "faces": [face]}).to_string());
        snapshot.push('\n');
        cards.insert(
            number.to_owned(),
            json!({"status":"complete"}),
        );
    }
    let document = json!({"format":1_u8,"kind":"fixture_cards","cards":cards});
    input["documents"]
        .as_array_mut()
        .unwrap()
        .push(json!(["filter-vanguard.yaml", document.to_string()]));
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../authored/rules/engine/index.yaml");
    let mut rules: Value = serde_saphyr::from_str(&read_to_string(path).unwrap()).unwrap();
    rules["input"] = rule_fixtures::rules(&snapshot, json!([]), json!([]))["input"].clone();
    let selected = repository::select_with_rules(
        &snapshot,
        &keywords,
        &input,
        &rules.to_string(),
    )
    .unwrap();
    let mut game = Game::from_selected_effects(
        selected,
        initial,
        &Value::Null,
        &Value::Null,
        "vanguard-filter",
    )
    .unwrap();
    let vanilla = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in ["eligible", "too-small", "other-trait"] {
        game.load_internal_card(number, &vanilla).unwrap();
    }
    game
}

#[test]
fn cp03_033_buffs_only_a_four_cost_vanguard_and_grants_rush_and_assail() {
    let mut initial = position_with_source("CP03-033", "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"eligible","card":"eligible"},
        {"id":"small","card":"too-small"},
        {"id":"other","card":"other-trait"}
    ]);
    let mut game = vanguard_game(&initial);
    let question = await_card_choice(enter_with_fanfare(&mut game, "CP03-033"));
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("targets")
    };
    assert_eq!(domain.members.len(), 1);
    resolved(choose_card_by_number(
        &mut game,
        CardPlay::Awaiting(question),
        "eligible",
    ));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["eligible"]["power"], 4_u8);
    assert_eq!(state["objects"]["eligible"]["hp"], 4_u8);
    let keywords = state["objects"]["eligible"]["keywords"].as_array().unwrap();
    assert!(keywords.contains(&json!("突進")));
    assert!(keywords.contains(&json!("指定攻撃")));
    assert_eq!(state["objects"]["small"]["power"], 2_u8);
    assert_eq!(state["objects"]["other"]["power"], 2_u8);
}

fn trigger_line(game: &mut Game, line: u64) -> CardPlay {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = pending.domain else {
        panic!("fanfare")
    };
    let option = options
        .iter()
        .find(|option| option.card == "SP01-045" && option.line == line)
        .unwrap();
    game.answer_internal_card(pending.actor, pending.token, Answer::Pending(option.handle))
        .unwrap()
}

#[test]
fn sp01_045_draws_then_moves_a_cheap_enemy_to_ex_only_when_entering_outside_hand() {
    for origin in ["hand", "ex"] {
        let mut initial = position_with_source("SP01-045", origin);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"victim","card":"BP01-046"}]);
        let mut game = game(&initial);
        assert!(matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        resolved(trigger_line(&mut game, 1));
        game.resume_internal_card().unwrap();
        let play = trigger_line(&mut game, 2);
        resolved(choose_card_by_number(&mut game, play, "BP01-046"));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["drawn"]));
        assert_eq!(
            state["P2"]["field"],
            if origin == "hand" {
                json!(["victim"])
            } else {
                json!([])
            }
        );
        assert_eq!(
            state["P2"]["ex"],
            if origin == "hand" {
                json!([])
            } else {
                json!(["victim"])
            }
        );
    }
}
