use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::SearchAnswer;

const CONTROLS: [(&str, &str, &str, &str, u8); 6] = [
    ("elf-a", "森の魔法A", "エルフ", "スペル", 1),
    ("elf-a-copy", "森の魔法A", "エルフ", "スペル", 1),
    ("elf-b", "森の魔法B", "エルフ", "スペル", 9),
    ("elf-follower", "森の仲間", "エルフ", "フォロワー", 1),
    ("royal-spell", "王の魔法", "ロイヤル", "スペル", 1),
    ("elf-amulet", "森の支援", "エルフ", "アミュレット", 1),
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
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP20-011"}]);
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
        "bp20-search",
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
fn bp20_011_searches_up_to_two_differently_named_elf_spells() {
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
        .find(|option| option.card == "BP20-011" && option.line == 1)
        .unwrap()
        .handle;
    let CardPlay::Awaiting(q) = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(trigger))
        .unwrap()
    else {
        panic!("printed private search")
    };
    let InputDomain::Search(domain) = &q.domain else {
        panic!("Elf spell domain")
    };
    let mut candidates = domain
        .cards
        .iter()
        .map(|card| card.card.as_str())
        .collect::<Vec<_>>();
    candidates.sort_unstable();
    assert_eq!(
        candidates,
        vec!["elf-a", "elf-a-copy", "elf-b"],
        "only Elf spells match, including the high-cost spell because there is no cost bound"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(2)),
        "the printed search allows at most two cards"
    );
    assert_eq!(
        domain.groups.as_ref().map(Vec::len),
        Some(1_usize),
        "different names are selected through the advertised group"
    );
    let handle = |number: &str| {
        domain
            .cards
            .iter()
            .find(|card| card.card == number)
            .unwrap()
            .handle
    };
    let first = handle("elf-a");
    let duplicate = handle("elf-a-copy");
    let second = handle("elf-b");
    let before = game.projection(View::Referee).unwrap();
    let rejected = game
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::Search(SearchAnswer::Grouped(vec![vec![first, duplicate]])),
        )
        .expect_err("two physical cards with the same name cannot both be found");
    assert!(
        rejected.to_string().contains("search names must differ"),
        "the negative answer is rejected for names rather than shape"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "a rejected duplicate-name choice leaves the position unchanged"
    );
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Search(SearchAnswer::Grouped(vec![vec![first, second]])),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    let hand = view["P1"]["hand"].as_array().unwrap();
    assert_eq!(
        hand.len(),
        2_usize,
        "both differently named spells reach the hand"
    );
    assert!(
        hand.contains(&json!("elf-a")) && hand.contains(&json!("elf-b")),
        "the chosen spell objects reach the printed destination"
    );
    assert_eq!(
        view["P1"]["deck_count"], 4_u8,
        "only the two chosen spells leave the deck"
    );
}
