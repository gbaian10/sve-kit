#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate these printed Look and choice scenarios."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::DecisionView;

const CONTROLS: [(&str, &str, &str, u8); 5] = [
    ("one", "一費の仲間", "フォロワー", 1),
    ("three", "三費の仲間", "フォロワー", 3),
    ("four", "四費の仲間", "フォロワー", 4),
    ("spell", "一費の魔法", "スペル", 1),
    ("amulet", "一費の護符", "アミュレット", 1),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind, cost) in CONTROLS {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":kind,
                "traits":[],
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

fn fixture(number: &str, deck: &[(&str, &str, u8)], legal_enemy: bool) -> Game {
    let mut state = draw_position();
    if number == "BP05-066" {
        state["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP05-065"}]);
        state["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
    } else {
        state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    }
    state["players"]["P1"]["zones"]["deck"] = json!(
        deck.iter()
            .map(|(id, card, cost)| json!({"id":id,"card":card,"state":{"cost":cost}}))
            .collect::<Vec<_>>()
    );
    state["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy-good","card":"one","state":{"cost":8_u8}},
        {"id":"enemy-wrong","card":"four","state":{"cost":1_u8}}
    ]);
    if !legal_enemy {
        state["players"]["P2"]["zones"]["field"] =
            json!([{ "id":"enemy-wrong", "card":"four", "state":{"cost":1_u8}}]);
    }
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        &state,
        &Value::Null,
        &Value::Null,
        "look-bp05",
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
        panic!("the printed card asks a question")
    };
    question
}

fn trigger(game: &mut Game, number: &str) -> DecisionView {
    if number == "BP05-066" {
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved",
            "the real base evolves at its printed cost"
        );
    } else {
        assert!(
            matches!(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "the real Fina enters play"
        );
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("the real authored trigger is pending")
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

fn select_observed(game: &mut Game, question: &DecisionView, card: &str) -> DecisionView {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("private observed card choice")
    };
    assert_eq!(
        domain.cardinality.minimum,
        Nat(0),
        "taking a card is optional"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "one observed card may be taken"
    );
    assert_eq!(
        domain.members.len(),
        1_usize,
        "original cost and follower kind exclude all other observed cards and the unobserved tail"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some(card),
        "eligibility uses original rather than changed cost"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "only the observer sees the private question"
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

fn bottom(game: &mut Game, question: &DecisionView) {
    let InputDomain::Order(cards) = &question.domain else {
        panic!("the observed remainder is privately ordered")
    };
    assert_eq!(
        cards.len(),
        4_usize,
        "only the unchosen four observed cards are ordered"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Order(cards.iter().rev().map(|card| card.handle).collect()),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["deck"],
        json!(["tail", "extra", "amulet", "spell", "wrong"]),
        "unobserved tail stays above the reversed observed remainder"
    );
}

fn modes(game: &mut Game) -> DecisionView {
    let question = trigger(game, "BP05-109");
    let InputDomain::Modes { modes: choices, .. } = &question.domain else {
        panic!("Fina offers its printed modes")
    };
    assert_eq!(
        choices.clone(),
        vec![ModeId(1), ModeId(2)],
        "both modes are legal while an original one-cost opposing follower exists"
    );
    question
}

#[test]
fn bp05_066_evolves_then_puts_an_original_three_cost_follower_into_play() {
    let mut game = fixture(
        "BP05-066",
        &[
            ("selected", "three", 8),
            ("wrong", "four", 1),
            ("spell", "spell", 1),
            ("amulet", "amulet", 1),
            ("extra", "four", 4),
            ("tail", "three", 3),
        ],
        true,
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["selected"]["cost"],
        8_u8,
        "the observed original three-cost follower has changed current cost eight before selection"
    );
    let question = trigger(&mut game, "BP05-066");
    let order = select_observed(&mut game, &question, "three");
    bottom(&mut game, &order);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["field"],
        json!(["source", "selected"]),
        "the observed follower enters the field directly after the real evolution"
    );
    assert_eq!(
        view["P1"]["hand"],
        json!([]),
        "the printed destination is not hand"
    );
}

#[test]
fn bp05_109_chooses_private_original_cost_look_or_a_formal_destruction_target() {
    let mut game = fixture(
        "BP05-109",
        &[
            ("selected", "one", 8),
            ("wrong", "four", 1),
            ("spell", "spell", 1),
            ("amulet", "amulet", 1),
            ("extra", "three", 1),
            ("tail", "one", 1),
        ],
        true,
    );
    let choice = modes(&mut game);
    let question = awaiting(
        game.answer_internal_card(choice.actor, choice.token, Answer::Modes(vec![ModeId(1)]))
            .unwrap(),
    );
    let order = select_observed(&mut game, &question, "one");
    bottom(&mut game, &order);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["hand"],
        json!(["selected"]),
        "the chosen original one-cost follower is added to hand"
    );
    assert_eq!(
        view["P2"]["field"],
        json!(["enemy-good", "enemy-wrong"]),
        "the Look mode does not destroy a follower"
    );
    let public = game.projection(View::P2).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    assert!(
        known.iter().any(|card| card["id"] == "selected"),
        "the selected hand card is publicly revealed"
    );
    assert!(
        !known
            .iter()
            .any(|card| ["wrong", "spell", "amulet", "extra", "tail"]
                .iter()
                .any(|id| card["id"] == *id)),
        "unselected observed cards and the tail remain private"
    );
    check_destroy_mode();
    check_unavailable_destroy_mode();
}

fn check_destroy_mode() {
    let mut game = fixture("BP05-109", &[("tail", "one", 1)], true);
    let choice = modes(&mut game);
    let question = awaiting(
        game.answer_internal_card(choice.actor, choice.token, Answer::Modes(vec![ModeId(2)]))
            .unwrap(),
    );
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("the destroy mode declares a formal target")
    };
    assert_eq!(
        domain.members.len(),
        1_usize,
        "only the opposing original one-cost follower is a legal target"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some("one"),
        "changed current costs do not change the formal original-cost restriction"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P2"]["field"],
        json!(["enemy-wrong"]),
        "the legal opposing follower is destroyed"
    );
    assert_eq!(
        view["P2"]["cemetery"],
        json!(["enemy-good"]),
        "the destroyed target reaches cemetery"
    );
    assert_eq!(
        view["P1"]["deck"],
        json!(["tail"]),
        "the destroy mode never observes or moves deck cards"
    );
}

fn check_unavailable_destroy_mode() {
    let mut game = fixture("BP05-109", &[("tail", "one", 1)], false);
    let question = trigger(&mut game, "BP05-109");
    let InputDomain::Modes { modes: choices, .. } = &question.domain else {
        panic!("choice legality is checked before selection")
    };
    assert_eq!(
        choices.clone(),
        vec![ModeId(1)],
        "Q879 forbids choosing destroy without a legal formal target"
    );
}
