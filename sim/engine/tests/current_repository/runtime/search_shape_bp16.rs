#![expect(
    clippy::panic,
    reason = "Unexpected decisions indicate a broken evolution fixture."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::SearchAnswer;

const CONTROLS: [(&str, &str, &str, &str, u8); 4] = [
    (
        "amulet-three",
        "三費護符",
        "ニュートラル",
        "アミュレット",
        3,
    ),
    ("amulet-five", "五費護符", "ニュートラル", "アミュレット", 5),
    ("amulet-six", "六費護符", "ニュートラル", "アミュレット", 6),
    ("follower-one", "一費從者", "ニュートラル", "フォロワー", 1),
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
    state["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    state["players"]["P1"]["pp"] = json!({"current":2_u8,"max":7_u8});
    state["players"]["P1"]["sep"] = json!(1_u8);
    state["players"]["P1"]["leader"]["life"] = json!(10_u8);
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP16-094"}]);
    state["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP16-095"}]);
    state["players"]["P1"]["zones"]["deck"] = json!(
        CONTROLS
            .iter()
            .map(|(number, ..)| {
                let mut card = json!({"id":number,"card":number});
                card["state"] = json!({"cost":if *number=="amulet-three" {8_u8} else {0_u8}});
                card
            })
            .collect::<Vec<_>>()
    );
    state["players"]["P2"]["zones"]["field"] =
        json!([{"id":"opponent-amulet","card":"amulet-five"}]);
    state["players"]["P2"]["zones"]["deck"] = json!([{"id":"opponent-draw","card":"follower-one"}]);
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        &state,
        &Value::Null,
        &Value::Null,
        "bp16-search",
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

fn evolve(game: &mut Game, super_evolution: bool) {
    let pay = if super_evolution {
        json!({"pp":1_u8,"sep":1_u8})
    } else {
        json!({"pp":1_u8})
    };
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":pay})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the current repository base performs real ordinary or super evolution"
    );
}

fn find_amulet(game: &mut Game, line: u64, expected: &[&str], choose: &str) {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("evolution head")
    };
    let trigger = options
        .iter()
        .find(|option| option.card == "BP16-095" && option.line == line)
        .unwrap()
        .handle;
    let CardPlay::Awaiting(q) = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(trigger))
        .unwrap()
    else {
        panic!("printed search")
    };
    let InputDomain::Search(domain) = &q.domain else {
        panic!("amulet cost domain")
    };
    let mut cards = domain
        .cards
        .iter()
        .map(|card| card.card.as_str())
        .collect::<Vec<_>>();
    cards.sort_unstable();
    assert_eq!(
        cards.as_slice(),
        expected,
        "each printed head applies its original-cost bound and amulet kind despite changed effective costs"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "each head searches one amulet"
    );
    let chosen = domain
        .cards
        .iter()
        .find(|card| card.card == choose)
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![chosen])),
        )
        .unwrap(),
    );
    assert!(
        game.projection(View::Referee).unwrap()["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!(choose)),
        "the searched amulet enters the field directly"
    );
    game.resume_internal_card().unwrap();
}

fn end_phase(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the evolved source reaches its own end phase"
    );
    loop {
        while let Some(q) = game.internal_pending_choice().unwrap() {
            let InputDomain::Pending(options) = &q.domain else {
                panic!("end trigger")
            };
            resolved(
                game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                    .unwrap(),
            );
            game.resume_internal_card().unwrap();
        }
        let Some(quick) = game.internal_quick_choice().unwrap() else {
            break;
        };
        assert!(
            matches!(
                game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
                    .unwrap(),
                CardPlay::Continued(_)
            ),
            "passing Quick advances the rule procedure rather than resolving a card ability"
        );
    }
}

#[test]
fn bp16_095_searches_both_evolution_cost_bounds_and_heals_only_with_two_own_amulets() {
    let ordinary = fixture();
    for super_evolution in [false, true] {
        let mut game = ordinary.clone();
        evolve(&mut game, super_evolution);
        find_amulet(&mut game, 2, &["amulet-three"], "amulet-three");
        if super_evolution {
            find_amulet(&mut game, 3, &["amulet-five"], "amulet-five");
        }
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "ordinary evolution has one head while super evolution resolves both printed heads"
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["deck_count"],
            if super_evolution { 2_u8 } else { 3_u8 },
            "each resolved head removes exactly its found card from the deck"
        );
        assert_eq!(
            view["P1"]["sep"],
            u8::from(!super_evolution),
            "only super evolution spends the SEP"
        );
        end_phase(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            if super_evolution { 12_u8 } else { 10_u8 },
            "two own amulets heal two, but one own plus one opposing amulet does not"
        );
    }
}
