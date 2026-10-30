use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::SearchAnswer;

const CONTROLS: [(&str, &str, &str, &str, u8); 5] = [
    ("royal-low", "王の仲間", "ロイヤル", "フォロワー", 1),
    ("royal-high", "大きな王の仲間", "ロイヤル", "フォロワー", 9),
    ("elf-follower", "森の仲間", "エルフ", "フォロワー", 1),
    ("royal-spell", "王の魔法", "ロイヤル", "スペル", 1),
    (
        "neutral-follower",
        "中立の仲間",
        "ニュートラル",
        "フォロワー",
        1,
    ),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, class, kind, cost) in CONTROLS {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":class,"card_type":kind,"traits":[],
                "cost":cost.to_string(),"power":if follower {"1"} else {"-"},
                "hp":if follower {"3"} else {"-"},"text":null,"sections":[]
            }]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([
                format!("{number}.yaml"),
                json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()
            ]));
        }
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../authored/rules/engine/index.yaml");
        let mut rules: Value = serde_saphyr::from_str(&read_to_string(path).unwrap()).unwrap();
        rules["input"] = rule_fixtures::rules(&snapshot, json!([]), json!([]))["input"].clone();
        Catalog::from_current_selection_with_rules(&snapshot, &keywords, &input, &rules.to_string(), &EngineIdentityInput::LegacyJp).unwrap()
    })
}

fn fixture() -> Game {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP01-036"}]);
    state["players"]["P1"]["zones"]["deck"] = json!(
        CONTROLS
            .iter()
            .map(|(number, ..)| json!({"id":number,"card":number}))
            .collect::<Vec<_>>()
    );
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        &state,
        &Value::Null,
        &Value::Null,
        "bp01-search",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in CONTROLS {
        game.load_internal_card(number, &empty).unwrap();
    }
    game
}

#[test]
fn bp01_036_searches_a_royal_follower_without_a_cost_bound() {
    let mut game = fixture();
    let played = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(played, CardPlay::Entered(_)),
        "the real repository source enters the field"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("printed fanfare")
    };
    let trigger = options
        .iter()
        .find(|option| option.card == "BP01-036" && option.line == 1)
        .unwrap()
        .handle;
    let CardPlay::Awaiting(q) = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(trigger))
        .unwrap()
    else {
        panic!("printed private search")
    };
    let InputDomain::Search(domain) = &q.domain else {
        panic!("Royal follower domain")
    };
    let mut candidates = domain
        .cards
        .iter()
        .map(|card| card.card.as_str())
        .collect::<Vec<_>>();
    candidates.sort_unstable();
    assert_eq!(
        candidates,
        vec!["royal-high", "royal-low"],
        "only Royal followers match, including a high-cost follower because there is no cost bound"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "the printed search finds one card"
    );
    assert!(
        domain.groups.is_none(),
        "the question advertises a single ungrouped selection"
    );
    let handle = domain
        .cards
        .iter()
        .find(|card| card.card == "royal-high")
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![handle])),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["hand"],
        json!(["royal-high"]),
        "the chosen Royal follower reaches the printed destination"
    );
    assert_eq!(
        view["P1"]["deck_count"], 4_u8,
        "only the chosen card leaves the deck"
    );
}
