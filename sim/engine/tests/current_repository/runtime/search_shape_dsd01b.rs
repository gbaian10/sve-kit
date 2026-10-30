#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed Dragon search."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::SearchAnswer;

const CONTROLS: [(&str, &str, &str, u8, bool); 7] = [
    ("dragon-a", "竜人A", "フォロワー", 0, true),
    ("dragon-copy", "竜人A", "フォロワー", 1, true),
    ("dragon-b", "竜人B", "フォロワー", 2, true),
    ("costly-dragon", "竜人C", "フォロワー", 3, true),
    ("other-follower", "別の仲間", "フォロワー", 1, false),
    ("dragon-spell", "竜人の魔法", "スペル", 1, true),
    ("dragon-amulet", "竜人の護符", "アミュレット", 1, true),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind, cost, martial) in CONTROLS {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":kind,
                "traits":if martial {vec!["武闘竜人"]} else {vec![]},
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
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"DSD01b-006"}]);
    state["players"]["P1"]["zones"]["deck"] = json!(
        CONTROLS
            .iter()
            .map(|(number, ..)| {
                let mut card = json!({"id":number,"card":number});
                card["state"] = json!({"cost":if number.starts_with("dragon-") {8_u8} else {0_u8}});
                card
            })
            .collect::<Vec<_>>()
    );
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        &state,
        &Value::Null,
        &Value::Null,
        "dsd01b-search",
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

fn enter_source(game: &mut Game) {
    let CardPlay::Awaiting(ward) = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("printed Ward entry")
    };
    assert!(
        matches!(ward.domain, InputDomain::WardEntry { .. }),
        "the real source retains its printed Ward"
    );
    assert!(
        matches!(
            game.answer_internal_card(ward.actor, ward.token, Answer::WardEntry(true))
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the source can enter acted using Ward"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
        true,
        "the chosen Ward posture applies on entry"
    );
}

fn begin_search(game: &mut Game) -> CardPlay {
    enter_source(game);
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("printed fanfare")
    };
    let trigger = options
        .iter()
        .find(|option| option.card == "DSD01b-006" && option.line == 2)
        .unwrap()
        .handle;
    game.answer_internal_card(pending.actor, pending.token, Answer::Pending(trigger))
        .unwrap()
}

#[test]
fn dsd01b_006_keeps_ward_and_searches_two_distinct_low_original_cost_martial_dragon_followers() {
    let mut game = fixture();
    let CardPlay::Awaiting(q) = begin_search(&mut game) else {
        panic!("printed search")
    };
    let InputDomain::Search(domain) = &q.domain else {
        panic!("Martial Dragon follower search")
    };
    let mut cards = domain
        .cards
        .iter()
        .map(|card| card.card.as_str())
        .collect::<Vec<_>>();
    cards.sort_unstable();
    assert_eq!(
        cards,
        vec!["dragon-a", "dragon-b", "dragon-copy"],
        "only Martial Dragon followers of original cost at most two match, regardless of effective costs"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(2)),
        "at most two cards can be found"
    );
    assert_eq!(
        domain.cardinality.minimum,
        Nat(0),
        "the printed up-to search permits finding fewer than two"
    );
    let handle = |number: &str| {
        domain
            .cards
            .iter()
            .find(|card| card.card == number)
            .unwrap()
            .handle
    };
    let first = handle("dragon-a");
    let duplicate = handle("dragon-copy");
    let second = handle("dragon-b");
    let before = game.projection(View::Referee).unwrap();
    let Err(error) = game.answer_internal_card(
        q.actor,
        q.token,
        Answer::Search(SearchAnswer::Grouped(vec![vec![first, duplicate]])),
    ) else {
        panic!("duplicate names are forbidden")
    };
    assert!(
        error.to_string().contains("search names must differ"),
        "the negative answer rejects names rather than the grouped answer shape"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "a rejected duplicate selection leaves the position unchanged"
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
    let field = view["P1"]["field"].as_array().unwrap();
    assert_eq!(
        field.len(),
        3_usize,
        "the source and two found followers occupy the field"
    );
    assert!(
        field.contains(&json!("dragon-a")) && field.contains(&json!("dragon-b")),
        "the differently named found followers enter the field directly"
    );
    assert_eq!(
        view["P1"]["deck_count"], 5_u8,
        "only the two chosen cards leave the deck"
    );
    assert_eq!(
        view["P1"]["hand"],
        json!([]),
        "the found followers are not added to hand"
    );
}
