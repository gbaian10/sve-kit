#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the printed Look scenarios."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::DecisionView;

const CONTROLS: [(&str, &str, &str, u8, bool); 9] = [
    ("f-one", "一費の仲間", "フォロワー", 1, false),
    ("f-two", "二費の仲間", "フォロワー", 2, false),
    ("s-one", "一費の魔法", "スペル", 1, false),
    ("s-two", "二費の魔法", "スペル", 2, false),
    ("a-two", "二費の護符", "アミュレット", 2, false),
    (
        "levion-same",
        "レヴィオンヴァンガード・ジェノ",
        "フォロワー",
        1,
        true,
    ),
    ("levion-good", "別のレヴィオン", "フォロワー", 3, true),
    ("levion-spell", "レヴィオンの魔法", "スペル", 1, true),
    ("other-f", "別の仲間", "フォロワー", 3, false),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind, cost, levion) in CONTROLS {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":kind,
                "traits":if levion {vec!["レヴィオン"]} else {vec![]},
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

fn fixture(number: &str, deck: &[(&str, &str, u8)]) -> Game {
    let mut state = draw_position();
    if number == "BP01-039" {
        state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    } else {
        let base = if number == "BP02-026" {
            "BP02-025"
        } else {
            "BP03-014"
        };
        state["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
        state["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
    }
    state["players"]["P1"]["zones"]["deck"] = json!(
        deck.iter()
            .map(|(id, card, cost)| json!({"id":id,"card":card,"state":{"cost":cost}}))
            .collect::<Vec<_>>()
    );
    state["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"f-one"}]);
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        &state,
        &Value::Null,
        &Value::Null,
        "look-cost",
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
    let CardPlay::Awaiting(q) = play else {
        panic!("printed private choice")
    };
    q
}

fn start(game: &mut Game, number: &str) -> DecisionView {
    if number == "BP01-039" {
        assert!(
            matches!(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "the real source enters the field"
        );
    } else {
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved",
            "the real current base evolves into the real card under test"
        );
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("printed trigger")
    };
    let handle = options.iter().find(|o| o.card == number).unwrap().handle;
    awaiting(
        game.answer_internal_card(pending.actor, pending.token, Answer::Pending(handle))
            .unwrap(),
    )
}

fn take(game: &mut Game, q: &DecisionView, expected_card: &str) -> DecisionView {
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("private observed choice")
    };
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
    assert_eq!(
        domain.members.len(),
        1_usize,
        "filters and the Look receipt leave only one eligible observed object, excluding an eligible unobserved tail"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some(expected_card),
        "the printed filter uses effective costs, kinds and source names as applicable"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "only the observing controller receives this question"
    );
    awaiting(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    )
}

fn order_rest(game: &mut Game, order: &DecisionView, deck: &[&str]) {
    let InputDomain::Order(cards) = &order.domain else {
        panic!("private remainder order")
    };
    assert_eq!(
        cards.len(),
        deck.len().checked_sub(1).unwrap(),
        "only the looked remainder is ordered, leaving the untouched tail at the top"
    );
    resolved(
        game.answer_internal_card(
            order.actor,
            order.token,
            Answer::Order(cards.iter().rev().map(|c| c.handle).collect()),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["deck"],
        json!(deck),
        "the observed remainder moves to the bottom in the chosen order without disturbing the tail"
    );
}

fn revealed_only(game: &Game) {
    let view = game.projection(View::P2).unwrap();
    let known = view["known_cards"].as_array().unwrap();
    assert!(
        known.iter().any(|card| card["id"] == "selected"),
        "the selected hand card is publicly revealed"
    );
    assert!(
        !known
            .iter()
            .any(|card| ["same", "wrong", "spell", "amulet", "other", "tail"]
                .iter()
                .any(|id| card["id"] == *id)),
        "unchosen observed cards and the untouched tail stay private"
    );
}

#[test]
fn bp01_039_uses_effective_cost_to_field_and_preserves_private_receipt_authority() {
    let mut game = fixture(
        "BP01-039",
        &[
            ("wrong", "f-one", 2),
            ("selected", "f-two", 1),
            ("spell", "s-one", 1),
            ("amulet", "a-two", 2),
            ("other", "other-f", 3),
            ("tail", "f-two", 1),
        ],
    );
    let q = start(&mut game, "BP01-039");
    let before = game.projection(View::Referee).unwrap();
    assert!(
        game.answer_internal_card(PlayerId { seat: 2 }, q.token, Answer::Targets(vec![0]))
            .is_err(),
        "the opponent cannot answer a private Look"
    );
    assert!(
        game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![999]))
            .is_err(),
        "a foreign handle cannot select an unobserved deck card"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "rejected choices preserve the position"
    );
    let order = take(&mut game, &q, "f-two");
    let waiting = game.projection(View::Referee).unwrap();
    assert!(
        game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![0]))
            .is_err(),
        "the consumed target token cannot answer the remainder order"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        waiting,
        "a stale answer preserves the order question and position"
    );
    order_rest(
        &mut game,
        &order,
        &["tail", "other", "amulet", "spell", "wrong"],
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"],
        json!(["source", "selected"]),
        "the effective one-cost follower enters the field directly"
    );
}

#[test]
fn bp02_026_excludes_source_names_then_reveals_the_other_levion_follower() {
    let mut game = fixture(
        "BP02-026",
        &[
            ("same", "levion-same", 1),
            ("selected", "levion-good", 3),
            ("spell", "levion-spell", 1),
            ("other", "other-f", 1),
            ("tail", "levion-good", 3),
        ],
    );
    let q = start(&mut game, "BP02-026");
    let order = take(&mut game, &q, "levion-good");
    order_rest(&mut game, &order, &["tail", "other", "spell", "same"]);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["selected"]),
        "the chosen differently named Levion follower reaches hand"
    );
    revealed_only(&game);
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "the real evolved card retains Assail and can attack a standing enemy"
    );
}

#[test]
fn bp03_015_looks_for_effective_cost_one_without_restricting_card_kind() {
    let mut game = fixture(
        "BP03-015",
        &[
            ("selected", "s-two", 1),
            ("wrong", "f-one", 2),
            ("amulet", "a-two", 2),
            ("other", "other-f", 3),
            ("tail", "s-two", 1),
        ],
    );
    let q = start(&mut game, "BP03-015");
    let order = take(&mut game, &q, "s-two");
    order_rest(&mut game, &order, &["tail", "other", "amulet", "wrong"]);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["selected"]),
        "a spell with effective cost one is eligible even though its printed cost is two"
    );
    revealed_only(&game);
}
