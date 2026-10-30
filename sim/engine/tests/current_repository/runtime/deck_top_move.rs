#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed top-card movement scenarios."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::DecisionView;

const CONTROLS: [(&str, &str, &str, u8, bool); 3] = [
    ("ocean", "海洋の魔法", "スペル", 9, true),
    ("other", "別の仲間", "フォロワー", 1, false),
    ("amulet", "別の護符", "アミュレット", 1, false),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind, cost, ocean) in CONTROLS {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":kind,
                "traits":if ocean {vec!["海洋"]} else {vec![]},
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

fn fixture(number: &str, full_ex: bool) -> Game {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    state["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"selected","card":"ocean"},{"id":"wrong","card":"other"},
        {"id":"amulet","card":"amulet"},{"id":"tail","card":"ocean"}
    ]);
    if number == "BP10-060" {
        state["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"d0","card":"ocean"},{"id":"d1","card":"other"},
            {"id":"d2","card":"amulet"},{"id":"d3","card":"ocean"},
            {"id":"d4","card":"other"},{"id":"tail","card":"ocean"}
        ]);
    }
    if full_ex {
        state["players"]["P1"]["zones"]["ex"] = json!(
            (0_u8..5)
                .map(|i| json!({"id":format!("ex{i}"),"card":"amulet"}))
                .collect::<Vec<_>>()
        );
    }
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        &state,
        &Value::Null,
        &Value::Null,
        "deck-top",
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

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed private choice")
    };
    question
}

fn play_and_trigger(game: &mut Game, number: &str) -> CardPlay {
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(play, CardPlay::Entered(_)),
        "the real source enters before its printed fanfare"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("real printed fanfare")
    };
    let handle = options
        .iter()
        .find(|option| option.card == number)
        .unwrap()
        .handle;
    game.answer_internal_card(pending.actor, pending.token, Answer::Pending(handle))
        .unwrap()
}

fn ocean_choice(game: &mut Game) -> DecisionView {
    let question = awaiting(play_and_trigger(game, "BP11-067"));
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("Ocean candidates are private")
    };
    assert_eq!(
        domain.members.len(),
        1_usize,
        "trait and Look receipt exclude unrelated cards and the unseen Ocean tail"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some("ocean"),
        "any Ocean kind and cost qualifies, including a nine-cost spell"
    );
    assert_eq!(
        domain.cardinality.minimum,
        Nat(0),
        "putting the observed card into EX is optional"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "only one observed Ocean card is chosen"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "the opponent does not see the hidden candidates"
    );
    awaiting(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    )
}

fn order(game: &mut Game, question: &DecisionView, remaining: usize) {
    let InputDomain::Order(cards) = &question.domain else {
        panic!("private remainder ordering")
    };
    assert_eq!(
        cards.len(),
        remaining,
        "only successfully moved cards leave the observed remainder"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Order(cards.iter().rev().map(|card| card.handle).collect()),
        )
        .unwrap(),
    );
}

#[test]
fn bp10_060_fanfare_banishes_exactly_the_top_five_without_a_look() {
    let mut game = fixture("BP10-060", false);
    resolved(play_and_trigger(&mut game, "BP10-060"));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["banish"],
        json!(["d0", "d1", "d2", "d3", "d4"]),
        "the literal first five cards are banished in order, regardless of their kinds"
    );
    assert_eq!(
        view["P1"]["cemetery"],
        json!([]),
        "banishment is a distinct physical action"
    );
    assert_eq!(
        view["P1"]["deck"],
        json!(["tail"]),
        "only the top five leave and the untouched tail remains on top"
    );
    assert_eq!(
        view["P1"]["pp"]["current"], 7_u8,
        "the real source pays its printed three PP"
    );
    let public = game.projection(View::P2).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    for id in ["d0", "d1", "d2", "d3", "d4"] {
        assert!(
            known.iter().any(|card| card["id"] == id),
            "each completed face-up banishment becomes public"
        );
    }
    assert!(
        !known.iter().any(|card| card["id"] == "tail"),
        "direct banishment grants no Look or reveal of the remaining deck"
    );
}

#[test]
fn bp11_067_moves_only_the_observed_ocean_card_to_ex_and_orders_the_remainder() {
    let mut game = fixture("BP11-067", false);
    let remainder = ocean_choice(&mut game);
    order(&mut game, &remainder, 2);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["ex"],
        json!(["selected"]),
        "the observed Ocean spell reaches its controller's EX"
    );
    assert_eq!(
        view["P1"]["deck"],
        json!(["tail", "amulet", "wrong"]),
        "unseen tail stays above the privately reversed remainder"
    );
    let public = game.projection(View::P2).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    assert!(
        known.iter().any(|card| card["id"] == "selected"),
        "EX arrival makes the chosen card public"
    );
    assert!(
        !known.iter().any(|card| ["wrong", "amulet", "tail"]
            .iter()
            .any(|id| card["id"] == *id)),
        "other observed cards and unseen tail stay private"
    );
    full_ex_keeps_the_unmoved_card_in_the_remainder();
}

fn full_ex_keeps_the_unmoved_card_in_the_remainder() {
    let mut game = fixture("BP11-067", true);
    let remainder = ocean_choice(&mut game);
    order(&mut game, &remainder, 3);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["ex_count"], 5_u8,
        "a full EX accepts no extra arrival"
    );
    assert_eq!(
        view["P1"]["deck"],
        json!(["tail", "amulet", "wrong", "selected"]),
        "the failed move stays in the observed remainder and goes to the chosen bottom order"
    );
}
