use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::SearchAnswer;

const CONTROLS: [(&str, &str, bool, bool); 5] = [
    ("city-good", "別の区役所カード", true, true),
    ("city-same", "区役所の丙種職員", true, false),
    ("city-other", "別の仲間", false, false),
    ("city-spell", "別の魔法", false, true),
    ("payment", "支払いの仲間", false, false),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, city, spell) in CONTROLS {
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":if spell {"スペル"} else {"フォロワー"},
                "traits":if city {vec!["区役所"]} else {vec![]},"cost":"1",
                "power":if spell {"-"} else {"1"},"hp":if spell {"-"} else {"3"},"text":null,"sections":[]
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
    state["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP18-126"},{"id":"payment","card":"payment"}
    ]);
    state["players"]["P1"]["zones"]["deck"] = json!(
        CONTROLS
            .iter()
            .filter(|(number, ..)| *number != "payment")
            .map(|(number, ..)| json!({"id":number,"card":number}))
            .collect::<Vec<_>>()
    );
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        &state,
        &Value::Null,
        &Value::Null,
        "bp18-search",
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
fn bp18_126_discards_before_searching_another_named_city_hall_card() {
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
        .find(|option| option.card == "BP18-126" && option.line == 1)
        .unwrap()
        .handle;
    let CardPlay::Awaiting(optional) = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(trigger))
        .unwrap()
    else {
        panic!("optional trigger payment")
    };
    assert!(
        matches!(optional.domain, InputDomain::OptionalCost),
        "the discard is paid before the search is performed"
    );
    let CardPlay::Awaiting(payment) = game
        .answer_internal_card(optional.actor, optional.token, Answer::OptionalCost(true))
        .unwrap()
    else {
        panic!("discard material")
    };
    let InputDomain::CostMaterials(materials) = &payment.domain else {
        panic!("discard material domain")
    };
    assert_eq!(
        materials.members.len(),
        1_usize,
        "only the remaining hand card can pay the printed cost"
    );
    let CardPlay::Awaiting(q) = game
        .answer_internal_card(
            payment.actor,
            payment.token,
            Answer::CostMaterials(vec![materials.members[0].handle]),
        )
        .unwrap()
    else {
        panic!("printed private search")
    };
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["payment"]),
        "the hand card reaches cemetery before the search answer"
    );
    let InputDomain::Search(domain) = &q.domain else {
        panic!("City Hall card domain")
    };
    assert_eq!(
        domain
            .cards
            .iter()
            .map(|card| card.card.as_str())
            .collect::<Vec<_>>(),
        vec!["city-good"],
        "same-name and non-City-Hall cards are excluded while a matching spell remains eligible"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "the printed search finds one card"
    );
    assert!(
        domain.groups.is_none(),
        "the question advertises an ungrouped selection"
    );
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["hand"],
        json!(["city-good"]),
        "the chosen different-name City Hall card reaches hand"
    );
    assert_eq!(
        view["P1"]["deck_count"], 3_u8,
        "only the eligible chosen card leaves the deck"
    );
}
