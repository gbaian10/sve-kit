#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the printed Deresmas Look scenario."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::DecisionView;

const CONTROLS: [(&str, &str, &str, u8, bool); 4] = [
    ("two", "二費のデレマス", "フォロワー", 2, true),
    ("three", "三費のデレマス", "フォロワー", 3, true),
    ("other", "別の仲間", "フォロワー", 1, false),
    ("spell", "デレマスの魔法", "スペル", 2, true),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind, cost, deresmas) in CONTROLS {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":kind,
                "traits":if deresmas {vec!["デレマス"]} else {vec![]},
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

fn fixture(candidate: &str) -> Game {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP02-093"}]);
    state["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"selected","card":candidate,"state":{"cost":8_u8}},
        {"id":"wrong","card":"three","state":{"cost":1_u8}},
        {"id":"other","card":"other"},
        {"id":"tail","card":"two"}
    ]);
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        &state,
        &Value::Null,
        &Value::Null,
        "look-cp02",
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
        panic!("printed private question")
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
        "the real Risa enters play"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("the real fanfare is pending")
    };
    let handle = options
        .iter()
        .find(|option| option.card == "CP02-093")
        .unwrap()
        .handle;
    awaiting(
        game.answer_internal_card(pending.actor, pending.token, Answer::Pending(handle))
            .unwrap(),
    )
}

fn choose(game: &mut Game, question: &DecisionView, eligible: bool) -> DecisionView {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("the private observed choice")
    };
    assert_eq!(
        domain.cardinality.minimum,
        Nat(0),
        "taking a card is optional"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "only one observed card may enter play"
    );
    assert_eq!(
        domain.members.len(),
        usize::from(eligible),
        "original cost, trait, kind and receipt jointly define the domain"
    );
    if eligible {
        assert_eq!(
            domain.members[0].card.as_deref(),
            Some("two"),
            "an original two-cost Deresmas follower qualifies at effective cost eight"
        );
    }
    assert!(
        game.internal_decision(View::P2).is_none(),
        "the opponent cannot see this private question"
    );
    awaiting(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(domain.members.iter().map(|member| member.handle).collect()),
        )
        .unwrap(),
    )
}

fn order(game: &mut Game, question: &DecisionView, remaining: usize) {
    let InputDomain::Order(cards) = &question.domain else {
        panic!("only the looked remainder is ordered")
    };
    assert_eq!(
        cards.len(),
        remaining,
        "the moved card and unobserved tail are excluded from the private remainder"
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

fn reject_spell() {
    let mut game = fixture("spell");
    let question = fanfare(&mut game);
    let remainder = choose(&mut game, &question, false);
    order(&mut game, &remainder, 3);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["field"],
        json!(["source"]),
        "a Deresmas spell never enters the field through the follower-only effect"
    );
    assert_eq!(
        view["P1"]["deck"],
        json!(["tail", "other", "wrong", "selected"]),
        "with no eligible follower all three observed cards are privately reordered at the bottom"
    );
    let public = game.projection(View::P2).unwrap();
    assert!(
        !public["known_cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|card| ["selected", "wrong", "other", "tail"]
                .iter()
                .any(|id| card["id"] == *id)),
        "unselected observations and the untouched tail remain private"
    );
}

#[test]
fn cp02_093_puts_a_low_original_cost_deresmas_follower_into_play() {
    let mut game = fixture("two");
    let question = fanfare(&mut game);
    let remainder = choose(&mut game, &question, true);
    order(&mut game, &remainder, 2);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["field"],
        json!(["source", "selected"]),
        "the real fanfare puts the qualifying follower directly into play"
    );
    assert_eq!(
        view["P1"]["hand"],
        json!([]),
        "the selected card is not added to hand"
    );
    assert_eq!(
        view["P1"]["deck"],
        json!(["tail", "other", "wrong"]),
        "the untouched tail stays above the privately reversed remainder"
    );
    assert_eq!(
        view["P1"]["pp"]["current"], 7_u8,
        "only the real source's printed three PP is paid, not the selected card's changed cost"
    );
    let public = game.projection(View::P2).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    assert!(
        known.iter().any(|card| card["id"] == "selected"),
        "the card put into play becomes public"
    );
    assert!(
        !known.iter().any(|card| ["wrong", "other", "tail"]
            .iter()
            .any(|id| card["id"] == *id)),
        "unselected observed cards and the tail stay private"
    );
    reject_spell();
}
