use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::SearchAnswer;

const CONTROLS: [(&str, &str, bool, bool); 5] = [
    ("academy-good", "学院の仲間", true, false),
    ("academy-same", "プリンセスマナリア・アン", true, false),
    ("academy-spell", "学院の魔法", true, true),
    ("other-trait", "別の仲間", false, false),
    ("bp02-base", "プリンセスマナリア・アン", true, false),
];

fn prepared() -> &'static (SelectedEffects, WellFormedCardProgram) {
    static READY: OnceLock<(SelectedEffects, WellFormedCardProgram)> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, academy, spell) in CONTROLS {
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ウィッチ","card_type":if spell {"スペル"} else {"フォロワー"},
                "traits":if academy {vec!["学院"]} else {vec![]},"cost":"1",
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
        let selected = Catalog::from_current_selection_with_rules(&snapshot, &keywords, &input, &rules.to_string(), &EngineIdentityInput::LegacyJp).unwrap();
        let mut tree = expanded::tree().clone();
        tree["cards"] = json!({"bp02-base":tree["cards"]["synthetic-evolution"]});
        let base = lowerer().lower(&tree).unwrap().remove("bp02-base").unwrap();
        (selected, base)
    })
}

fn fixture() -> Game {
    let (selected, base) = prepared();
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"bp02-base"}]);
    state["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP02-039"}]);
    state["players"]["P1"]["zones"]["deck"] = json!(
        CONTROLS
            .iter()
            .filter(|(number, ..)| *number != "bp02-base")
            .map(|(number, ..)| json!({"id":number,"card":number}))
            .collect::<Vec<_>>()
    );
    let mut game = Game::from_selected_effects(
        selected.clone(),
        &state,
        &Value::Null,
        &Value::Null,
        "bp02-search",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in CONTROLS {
        game.load_internal_card(number, if number == "bp02-base" { base } else { &empty })
            .unwrap();
    }
    game
}

#[test]
fn bp02_039_real_evolution_searches_another_named_academy_follower() {
    let mut game = fixture();
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
    )
    .unwrap();
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("printed evolution trigger")
    };
    let trigger = options
        .iter()
        .find(|option| option.card == "BP02-039" && option.line == 1)
        .unwrap()
        .handle;
    let CardPlay::Awaiting(q) = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(trigger))
        .unwrap()
    else {
        panic!("printed private search")
    };
    let InputDomain::Search(domain) = &q.domain else {
        panic!("Academy follower domain")
    };
    assert_eq!(
        domain
            .cards
            .iter()
            .map(|card| card.card.as_str())
            .collect::<Vec<_>>(),
        vec!["academy-good"],
        "same-name cards, non-Academy followers and Academy spells are excluded"
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
        json!(["academy-good"]),
        "the chosen different-name Academy follower reaches hand"
    );
    assert_eq!(
        view["P1"]["deck_count"], 3_u8,
        "only the eligible chosen card leaves the deck"
    );
}
