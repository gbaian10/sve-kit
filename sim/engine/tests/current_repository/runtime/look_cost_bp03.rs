#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate this printed spell scenario."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::DecisionView;

const CONTROLS: [(&str, &str, &str, u8, bool); 4] = [
    ("chess-same", "チェック", "スペル", 1, true),
    ("chess-good", "別のチェスカード", "スペル", 9, true),
    ("other", "別の仲間", "フォロワー", 1, false),
    ("enemy", "相手の仲間", "フォロワー", 1, false),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind, cost, chess) in CONTROLS {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":kind,
                "traits":if chess {vec!["チェス"]} else {vec![]},
                "cost":cost.to_string(),"power":if follower {"1"} else {"-"},
                "hp":if follower {"7"} else {"-"},"text":null,"sections":[]
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
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP03-044"}]);
    state["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"same","card":"chess-same"},{"id":"selected","card":"chess-good"},
        {"id":"other","card":"other"},{"id":"tail","card":"chess-good"}
    ]);
    state["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"enemy"}]);
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        &state,
        &Value::Null,
        &Value::Null,
        "look-bp03",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (control, ..) in CONTROLS {
        game.load_internal_card(control, &empty).unwrap();
    }
    game
}

fn finish_look(game: &mut Game, q: &DecisionView) {
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("private Chess choice")
    };
    assert_eq!(
        domain.members.len(),
        1_usize,
        "same-name, other traits and the eligible but unobserved tail are excluded"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some("chess-good"),
        "the differently named Chess spell is eligible regardless of its high cost"
    );
    assert_eq!(
        domain.cardinality.minimum,
        Nat(0),
        "the printed choice may be declined"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "at most one observed card is chosen"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "only the controller sees the Look question"
    );
    let CardPlay::Awaiting(order) = game
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap()
    else {
        panic!("remainder ordering")
    };
    let InputDomain::Order(cards) = &order.domain else {
        panic!("private bottom order")
    };
    assert_eq!(
        cards.len(),
        2_usize,
        "only the two unchosen observed cards require ordering"
    );
    resolved(
        game.answer_internal_card(
            order.actor,
            order.token,
            Answer::Order(cards.iter().rev().map(|card| card.handle).collect()),
        )
        .unwrap(),
    );
}

#[test]
fn bp03_044_targets_and_damages_before_looking_for_a_differently_named_chess_card() {
    let mut game = fixture();
    let CardPlay::Awaiting(target) = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("formal opposing follower target")
    };
    let InputDomain::Targets(domain) = &target.domain else {
        panic!("formal targets")
    };
    assert_eq!(
        domain.members.len(),
        1_usize,
        "only the opposing follower can be targeted"
    );
    assert!(
        domain.members.iter().all(|member| member.player.seat == 2),
        "the spell targets the opposing field"
    );
    let CardPlay::Awaiting(q) = game
        .answer_internal_card(
            target.actor,
            target.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap()
    else {
        panic!("private Look after damage")
    };
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
        3_u8,
        "the printed four damage happens before the private Look choice"
    );
    finish_look(&mut game, &q);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["hand"],
        json!(["selected"]),
        "the chosen differently named Chess card reaches hand"
    );
    assert_eq!(
        view["P1"]["deck"],
        json!(["tail", "other", "same"]),
        "the untouched tail stays on top and the remainder follows the selected bottom order"
    );
    assert_eq!(
        view["P1"]["cemetery"],
        json!(["source"]),
        "the real spell completes and reaches cemetery"
    );
    let opponent = game.projection(View::P2).unwrap();
    let known = opponent["known_cards"].as_array().unwrap();
    assert!(
        known.iter().any(|card| card["id"] == "selected"),
        "only the selected hand card is revealed by the printed instruction"
    );
    assert!(
        !known
            .iter()
            .any(|card| ["same", "other", "tail"].iter().any(|id| card["id"] == *id)),
        "the private remainder and untouched tail stay hidden"
    );
}
