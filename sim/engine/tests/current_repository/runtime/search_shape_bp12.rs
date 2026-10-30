#![expect(
    clippy::panic,
    reason = "Unexpected spell decisions indicate a broken test fixture."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::SearchAnswer;

const CONTROLS: [(&str, &str, &str, &str, u8); 4] = [
    ("munyar-a", "ムニャールA", "ビショップ", "フォロワー", 1),
    (
        "munyar-copy",
        "ムニャールA",
        "ニュートラル",
        "アミュレット",
        1,
    ),
    (
        "munyar-b",
        "別のムニャールB",
        "ニュートラル",
        "アミュレット",
        9,
    ),
    ("other", "別の仲間", "ビショップ", "フォロワー", 1),
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
    state["players"]["P1"]["leader"]["life"] = json!(10_u8);
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP12-092"}]);
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
        "bp12-search",
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
fn bp12_092_chooses_heal_draw_or_differently_named_munyar_field_search() {
    let mut game = fixture();
    let CardPlay::Awaiting(modes) = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("printed spell modes")
    };
    let CardPlay::Awaiting(q) = game
        .answer_internal_card(modes.actor, modes.token, Answer::Modes(vec![ModeId(2)]))
        .unwrap()
    else {
        panic!("printed search")
    };
    let InputDomain::Search(domain) = &q.domain else {
        panic!("Munyar search domain")
    };
    let mut candidates = domain
        .cards
        .iter()
        .map(|card| card.card.as_str())
        .collect::<Vec<_>>();
    candidates.sort_unstable();
    assert_eq!(
        candidates,
        vec!["munyar-a", "munyar-b", "munyar-copy"],
        "name fragments match without a kind, class or cost restriction"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(2)),
        "up to two cards may be found"
    );
    let handle = |number: &str| {
        domain
            .cards
            .iter()
            .find(|card| card.card == number)
            .unwrap()
            .handle
    };
    let first = handle("munyar-a");
    let duplicate = handle("munyar-copy");
    let second = handle("munyar-b");
    let before = game.projection(View::Referee).unwrap();
    let rejected = game
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::Search(SearchAnswer::Grouped(vec![vec![first, duplicate]])),
        )
        .expect_err("same names cannot both be found even across card kinds");
    assert!(
        rejected.to_string().contains("search names must differ"),
        "duplicate names are rejected for the printed restriction"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "rejected selection leaves the position unchanged"
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
    assert_eq!(field.len(), 2_usize, "both found cards enter the field");
    assert!(
        field.contains(&json!("munyar-a")) && field.contains(&json!("munyar-b")),
        "the differently named follower and amulet reach the printed destination"
    );
    assert_eq!(
        view["P1"]["deck_count"], 2_u8,
        "only the chosen two leave the deck"
    );
    assert_eq!(
        view["P1"]["leader"]["life"], 10_u8,
        "the other mode does not heal"
    );

    check_healing_mode();
}

fn check_healing_mode() {
    let mut healing = fixture();
    let CardPlay::Awaiting(healing_modes) = healing
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("printed spell healing_modes")
    };
    resolved(
        healing
            .answer_internal_card(
                healing_modes.actor,
                healing_modes.token,
                Answer::Modes(vec![ModeId(1)]),
            )
            .unwrap(),
    );
    let healed = healing.projection(View::Referee).unwrap();
    assert_eq!(
        healed["P1"]["leader"]["life"], 12_u8,
        "the first mode heals two"
    );
    assert_eq!(
        healed["P1"]["hand"].as_array().unwrap().len(),
        2_usize,
        "the first mode draws two"
    );
    assert_eq!(
        healed["P1"]["deck_count"], 2_u8,
        "both drawn cards leave the deck"
    );
    assert_eq!(
        healed["P1"]["field"],
        json!([]),
        "the first mode does not perform the field search"
    );
}
