#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed Hero Look and Rush scenario."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::DecisionView;

const CONTROLS: [(&str, &str, &str, u8, bool); 2] = [
    ("hero", "ヒーローの魔法", "スペル", 9, true),
    ("other", "別の仲間", "フォロワー", 1, false),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind, cost, hero) in CONTROLS {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":kind,
                "traits":if hero {vec!["ヒーロー"]} else {vec![]},
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
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP10-033"}]);
    state["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"selected","card":"hero"},{"id":"wrong","card":"other"},{"id":"tail","card":"hero"}
    ]);
    state["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"other","state":{"acted":true}}]);
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        &state,
        &Value::Null,
        &Value::Null,
        "top-bp10",
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

fn fanfare(game: &mut Game) -> DecisionView {
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the real two-cost Slasher enters before its fanfare"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("real Hero fanfare")
    };
    let handle = options
        .iter()
        .find(|option| option.card == "BP10-033")
        .unwrap()
        .handle;
    awaiting(
        game.answer_internal_card(pending.actor, pending.token, Answer::Pending(handle))
            .unwrap(),
    )
}

fn take_hero(game: &mut Game, question: &DecisionView) {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("private observed Hero choice")
    };
    assert_eq!(
        domain.members.len(),
        1_usize,
        "trait and Look receipt exclude the non-Hero and unseen eligible tail"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some("hero"),
        "the printed filter accepts any Hero kind and cost, including a nine-cost spell"
    );
    assert_eq!(
        domain.cardinality.minimum,
        Nat(0),
        "taking the Hero is optional"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "only one observed Hero may be chosen"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "only the observer sees the candidate question"
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
        panic!("the single observed remainder is still ordered")
    };
    assert_eq!(
        cards.len(),
        1_usize,
        "only the looked non-Hero remains, not the unseen tail"
    );
    resolved(
        game.answer_internal_card(
            order.actor,
            order.token,
            Answer::Order(cards.iter().map(|card| card.handle).collect()),
        )
        .unwrap(),
    );
}

fn rush_attack(game: &mut Game) {
    game.resume_internal_card().unwrap();
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack",
        "Rush does not allow an immediate leader attack"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "the rejected leader attack preserves the position"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "Rush allows the newly entered source to attack an acted follower"
    );
    let quick = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
        .unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
        5_u8,
        "the real source deals its printed two combat damage"
    );
}

#[test]
fn bp10_033_fanfare_moves_the_observed_hero_to_ex_then_uses_rush() {
    let mut game = fixture();
    let question = fanfare(&mut game);
    take_hero(&mut game, &question);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["ex"],
        json!(["selected"]),
        "the selected Hero reaches EX"
    );
    assert_eq!(
        view["P1"]["deck"],
        json!(["tail", "wrong"]),
        "the unobserved tail stays above the privately bottomed remainder"
    );
    assert_eq!(
        view["P1"]["pp"]["current"], 8_u8,
        "the printed two PP is paid only for the source"
    );
    let public = game.projection(View::P2).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    assert!(
        known.iter().any(|card| card["id"] == "selected"),
        "the EX arrival becomes public"
    );
    assert!(
        !known
            .iter()
            .any(|card| card["id"] == "wrong" || card["id"] == "tail"),
        "the unchosen observed card and unobserved tail remain private"
    );
    rush_attack(&mut game);
}
