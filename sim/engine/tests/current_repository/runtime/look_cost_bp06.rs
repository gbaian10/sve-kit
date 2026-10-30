#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these printed Hunter Look scenarios."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::DecisionView;

const CONTROLS: [(&str, &str, &str, u8, bool); 5] = [
    ("hunter-four", "四費の狩人", "フォロワー", 4, true),
    ("hunter-five", "五費の狩人", "フォロワー", 5, true),
    ("hunter-spell", "狩人の魔法", "スペル", 1, true),
    ("other", "別の仲間", "フォロワー", 1, false),
    ("enemy", "大きな相手", "フォロワー", 9, false),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind, cost, hunter) in CONTROLS {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":kind,
                "traits":if hunter {vec!["狩人"]} else {vec![]},
                "cost":cost.to_string(),"power":if follower {"1"} else {"-"},
                "hp":if follower {"9"} else {"-"},"text":null,"sections":[]
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
    if number == "BP06-001" {
        state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    } else {
        state["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP06-001"}]);
        state["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
    }
    state["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"selected","card":"hunter-four","state":{"cost":8_u8}},
        {"id":"wrong","card":"hunter-five","state":{"cost":1_u8}},
        {"id":"spell","card":"hunter-spell"},
        {"id":"other","card":"other"},
        {"id":"tail","card":"hunter-four"}
    ]);
    state["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"enemy","state":{"acted":true}}]);
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        &state,
        &Value::Null,
        &Value::Null,
        "look-bp06",
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
        panic!("printed private decision")
    };
    question
}

fn trigger(game: &mut Game, number: &str) -> DecisionView {
    if number == "BP06-001" {
        assert!(
            matches!(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "the real Limaga enters before its fanfare"
        );
    } else {
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})
            )
            .unwrap()
            .outcome,
            "resolved",
            "the real base evolves for its printed two PP"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            8_u8,
            "real evolution pays exactly two PP"
        );
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("the real triggered ability is pending")
    };
    let handle = options
        .iter()
        .find(|option| option.card == number)
        .unwrap()
        .handle;
    awaiting(
        game.answer_internal_card(pending.actor, pending.token, Answer::Pending(handle))
            .unwrap(),
    )
}

fn complete_look(game: &mut Game, question: &DecisionView) {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("a private observed Hunter choice")
    };
    assert_eq!(
        domain.cardinality.minimum,
        Nat(0),
        "the printed choice is optional"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "one observed Hunter may enter play"
    );
    assert_eq!(
        domain.members.len(),
        1_usize,
        "original cost, kind, trait and Look receipt exclude each counterexample and the eligible tail"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some("hunter-four"),
        "original four-cost Hunter qualifies even at current cost eight"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "only the observer receives the hidden candidates"
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
        panic!("the three-card remainder needs a private order")
    };
    assert_eq!(
        cards.len(),
        3_usize,
        "the selected card and unobserved tail do not enter the remainder"
    );
    resolved(
        game.answer_internal_card(
            order.actor,
            order.token,
            Answer::Order(cards.iter().rev().map(|card| card.handle).collect()),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["field"],
        json!(["source", "selected"]),
        "the eligible Hunter goes directly into play"
    );
    assert_eq!(
        view["P1"]["hand"],
        json!([]),
        "the Look does not put anything into hand"
    );
    assert_eq!(
        view["P1"]["deck"],
        json!(["tail", "other", "spell", "wrong"]),
        "unobserved tail stays above the reversed observed remainder"
    );
    let public = game.projection(View::P2).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    assert!(
        known.iter().any(|card| card["id"] == "selected"),
        "the chosen card becomes public by entering the field"
    );
    assert!(
        !known.iter().any(|card| ["wrong", "spell", "other", "tail"]
            .iter()
            .any(|id| card["id"] == *id)),
        "the unselected observed cards and tail stay private"
    );
    let keywords = view["objects"]["source"]["keywords"].as_array().unwrap();
    assert!(
        keywords.contains(&json!("疾走")) && keywords.contains(&json!("必殺")),
        "both printed keywords survive the complete real effect"
    );
}

fn pass_quick(game: &mut Game) {
    let question = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
        .unwrap();
}

#[test]
fn bp06_001_fanfare_finds_a_low_original_cost_hunter_then_storm_attacks() {
    let mut game = fixture("BP06-001");
    let question = trigger(&mut game, "BP06-001");
    complete_look(&mut game, &question);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        4_u8,
        "the real six-cost follower pays its normal cost"
    );
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "Storm permits the newly entered source to attack the opposing leader"
    );
    pass_quick(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        16_u8,
        "the real source deals its printed four combat damage immediately"
    );
}

#[test]
fn bp06_002_real_evolution_looks_for_a_hunter_and_retains_bane_in_battle() {
    let mut game = fixture("BP06-002");
    let question = trigger(&mut game, "BP06-002");
    complete_look(&mut game, &question);
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "the evolved source enters a real battle"
    );
    pass_quick(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["enemy"]),
        "Bane destroys a nine-health follower that would survive five combat damage"
    );
}
