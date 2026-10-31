#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed Priconne follower Look scenario."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::DecisionView;

const CONTROLS: [(&str, &str, &str, bool); 3] = [
    ("priconne", "プリコネの仲間", "フォロワー", true),
    ("spell", "プリコネの魔法", "スペル", true),
    ("other", "別の仲間", "フォロワー", false),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind, priconne) in CONTROLS {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":kind,
                "traits":if priconne {vec!["プリコネ"]} else {vec![]},"cost":"9",
                "power":if follower {"1"} else {"-"},"hp":if follower {"9"} else {"-"},
                "text":null,"sections":[]
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

fn fixture(number: &str) -> Game {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    state["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"selected","card":"priconne"},{"id":"wrong-kind","card":"spell"},
        {"id":"wrong-trait","card":"other"},{"id":"third","card":"other"},
        {"id":"tail","card":"priconne"}
    ]);
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        &state,
        &Value::Null,
        &Value::Null,
        "top-cp04",
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

fn take_follower(game: &mut Game, question: &DecisionView) {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("observed Priconne follower domain")
    };
    assert_eq!(
        domain.members.len(),
        1_usize,
        "kind and trait reject each observed counterexample and the unseen eligible tail"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some("priconne"),
        "the nine-cost Priconne follower qualifies without a cost restriction"
    );
    assert_eq!(
        domain.cardinality.minimum,
        Nat(0),
        "the printed choice is optional"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "at most one observed follower may move to EX"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "only the observer sees the private candidate question"
    );
    let order = awaiting(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    let InputDomain::Order(cards) = &order.domain else {
        panic!("private remainder ordering")
    };
    assert_eq!(
        cards.len(),
        3_usize,
        "only the three unselected observed cards are ordered"
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

fn assert_result(game: &Game) {
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["ex"],
        json!(["selected"]),
        "the selected Priconne follower reaches EX"
    );
    assert_eq!(
        view["P1"]["deck"],
        json!(["tail", "third", "wrong-trait", "wrong-kind"]),
        "the unobserved tail stays above the privately reversed remainder"
    );
    assert_eq!(
        view["P1"]["cemetery"],
        json!(["source"]),
        "the actual spell reaches the cemetery after resolution"
    );
    assert_eq!(
        view["P1"]["pp"]["current"], 9_u8,
        "the actual one-cost spell pays exactly one PP"
    );
    let public = game.projection(View::P2).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    assert!(
        known.iter().any(|card| card["id"] == "selected"),
        "the EX arrival becomes public"
    );
    assert!(
        !known.iter().any(|card| card["id"] == "wrong-kind"
            || card["id"] == "wrong-trait"
            || card["id"] == "third"
            || card["id"] == "tail"),
        "unselected observed cards and the unseen tail remain private"
    );
}

#[test]
fn cp04_118_and_p90_play_the_real_spell_and_select_only_observed_priconne_followers() {
    for number in ["CP04-118", "CP04-P90"] {
        let mut game = fixture(number);
        let question = awaiting(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
        );
        take_follower(&mut game, &question);
        assert_result(&game);
    }
}
