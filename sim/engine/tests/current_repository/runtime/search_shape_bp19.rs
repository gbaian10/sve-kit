#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the printed union searches."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::SearchAnswer;

const CONTROLS: [(&str, &str, &str, u8, bool); 7] = [
    ("beast-follower", "零費の獣", "フォロワー", 0, true),
    ("beast-amulet", "一費の獣", "アミュレット", 1, true),
    ("costly-follower", "二費の獣", "フォロワー", 2, true),
    ("costly-amulet", "二費の獣護符", "アミュレット", 2, true),
    ("beast-spell", "獣の魔法", "スペル", 0, true),
    ("other-follower", "別の仲間", "フォロワー", 1, false),
    ("other-amulet", "別の護符", "アミュレット", 1, false),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind, cost, beast) in CONTROLS {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":kind,
                "traits":if beast {vec!["獣"]} else {vec![]},
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
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP19-106"}]);
    state["players"]["P1"]["zones"]["deck"] = json!(
        CONTROLS
            .iter()
            .map(|(number, ..)| {
                let mut card = json!({"id":number,"card":number});
                card["state"] = json!({"cost":if number.starts_with("beast-") {8_u8} else {0_u8}});
                card
            })
            .collect::<Vec<_>>()
    );
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        &state,
        &Value::Null,
        &Value::Null,
        "bp19-union",
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

fn begin_search(game: &mut Game) -> CardPlay {
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the real repository source enters the field"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("printed fanfare")
    };
    let trigger = options
        .iter()
        .find(|option| option.card == "BP19-106" && option.line == 1)
        .unwrap()
        .handle;
    game.answer_internal_card(pending.actor, pending.token, Answer::Pending(trigger))
        .unwrap()
}

#[test]
fn bp19_106_searches_either_low_original_cost_beast_follower_or_amulet_to_field() {
    let initial = fixture();
    for choose in ["beast-follower", "beast-amulet"] {
        let mut game = initial.clone();
        let CardPlay::Awaiting(q) = begin_search(&mut game) else {
            panic!("printed private search")
        };
        let InputDomain::Search(domain) = &q.domain else {
            panic!("Beast union search")
        };
        let mut cards = domain
            .cards
            .iter()
            .map(|card| card.card.as_str())
            .collect::<Vec<_>>();
        cards.sort_unstable();
        assert_eq!(
            cards,
            vec!["beast-amulet", "beast-follower"],
            "both union branches use original cost at most one and Beast trait; spells, other traits and printed two-cost cards are excluded despite discounts"
        );
        assert_eq!(
            domain.cardinality.maximum,
            Some(Nat(1)),
            "the printed union finds one card total"
        );
        assert!(
            domain.groups.is_none(),
            "the union shares one ungrouped search answer"
        );
        let handle = domain
            .cards
            .iter()
            .find(|card| card.card == choose)
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
        let field = view["P1"]["field"].as_array().unwrap();
        assert_eq!(
            field.len(),
            2_usize,
            "the source and exactly one found card occupy the field"
        );
        assert!(
            field.contains(&json!("source")) && field.contains(&json!(choose)),
            "both follower and amulet branches independently reach the field"
        );
        assert_eq!(
            view["P1"]["deck_count"], 6_u8,
            "only the chosen union member leaves the deck"
        );
        assert_eq!(
            view["P1"]["hand"],
            json!([]),
            "the searched card is not added to hand"
        );
    }
}
