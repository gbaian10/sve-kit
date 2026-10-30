#![expect(
    clippy::panic,
    reason = "Unexpected decisions indicate a broken card fixture."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::SearchAnswer;

const CONTROLS: [(&str, &str, &str, bool, u8); 9] = [
    ("hero-a", "仲間A", "フォロワー", true, 1),
    ("hero-copy", "仲間A", "フォロワー", true, 2),
    ("hero-b", "仲間B", "フォロワー", true, 3),
    ("hero-high", "仲間C", "フォロワー", true, 4),
    ("other", "別の仲間", "フォロワー", false, 1),
    ("hero-amulet", "別の護符", "アミュレット", true, 2),
    ("payment-spell", "支払いの魔法", "スペル", true, 1),
    ("payment-amulet", "支払いの護符", "アミュレット", true, 1),
    ("spare", "手札の仲間", "フォロワー", false, 0),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind, hero, cost) in CONTROLS {
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":kind,
                "traits":if hero {vec!["ヒーロー"]} else {vec![]},"cost":cost.to_string(),
                "power":if kind == "フォロワー" {"1"} else {"-"},"hp":if kind == "フォロワー" {"3"} else {"-"},"text":null,"sections":[]
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
    state["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP10-027"},{"id":"payment-spell","card":"payment-spell"},{"id":"payment-amulet","card":"payment-amulet"},{"id":"spare","card":"spare"}
    ]);
    state["players"]["P1"]["zones"]["deck"] = json!(
        CONTROLS
            .iter()
            .filter(|(number, ..)| !["payment-spell", "payment-amulet", "spare"].contains(number))
            .map(|(number, ..)| {
                let mut card = json!({"id":number,"card":number});
                if *number == "hero-high" {
                    card["state"] = json!({"cost":1_u8});
                }
                if *number == "hero-b" {
                    card["state"] = json!({"cost":7_u8});
                }
                card
            })
            .collect::<Vec<_>>()
    );
    state["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP10-028"}]);
    state["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"other","state":{"acted":true}}]);
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        &state,
        &Value::Null,
        &Value::Null,
        "bp10-search",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in CONTROLS {
        game.load_internal_card(number, &empty).unwrap();
    }
    game
}

fn pay_for_search(game: &mut Game) -> CardPlay {
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the real source enters the field"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("fanfare")
    };
    let trigger = options
        .iter()
        .find(|option| option.card == "BP10-027" && option.line == 3)
        .unwrap()
        .handle;
    let CardPlay::Awaiting(optional) = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(trigger))
        .unwrap()
    else {
        panic!("optional payment")
    };
    let CardPlay::Awaiting(payment) = game
        .answer_internal_card(optional.actor, optional.token, Answer::OptionalCost(true))
        .unwrap()
    else {
        panic!("Hero discard")
    };
    let InputDomain::CostMaterials(materials) = &payment.domain else {
        panic!("discard materials")
    };
    assert_eq!(
        materials.members.len(),
        2_usize,
        "only Hero cards in hand can pay, including spell and amulet but not the unrelated follower"
    );
    let play = game
        .answer_internal_card(
            payment.actor,
            payment.token,
            Answer::CostMaterials(
                materials
                    .members
                    .iter()
                    .map(|member| member.handle)
                    .collect(),
            ),
        )
        .unwrap();
    let view = game.projection(View::Referee).unwrap();
    let cemetery = view["P1"]["cemetery"].as_array().unwrap();
    assert_eq!(
        cemetery.len(),
        2_usize,
        "both materials are discarded before the search"
    );
    assert!(
        cemetery.contains(&json!("payment-spell")) && cemetery.contains(&json!("payment-amulet")),
        "the two printed Hero materials reach cemetery"
    );
    assert_eq!(
        view["P1"]["hand"],
        json!(["spare"]),
        "the non-Hero hand card is retained"
    );
    play
}

fn find_heroes(game: &mut Game, play: CardPlay) {
    let CardPlay::Awaiting(q) = play else {
        panic!("printed search")
    };
    let InputDomain::Search(domain) = &q.domain else {
        panic!("Hero follower search")
    };
    let mut cards = domain
        .cards
        .iter()
        .map(|card| card.card.as_str())
        .collect::<Vec<_>>();
    cards.sort_unstable();
    assert_eq!(
        cards,
        vec!["hero-a", "hero-b", "hero-copy"],
        "only Hero followers of original cost at most three match, regardless of changed effective costs"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(2)),
        "up to two differently named cards may be found"
    );
    let handle = |number: &str| {
        domain
            .cards
            .iter()
            .find(|card| card.card == number)
            .unwrap()
            .handle
    };
    let first = handle("hero-a");
    let duplicate = handle("hero-copy");
    let second = handle("hero-b");
    let before = game.projection(View::Referee).unwrap();
    let Err(error) = game.answer_internal_card(
        q.actor,
        q.token,
        Answer::Search(SearchAnswer::Grouped(vec![vec![first, duplicate]])),
    ) else {
        panic!("duplicate names are forbidden")
    };
    assert!(
        error.to_string().contains("search names must differ"),
        "the negative answer rejects names rather than shape"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "rejected names preserve the position"
    );
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Search(SearchAnswer::Grouped(vec![vec![first, second]])),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    let field = view["P1"]["field"].as_array().unwrap();
    assert_eq!(
        field.len(),
        3_usize,
        "the source and both searched followers occupy the field"
    );
    assert!(
        field.contains(&json!("hero-a")) && field.contains(&json!("hero-b")),
        "both differently named found followers enter the field"
    );
    assert_eq!(
        view["P1"]["deck_count"], 4_u8,
        "only the two found cards leave the deck"
    );
    game.resume_internal_card().unwrap();
}

#[test]
fn bp10_027_discards_two_heroes_searches_distinct_names_and_keeps_rush_and_evolution() {
    let mut game = fixture();
    let play = pay_for_search(&mut game);
    find_heroes(&mut game, play);
    let mut evolving = game.clone();
    assert_eq!(
        evolving
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})
            )
            .unwrap()
            .outcome,
        "resolved",
        "the real base can pay its printed two PP to evolve into its current repository partner"
    );
    assert_eq!(
        evolving.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        2_u8,
        "evolution pays exactly two PP after the six PP card play"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack",
        "Rush does not allow an immediate leader attack"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "the real source retains Rush and can immediately attack an acted enemy"
    );
}
