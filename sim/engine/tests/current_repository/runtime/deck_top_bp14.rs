#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed Banquet Look scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use serde_json::Map;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::DecisionView;

const CONTROLS: [(&str, &str, &str, bool); 3] = [
    ("banquet", "宴楽の魔法", "スペル", true),
    ("other", "別の仲間", "フォロワー", false),
    ("amulet", "別の護符", "アミュレット", false),
];

struct Fixture {
    selected: SelectedEffects,
    bases: BTreeMap<String, WellFormedCardProgram>,
}

fn add_controls(snapshot: &mut String, input: &mut Value) {
    for (number, name, kind, banquet) in CONTROLS {
        let follower = kind == "フォロワー";
        snapshot.push_str(
            &json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":kind,
                "traits":if banquet {vec!["宴楽"]} else {vec![]},"cost":"9",
                "power":if follower {"1"} else {"-"},"hp":if follower {"9"} else {"-"},
                "text":null,"sections":[]
            }]})
            .to_string(),
        );
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!([
            format!("{number}.yaml"),
            json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()
        ]));
    }
}

fn add_bases(snapshot: &mut String, input: &mut Value) -> BTreeMap<String, WellFormedCardProgram> {
    let mut tree = expanded::tree().clone();
    let template = tree["cards"]["synthetic-evolution"].clone();
    let mut cards = Map::new();
    let mut programs = Map::new();
    for (base, form) in [("elf-base", "BP14-009"), ("ansage-base", "BP14-075")] {
        let record = snapshot
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|record| record["number"] == form)
            .unwrap();
        // The base only produces evolution; unrelated real base abilities remain unadmitted.
        snapshot.push_str(
            &json!({"number":base,"faces":[{
                "name":record["faces"][0]["name"],"card_class":record["faces"][0]["card_class"],
                "card_type":"フォロワー","cost":"1","power":"1","hp":"1",
                "traits":[],"text":null,"sections":[]
            }]})
            .to_string(),
        );
        snapshot.push('\n');
        cards.insert(base.into(), json!({"status":"complete","abilities":[]}));
        programs.insert(base.into(), template.clone());
    }
    input["documents"].as_array_mut().unwrap().push(json!([
        "top-bp14-bases.yaml",
        json!({"format":1_u8,"kind":"effect_set","cards":cards}).to_string()
    ]));
    tree["cards"] = Value::Object(programs);
    lowerer().lower(&tree).unwrap()
}

fn prepared() -> &'static Fixture {
    static READY: OnceLock<Fixture> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        add_controls(&mut snapshot, &mut input);
        let bases = add_bases(&mut snapshot, &mut input);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../authored/rules/engine/index.yaml");
        let mut rules: Value = serde_saphyr::from_str(&read_to_string(path).unwrap()).unwrap();
        rules["input"] = rule_fixtures::rules(&snapshot, json!([]), json!([]))["input"].clone();
        let selected = Catalog::from_current_selection_with_rules(
            &snapshot,
            &keywords,
            &input,
            &rules.to_string(),
            &EngineIdentityInput::LegacyJp,
        )
        .unwrap();
        Fixture { selected, bases }
    })
}

fn fixture(number: &str, base: Option<&str>) -> Game {
    let mut state = draw_position();
    if let Some(base) = base {
        state["players"]["P1"]["zones"]["hand"] = json!([]);
        state["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
        state["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
    } else {
        state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    }
    state["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"selected","card":"banquet"},{"id":"wrong","card":"other"},
        {"id":"second","card":"amulet"},{"id":"third","card":"other"},
        {"id":"tail","card":"banquet"}
    ]);
    let mut game = Game::from_selected_effects(
        prepared().selected.clone(),
        &state,
        &Value::Null,
        &Value::Null,
        "top-bp14",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (control, ..) in CONTROLS {
        game.load_internal_card(control, &empty).unwrap();
    }
    if let Some(base) = base {
        game.load_internal_card(base, &prepared().bases[base])
            .unwrap();
    }
    game
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed private decision")
    };
    question
}

fn evolve(game: &mut Game, number: &str) -> DecisionView {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the synthetic base produces a genuine paid evolution event for the intact real evolved program"
    );
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("real evolution trigger")
    };
    let handle = options
        .iter()
        .find(|option| option.card == number)
        .unwrap()
        .handle;
    awaiting(
        game.answer_internal_card(question.actor, question.token, Answer::Pending(handle))
            .unwrap(),
    )
}

fn complete_look(game: &mut Game, question: &DecisionView) {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("observed Banquet choice")
    };
    assert_eq!(
        domain.members.len(),
        1_usize,
        "trait and Look receipt exclude three non-Banquet cards and the unseen eligible tail"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some("banquet"),
        "a nine-cost spell qualifies because kind and cost are unrestricted"
    );
    assert_eq!(
        domain.cardinality.minimum,
        Nat(0),
        "the printed choice is optional"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "at most one observed Banquet card may move to EX"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "the observer alone receives the hidden candidate question"
    );
    let order = awaiting(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    let InputDomain::Order(cards) = &order.domain else {
        panic!("private remainder ordering")
    };
    assert_eq!(
        cards.len(),
        3_usize,
        "only the three observed unselected cards are ordered"
    );
    resolved(
        game.answer_internal_card(
            order.actor,
            order.token,
            Answer::Order(cards.iter().rev().map(|card| card.handle).collect()),
        )
        .unwrap(),
    );
}

fn assert_result(game: &Game) {
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["ex"],
        json!(["selected"]),
        "the selected Banquet spell reaches EX"
    );
    assert_eq!(
        view["P1"]["deck"],
        json!(["tail", "third", "second", "wrong"]),
        "the unseen tail stays above the reversed observed remainder"
    );
    assert_eq!(
        view["P1"]["pp"]["current"], 9_u8,
        "only the one PP spell or evolution cost is paid"
    );
    let public = game.projection(View::P2).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    assert!(
        known.iter().any(|card| card["id"] == "selected"),
        "the selected EX arrival becomes public"
    );
    assert!(
        !known.iter().any(|card| card["id"] == "wrong"
            || card["id"] == "second"
            || card["id"] == "third"
            || card["id"] == "tail"),
        "the remaining observed cards and unseen tail stay private"
    );
}

fn evolution_scenario(number: &str, base: &str) {
    let mut game = fixture(number, Some(base));
    let question = evolve(&mut game, number);
    complete_look(&mut game, &question);
    assert_result(&game);
}

#[test]
fn bp14_009_and_p03_evolve_then_move_an_observed_banquet_card_to_ex() {
    for number in ["BP14-009", "BP14-P03"] {
        evolution_scenario(number, "elf-base");
    }
}

#[test]
fn bp14_075_and_pr368_evolve_then_move_an_observed_banquet_card_to_ex() {
    for number in ["BP14-075", "PR-368"] {
        evolution_scenario(number, "ansage-base");
    }
}

#[test]
fn bp14_115_and_pr385_play_the_real_spell_and_keep_the_remainder_private() {
    for number in ["BP14-115", "PR-385"] {
        let mut game = fixture(number, None);
        let question = awaiting(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
        );
        complete_look(&mut game, &question);
        assert_result(&game);
        assert!(
            game.projection(View::Referee).unwrap()["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            "the actual spell reaches the cemetery after resolution"
        );
    }
}
