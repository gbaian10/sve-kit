#![expect(
    clippy::panic,
    reason = "Unexpected decisions indicate a broken card fixture."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::SearchAnswer;

const CONTROLS: [(&str, bool, u8); 6] = [
    ("amulet-one", true, 1),
    ("amulet-zero", true, 0),
    ("amulet-two", true, 2),
    ("follower-one", false, 1),
    ("payment", true, 3),
    ("spare", false, 0),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, amulet, cost) in CONTROLS {
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":number,"card_class":"ニュートラル","card_type":if amulet {"アミュレット"} else {"フォロワー"},
                "traits":[],"cost":cost.to_string(),
                "power":if amulet {"-"} else {"1"},"hp":if amulet {"-"} else {"3"},"text":null,"sections":[]
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
        {"id":"source","card":"BP05-091"},{"id":"payment","card":"payment"},{"id":"spare","card":"spare"}
    ]);
    state["players"]["P1"]["zones"]["deck"] = json!(
        CONTROLS
            .iter()
            .filter(|(number, ..)| !["payment", "spare"].contains(number))
            .map(|(number, ..)| json!({"id":number,"card":number}))
            .collect::<Vec<_>>()
    );
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        &state,
        &Value::Null,
        &Value::Null,
        "bp05-search",
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

fn pay_for_search(game: &mut Game) -> CardPlay {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("fanfare")
    };
    let trigger = options
        .iter()
        .find(|option| option.card == "BP05-091" && option.line == 2)
        .unwrap()
        .handle;
    let CardPlay::Awaiting(optional) = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(trigger))
        .unwrap()
    else {
        panic!("optional payment")
    };
    assert!(
        matches!(optional.domain, InputDomain::OptionalCost),
        "the printed discard is an optional trigger payment"
    );
    let CardPlay::Awaiting(payment) = game
        .answer_internal_card(optional.actor, optional.token, Answer::OptionalCost(true))
        .unwrap()
    else {
        panic!("amulet discard")
    };
    let InputDomain::CostMaterials(materials) = &payment.domain else {
        panic!("discard materials")
    };
    assert_eq!(
        materials.members.len(),
        1_usize,
        "only the hand amulet can pay, not the spare follower"
    );
    let play = game
        .answer_internal_card(
            payment.actor,
            payment.token,
            Answer::CostMaterials(vec![materials.members[0].handle]),
        )
        .unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["payment"]),
        "the amulet reaches cemetery before the search answer"
    );
    play
}

#[test]
fn bp05_091_keeps_ward_and_discards_an_amulet_before_searching_one_cost_amulet_to_field() {
    let mut game = fixture();
    enter_source(&mut game);
    let CardPlay::Awaiting(q) = pay_for_search(&mut game) else {
        panic!("printed search")
    };
    let InputDomain::Search(domain) = &q.domain else {
        panic!("original cost one amulet domain")
    };
    assert_eq!(
        domain
            .cards
            .iter()
            .map(|card| card.card.as_str())
            .collect::<Vec<_>>(),
        vec!["amulet-one"],
        "exact original cost one excludes both zero and two cost amulets and a one cost follower"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "the printed search finds exactly one amulet"
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
    let field = view["P1"]["field"].as_array().unwrap();
    assert_eq!(
        field.len(),
        2_usize,
        "the source and found amulet are both on the field"
    );
    assert!(
        field.contains(&json!("source")) && field.contains(&json!("amulet-one")),
        "the selected amulet enters the field directly"
    );
    assert_eq!(
        view["P1"]["hand"],
        json!(["spare"]),
        "the unrelated hand follower is not discarded"
    );
    assert_eq!(
        view["P1"]["deck_count"], 3_u8,
        "only the found amulet leaves the deck"
    );
}
