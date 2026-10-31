#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed Beast Look and top banish scenarios."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::DecisionView;

const CONTROLS: [(&str, &str, &str, bool); 3] = [
    ("beast", "獣の魔法", "スペル", true),
    ("other", "別の仲間", "フォロワー", false),
    ("amulet", "別の護符", "アミュレット", false),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind, beast) in CONTROLS {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":kind,
                "traits":if beast {vec!["獣"]} else {vec![]},"cost":"9",
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

fn fixture(state: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        state,
        &Value::Null,
        &Value::Null,
        "top-bp18",
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

fn trigger(game: &mut Game, number: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("real pending printed trigger")
    };
    let handle = options
        .iter()
        .find(|option| option.card == number)
        .unwrap()
        .handle;
    game.answer_internal_card(question.actor, question.token, Answer::Pending(handle))
        .unwrap()
}

fn take_beast(game: &mut Game, question: &DecisionView) {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("observed Beast choice")
    };
    assert_eq!(
        domain.members.len(),
        1_usize,
        "trait and Look receipt exclude non-Beasts and the unobserved eligible tail"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some("beast"),
        "the printed filter admits a nine-cost Beast spell without a kind or cost restriction"
    );
    assert_eq!(
        domain.cardinality.minimum,
        Nat(0),
        "putting the observed card into EX is optional"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "at most one observed Beast card may move"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "only the observer sees the private question"
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
        panic!("two-card private remainder")
    };
    assert_eq!(
        cards.len(),
        2_usize,
        "only the two unselected observed cards are ordered"
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
fn bp18_012_real_base_evolves_then_moves_an_observed_beast_card_to_ex() {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP18-011"}]);
    state["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP18-012"}]);
    state["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"selected","card":"beast"},{"id":"wrong","card":"other"},
        {"id":"amulet","card":"amulet"},{"id":"tail","card":"beast"}
    ]);
    let mut game = fixture(&state);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the real admitted BP18-011 base evolves for its printed one PP"
    );
    let question = awaiting(trigger(&mut game, "BP18-012"));
    take_beast(&mut game, &question);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["pp"]["current"], 9_u8,
        "real evolution pays exactly one PP"
    );
    assert_eq!(
        view["P1"]["ex"],
        json!(["selected"]),
        "the real evolved ability moves the observed Beast spell to EX"
    );
    assert_eq!(
        view["P1"]["deck"],
        json!(["tail", "amulet", "wrong"]),
        "the unseen tail stays above the privately reversed observed remainder"
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
            .any(|card| card["id"] == "wrong" || card["id"] == "amulet" || card["id"] == "tail"),
        "the unchosen observed cards and unobserved tail remain private"
    );
}

fn banish_state(number: &str, has_top: bool) -> Value {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    state["players"]["P1"]["zones"]["deck"] = if has_top {
        json!([
            {"id":"top","card":"beast"},{"id":"tail","card":"other"}
        ])
    } else {
        json!([])
    };
    state
}

fn banish_scenario(number: &str, has_top: bool) {
    let mut game = fixture(&banish_state(number, has_top));
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the real Bouncer enters for its printed two PP"
    );
    resolved(trigger(&mut game, number));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["banish"],
        if has_top { json!(["top"]) } else { json!([]) },
        "the real fanfare banishes exactly the first available card, without a kind or cost filter"
    );
    assert_eq!(
        view["P1"]["deck"],
        if has_top { json!(["tail"]) } else { json!([]) },
        "only the literal top one is consumed and an empty deck is unchanged"
    );
    assert_eq!(
        view["P1"]["cemetery"],
        json!([]),
        "banishing is distinct from cemetery movement"
    );
    assert_eq!(
        view["P1"]["field"],
        json!(["source"]),
        "the source stays on the field after its fanfare"
    );
    assert_eq!(
        view["P1"]["pp"]["current"], 8_u8,
        "the real source pays exactly two PP"
    );
    assert!(
        game.internal_decision(View::P1).is_none(),
        "direct banish does not create a Look choice"
    );
    let public = game.projection(View::P2).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    assert_eq!(
        known.iter().any(|card| card["id"] == "top"),
        has_top,
        "the banished top card becomes public only when present"
    );
    assert!(
        !known.iter().any(|card| card["id"] == "tail"),
        "the unbanished tail stays private"
    );
}

#[test]
fn bp18_051_and_p23_banish_the_actual_top_one_without_drawing() {
    for number in ["BP18-051", "BP18-P23"] {
        for has_top in [true, false] {
            banish_scenario(number, has_top);
        }
    }
}
