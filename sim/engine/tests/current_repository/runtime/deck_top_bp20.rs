#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed Ocean payment and Beast Look scenarios."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::DecisionView;

const CONTROLS: [(&str, &str, &str, &str); 4] = [
    ("ocean", "海洋の魔法", "スペル", "海洋"),
    ("beast", "獣の仲間", "フォロワー", "獣"),
    ("beast-spell", "獣の魔法", "スペル", "獣"),
    ("other", "別の仲間", "フォロワー", "兵士"),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind, family) in CONTROLS {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":kind,
                "traits":[family],"cost":"9","power":if follower {"1"} else {"-"},
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

fn fixture(state: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        state,
        &Value::Null,
        &Value::Null,
        "top-bp20",
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
        panic!("printed decision")
    };
    question
}

fn trigger(game: &mut Game, number: &str) -> DecisionView {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("real pending printed trigger")
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

fn mermaid_position(number: &str) -> Value {
    let mut state = draw_position();
    state["players"]["P1"]["leader"]["life"] = json!(10_u8);
    let mut hand = vec![
        json!({"id":"payment","card":"ocean"}),
        json!({"id":"wrong-hand","card":"beast-spell"}),
    ];
    if matches!(number, "BP20-060" | "BP20-P30") {
        hand.insert(0, json!({"id":"source","card":number}));
    } else {
        state["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP20-060"}]);
        state["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
    }
    state["players"]["P1"]["zones"]["hand"] = Value::Array(hand);
    state["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"selected","card":"ocean"},{"id":"wrong","card":"other"},
        {"id":"tail","card":"ocean"}
    ]);
    state
}

fn mermaid_trigger(game: &mut Game, number: &str) -> DecisionView {
    if matches!(number, "BP20-060" | "BP20-P30") {
        assert!(
            matches!(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "the real base enters for its printed two PP"
        );
    } else {
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved",
            "the real complete BP20-060 base pays one PP to evolve"
        );
    }
    trigger(game, number)
}

fn pay_ocean(game: &mut Game, optional: &DecisionView) -> DecisionView {
    let payment = awaiting(
        game.answer_internal_card(optional.actor, optional.token, Answer::OptionalCost(true))
            .unwrap(),
    );
    let InputDomain::CostMaterials(materials) = &payment.domain else {
        panic!("Ocean discard cost")
    };
    assert_eq!(
        materials.members.len(),
        1_usize,
        "only the Ocean card in hand may pay, not the non-Ocean spell or unseen deck cards"
    );
    let result = game
        .answer_internal_card(
            payment.actor,
            payment.token,
            Answer::CostMaterials(vec![materials.members[0].handle]),
        )
        .unwrap();
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["cemetery"],
        json!(["payment"]),
        "the Ocean spell is discarded before any Look benefit"
    );
    assert_eq!(
        view["P1"]["hand"],
        json!(["wrong-hand"]),
        "the non-Ocean card stays in hand"
    );
    assert_eq!(
        view["P1"]["leader"]["life"], 10_u8,
        "healing has not preceded the paid observation"
    );
    awaiting(result)
}

fn choose_card(game: &mut Game, question: &DecisionView, number: &str, remainder: usize) {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("private filtered Look choice")
    };
    assert_eq!(
        domain.members.len(),
        1_usize,
        "only the observed eligible card is selectable, excluding filters and the unseen eligible tail"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some(number),
        "the intended nine-cost card satisfies the printed filter"
    );
    assert_eq!(
        domain.cardinality.minimum,
        Nat(0),
        "the printed choice is optional"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "only one observed card may move to EX"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "only the observer sees private candidates"
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
        panic!("private observed remainder")
    };
    assert_eq!(
        cards.len(),
        remainder,
        "only unselected observed cards are ordered"
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

fn assert_privacy(game: &Game, selected: bool) {
    let public = game.projection(View::P2).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    assert_eq!(
        known.iter().any(|card| card["id"] == "selected"),
        selected,
        "only the selected EX arrival is public"
    );
    assert!(
        !known.iter().any(|card| card["id"] == "wrong"
            || card["id"] == "tail"
            || card["id"] == "wrong-kind"
            || card["id"] == "wrong-trait"),
        "the unchosen observed cards and unobserved tail remain private"
    );
}

fn mermaid_scenario(number: &str, pay: bool) {
    let mut game = fixture(&mermaid_position(number));
    let optional = mermaid_trigger(&mut game, number);
    if pay {
        let look = pay_ocean(&mut game, &optional);
        choose_card(&mut game, &look, "ocean", 1_usize);
    } else {
        assert_eq!(
            game.answer_internal_card(optional.actor, optional.token, Answer::OptionalCost(false))
                .unwrap(),
            CardPlay::Cancelled,
            "declining the printed trigger cost cancels that occurrence instead of resolving its body"
        );
    }
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["leader"]["life"],
        if pay { 11_u8 } else { 10_u8 },
        "only paying the printed discard permits the final one healing"
    );
    assert_eq!(
        view["P1"]["ex"],
        if pay { json!(["selected"]) } else { json!([]) },
        "only the paid ability moves an observed Ocean card to EX"
    );
    assert_eq!(
        view["P1"]["deck"],
        if pay {
            json!(["tail", "wrong"])
        } else {
            json!(["selected", "wrong", "tail"])
        },
        "declining the cost preserves the unseen deck; paying bottoms only the observed remainder"
    );
    assert_eq!(
        view["P1"]["cemetery"],
        if pay { json!(["payment"]) } else { json!([]) },
        "declining the cost discards nothing"
    );
    assert_eq!(
        view["P1"]["hand"],
        if pay {
            json!(["wrong-hand"])
        } else {
            json!(["payment", "wrong-hand"])
        },
        "unpaid material remains in hand"
    );
    assert_eq!(
        view["P1"]["pp"]["current"],
        if matches!(number, "BP20-060" | "BP20-P30") {
            8_u8
        } else {
            9_u8
        },
        "only the real card play or evolution PP is paid"
    );
    assert_privacy(&game, pay);
}

#[test]
fn bp20_060_and_p30_pay_ocean_discard_before_the_real_fanfare_or_decline() {
    for number in ["BP20-060", "BP20-P30"] {
        for pay in [true, false] {
            mermaid_scenario(number, pay);
        }
    }
}

#[test]
fn bp20_061_and_p31_evolve_the_real_base_then_pay_ocean_discard_or_decline() {
    for number in ["BP20-061", "BP20-P31"] {
        for pay in [true, false] {
            mermaid_scenario(number, pay);
        }
    }
}

#[test]
fn bp20_109_fanfare_selects_only_an_observed_beast_follower_for_ex() {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP20-109"}]);
    state["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"selected","card":"beast"},{"id":"wrong-kind","card":"beast-spell"},
        {"id":"wrong-trait","card":"ocean"},{"id":"tail","card":"beast"}
    ]);
    let mut game = fixture(&state);
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the real two-cost Al-mi'raj enters before its fanfare"
    );
    let look = trigger(&mut game, "BP20-109");
    choose_card(&mut game, &look, "beast", 2_usize);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["ex"],
        json!(["selected"]),
        "the eligible Beast follower reaches EX"
    );
    assert_eq!(
        view["P1"]["deck"],
        json!(["tail", "wrong-trait", "wrong-kind"]),
        "the unseen tail stays above the privately reversed remainder"
    );
    assert_eq!(
        view["P1"]["pp"]["current"], 8_u8,
        "the real two PP play cost is paid"
    );
    assert_privacy(&game, true);
}
