#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the printed union searches."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::SearchAnswer;

const CONTROLS: [(&str, &str, &str, u8, bool); 9] = [
    ("cp-follower", "同じ名前の仲間", "フォロワー", 1, true),
    ("cp-amulet", "別の名前の支援", "アミュレット", 1, true),
    ("cp-same-amulet", "同じ名前の仲間", "アミュレット", 1, true),
    ("cp-spell", "別の名前の魔法", "スペル", 1, true),
    ("cp-zero", "零コストの支援", "アミュレット", 0, true),
    ("cp-two", "二コストの仲間", "フォロワー", 2, true),
    ("cp-other-trait", "特性の異なる仲間", "フォロワー", 1, false),
    ("cp-costly-spell", "二コストの魔法", "スペル", 2, true),
    (
        "cp-other-amulet",
        "他の特性の支援",
        "アミュレット",
        1,
        false,
    ),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind, cost, deresmas) in CONTROLS {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":kind,
                "traits":if deresmas {vec!["デレマス"]} else {vec![]},
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

fn fixture(state: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        state,
        &Value::Null,
        &Value::Null,
        "cp02-union",
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

fn position(number: &str) -> Value {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    state["players"]["P1"]["zones"]["deck"] = json!(
        CONTROLS
            .iter()
            .map(|(card, ..)| json!({"id":card,"card":card}))
            .collect::<Vec<_>>()
    );
    state
}

fn enter(game: &mut Game, number: &str) -> CardPlay {
    let played = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(played, CardPlay::Entered(_)),
        "the real repository source enters the field"
    );
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed fanfare")
    };
    let handle = options
        .iter()
        .find(|o| o.card == number && o.line == 1)
        .unwrap()
        .handle;
    game.answer_internal_card(q.actor, q.token, Answer::Pending(handle))
        .unwrap()
}

#[test]
fn cp02_008_searches_two_distinct_names_across_follower_and_amulet() {
    let mut state = position("CP02-008");
    state["players"]["P1"]["zones"]["deck"][0]["state"] = json!({"cost":8_u8});
    state["players"]["P1"]["zones"]["deck"][4]["state"] = json!({"cost":1_u8});
    let mut game = fixture(&state);
    let CardPlay::Awaiting(q) = enter(&mut game, "CP02-008") else {
        panic!("printed private search")
    };
    let InputDomain::Search(domain) = &q.domain else {
        panic!("private union domain")
    };
    let mut candidates = domain
        .cards
        .iter()
        .map(|c| c.card.as_str())
        .collect::<Vec<_>>();
    candidates.sort_unstable();
    assert_eq!(
        candidates,
        vec!["cp-amulet", "cp-follower", "cp-same-amulet"],
        "both branches require printed cost one and the Deresmas trait"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(2)),
        "the printed search allows at most two cards"
    );
    assert_eq!(
        domain.groups.as_ref().map(Vec::len),
        Some(1_usize),
        "the distinct-name search exposes one selection group"
    );
    let handle = |number| {
        domain
            .cards
            .iter()
            .find(|c| c.card == number)
            .unwrap()
            .handle
    };
    let follower = handle("cp-follower");
    let amulet = handle("cp-amulet");
    let duplicate = handle("cp-same-amulet");
    let before = game.projection(View::Referee).unwrap();
    let duplicate_error = game
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::Search(SearchAnswer::Grouped(vec![vec![follower, duplicate]])),
        )
        .expect_err("equal names remain illegal even across different card kinds");
    assert!(
        duplicate_error
            .to_string()
            .contains("search names must differ"),
        "duplicate names are rejected after the grouped answer shape has been accepted"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "a rejected same-name answer preserves the position"
    );
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Search(SearchAnswer::Grouped(vec![vec![follower, amulet]])),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    let field = view["P1"]["field"].as_array().unwrap();
    assert_eq!(
        field.len(),
        3_usize,
        "source and both found cards occupy the field"
    );
    assert!(
        field.contains(&json!("cp-follower")) && field.contains(&json!("cp-amulet")),
        "the authored union moves a follower and an amulet to the printed destination"
    );
    assert_eq!(
        view["P1"]["deck_count"], 7_u8,
        "only the two differently named cards leave the deck"
    );
}

#[test]
fn cp02_059_counts_only_own_followers_then_searches_either_nonfollower_kind() {
    for chosen in ["cp-other-amulet", "cp-spell"] {
        let mut state = position("CP02-059");
        state["players"]["P1"]["zones"]["field"] = json!([
            {"id":"ally-a","card":"cp-follower"},{"id":"ally-b","card":"cp-follower"},
            {"id":"support","card":"cp-amulet"}
        ]);
        let mut game = fixture(&state);
        let CardPlay::Awaiting(q) = enter(&mut game, "CP02-059") else {
            panic!("three followers enable the search")
        };
        let InputDomain::Search(domain) = &q.domain else {
            panic!("private nonfollower union")
        };
        let mut candidates = domain
            .cards
            .iter()
            .map(|c| c.card.as_str())
            .collect::<Vec<_>>();
        candidates.sort_unstable();
        assert_eq!(
            candidates,
            vec!["cp-amulet", "cp-other-amulet", "cp-same-amulet", "cp-spell"],
            "the union admits cost-one spells and amulets but excludes followers and other costs"
        );
        let handle = domain
            .cards
            .iter()
            .find(|c| c.card == chosen)
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
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!([chosen]),
            "each branch independently reaches the hand"
        );
    }
    let mut too_few = position("CP02-059");
    too_few["players"]["P1"]["zones"]["field"] = json!([
        {"id":"ally","card":"cp-follower"},{"id":"support","card":"cp-amulet"}
    ]);
    too_few["players"]["P2"]["zones"]["field"] = json!([{"id":"opponent","card":"cp-follower"}]);
    let mut insufficient = fixture(&too_few);
    resolved(enter(&mut insufficient, "CP02-059"));
    let view = insufficient.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["hand"],
        json!([]),
        "own amulets and opposing followers do not satisfy the three-follower threshold"
    );
    assert_eq!(
        view["P1"]["deck_count"], 9_u8,
        "a false condition never searches or moves a deck card"
    );
}
