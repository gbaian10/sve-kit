#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the printed Machine Look scenario."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::DecisionView;

const CONTROLS: [(&str, &str, &str, u8, bool); 3] = [
    ("machine", "機械の魔法", "スペル", 9, true),
    ("other", "別の仲間", "フォロワー", 1, false),
    ("amulet", "別の護符", "アミュレット", 1, false),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind, cost, machine) in CONTROLS {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":kind,
                "traits":if machine {vec!["機械"]} else {vec![]},
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

fn fixture() -> Game {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP07-042"}]);
    state["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"selected","card":"machine"},{"id":"wrong","card":"other"},
        {"id":"amulet","card":"amulet"},{"id":"extra","card":"other"},
        {"id":"tail","card":"machine"}
    ]);
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        &state,
        &Value::Null,
        &Value::Null,
        "top-bp07",
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

fn enter_and_trigger(game: &mut Game) -> DecisionView {
    let ward = awaiting(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
    );
    assert!(
        matches!(ward.domain, InputDomain::WardEntry { .. }),
        "the real printed Ward offers its entry choice"
    );
    assert!(
        matches!(
            game.answer_internal_card(ward.actor, ward.token, Answer::WardEntry(true))
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "Ward enters the source acted before the fanfare"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("real Machine fanfare")
    };
    let handle = options
        .iter()
        .find(|option| option.card == "BP07-042")
        .unwrap()
        .handle;
    awaiting(
        game.answer_internal_card(pending.actor, pending.token, Answer::Pending(handle))
            .unwrap(),
    )
}

fn choose_machine(game: &mut Game, question: &DecisionView) -> DecisionView {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("private Machine choice")
    };
    assert_eq!(
        domain.members.len(),
        1_usize,
        "trait and observed receipt exclude unrelated cards and the unobserved eligible tail"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some("machine"),
        "a nine-cost Machine spell qualifies without an unprinted kind or cost limit"
    );
    assert_eq!(
        domain.cardinality.minimum,
        Nat(0),
        "the printed selection may be declined"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "at most one observed Machine is chosen"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "only the observing player receives the hidden candidates"
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

#[test]
fn bp07_042_ward_fanfare_moves_the_observed_machine_to_ex_and_orders_the_rest() {
    let mut game = fixture();
    let question = enter_and_trigger(&mut game);
    let remainder = choose_machine(&mut game, &question);
    let InputDomain::Order(cards) = &remainder.domain else {
        panic!("private remainder ordering")
    };
    assert_eq!(
        cards.len(),
        3_usize,
        "only the three unchosen observed cards are ordered"
    );
    resolved(
        game.answer_internal_card(
            remainder.actor,
            remainder.token,
            Answer::Order(cards.iter().rev().map(|card| card.handle).collect()),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["ex"],
        json!(["selected"]),
        "the selected Machine reaches EX"
    );
    assert_eq!(
        view["P1"]["deck"],
        json!(["tail", "extra", "amulet", "wrong"]),
        "the untouched tail stays above the reverse-ordered observed remainder"
    );
    assert_eq!(
        view["objects"]["source"]["acted"], true,
        "the real Ward choice remains in force"
    );
    assert_eq!(
        view["P1"]["pp"]["current"], 7_u8,
        "only the printed three PP play cost is paid"
    );
    let public = game.projection(View::P2).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    assert!(
        known.iter().any(|card| card["id"] == "selected"),
        "the EX arrival is public"
    );
    assert!(
        !known.iter().any(|card| ["wrong", "amulet", "extra", "tail"]
            .iter()
            .any(|id| card["id"] == *id)),
        "the unchosen observed cards and unobserved tail remain private"
    );
}
