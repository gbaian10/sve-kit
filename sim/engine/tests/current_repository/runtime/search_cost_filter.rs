#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate a printed search scenario."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::{SearchAnswer, SearchDomain};

type ControlFace = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
);
const CONTROLS: [ControlFace; 8] = [
    ("search-one-spell", "スペル", "1", "-", "-", None),
    ("search-two-spell", "スペル", "2", "-", "-", None),
    ("search-one-follower", "フォロワー", "1", "1", "3", None),
    (
        "search-three-machine",
        "フォロワー",
        "3",
        "2",
        "3",
        Some("機械"),
    ),
    (
        "search-four-machine",
        "フォロワー",
        "4",
        "2",
        "3",
        Some("機械"),
    ),
    ("search-three-plain", "フォロワー", "3", "2", "3", None),
    ("search-unknown", "フォロワー", "3", "2", "3", None),
    ("producer", "フォロワー", "0", "1", "1", None),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, kind, cost, power, hp, trait_id) in CONTROLS {
            snapshot.push_str(
                &json!({"number":number,"faces":[{
                    "name":number,"card_class":"ニュートラル","card_type":kind,
                    "traits":trait_id.into_iter().collect::<Vec<_>>(),"cost":cost,
                    "power":power,"hp":hp,"text":null,"sections":[]
                }]})
                .to_string(),
            );
            snapshot.push('\n');
            let mut document = json!({"format":1_u8,"kind":"effect_set","cards":{}});
            document["cards"][number] = json!({"status":"complete","abilities":[]});
            input["documents"]
                .as_array_mut()
                .unwrap()
                .push(json!([format!("{number}.yaml"), document.to_string()]));
        }
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../authored/rules/engine/index.yaml");
        let mut rules: Value = serde_saphyr::from_str(&read_to_string(path).unwrap()).unwrap();
        rules["input"] = rule_fixtures::rules(&snapshot, json!([]), json!([]))["input"].clone();
        Catalog::from_current_selection_with_rules(
            &snapshot,
            &keywords,
            &input,
            &rules.to_string(),
            &EngineIdentityInput::LegacyJp,
        )
        .unwrap()
    })
}
fn fixture(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "search-cost",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in CONTROLS {
        if !["producer", "search-unknown"].contains(&number) {
            game.load_internal_card(number, &empty).unwrap();
        }
    }
    game
}
fn settle(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = &play {
        let answer = if matches!(question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else {
            break;
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}
fn trigger(game: &mut Game, number: &str, line: u64) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending")
    };
    let handle = options
        .iter()
        .find(|option| option.card == number && option.line == line)
        .unwrap()
        .handle;
    let play = game
        .answer_internal_card(question.actor, question.token, Answer::Pending(handle))
        .unwrap();
    settle(game, play)
}
fn enter(game: &mut Game, number: &str, line: u64) -> CardPlay {
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(settle(game, play), CardPlay::Entered(_)),
        "the real source enters"
    );
    trigger(game, number, line)
}
fn found(game: &mut Game, play: CardPlay, expected: &[&str]) {
    let CardPlay::Awaiting(question) = play else {
        panic!("search choice")
    };
    let InputDomain::Search(SearchDomain {
        cards, cardinality, ..
    }) = &question.domain
    else {
        panic!("search domain")
    };
    let mut actual = cards
        .iter()
        .map(|card| card.card.as_str())
        .collect::<Vec<_>>();
    actual.sort_unstable();
    assert_eq!(
        actual, expected,
        "the private domain contains exactly the qualifying printings"
    );
    assert_eq!(
        cardinality.minimum,
        Nat(0),
        "conditional private search retains its find-failure exception"
    );
    let answer = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![cards[0].handle])),
        )
        .unwrap();
    resolved(settle(game, answer));
}
fn position(number: &str, deck: Value) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] = deck;
    initial
}
#[test]
fn bp04_046_searches_effective_cost_one_spells_and_preserves_other_private_cards() {
    let initial = position(
        "BP04-046",
        json!([
            {"id":"printed-one","card":"search-one-spell","state":{"cost":2_u8}},
            {"id":"effective-one","card":"search-two-spell","state":{"cost":1_u8}},
            {"id":"follower","card":"search-one-follower"}
        ]),
    );
    let mut game = fixture(&initial);
    let play = enter(&mut game, "BP04-046", 1);
    found(&mut game, play, &["search-two-spell"]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["effective-one"]));
    assert_eq!(state["P1"]["deck_count"], 2_u8);
    let opponent = game.projection(View::P2).unwrap();
    assert!(
        !opponent["objects"]
            .as_object()
            .unwrap()
            .contains_key("printed-one")
    );
}
#[test]
fn bp07_072_searches_original_cost_machine_then_its_real_lastword_heals() {
    let mut initial = position(
        "BP07-072",
        json!([
            {"id":"good","card":"search-three-machine","state":{"cost":9_u8}},
            {"id":"too-much","card":"search-four-machine","state":{"cost":1_u8}},
            {"id":"wrong-trait","card":"search-three-plain"}
        ]),
    );
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"producer","card":"producer"}]);
    let mut game = fixture(&initial);
    let mut objects = subjects();
    if let Selector::Zone { players, .. } = &mut objects {
        players.relation = PlayerRelation::SelfPlayer;
        players.cardinality = Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        };
    }
    game.load_internal_card("producer", &producer(Action::Destroy { objects }))
        .unwrap();
    let play = enter(&mut game, "BP07-072", 1);
    found(&mut game, play, &["search-three-machine"]);
    game.resume_internal_card().unwrap();
    let entered = game.projection(View::Referee).unwrap();
    assert_eq!(
        entered["P1"]["field"],
        json!(["producer", "source", "good"])
    );
    assert_eq!(entered["P1"]["deck_count"], 2_u8);
    let destruction = game
        .activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
        .unwrap();
    resolved(destruction);
    game.resume_internal_card().unwrap();
    let departure = trigger(&mut game, "BP07-072", 2);
    resolved(departure);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        12_u8
    );
}
#[test]
fn bp20_108_pays_for_power_and_storm_search_then_draws_on_its_own_attack() {
    let initial = position(
        "BP20-108",
        json!([
            {"id":"good","card":"BP01-048"},
            {"id":"too-strong","card":"BP01-158"},
            {"id":"without-storm","card":"search-one-follower"}
        ]),
    );
    let mut game = fixture(&initial);
    let played = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(matches!(settle(&mut game, played), CardPlay::Entered(_)));
    let before = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
        .as_u64()
        .unwrap();
    let search = trigger(&mut game, "BP20-108", 3);
    found(&mut game, search, &["BP01-048"]);
    game.resume_internal_card().unwrap();
    let entered = game.projection(View::Referee).unwrap();
    assert_eq!(entered["P1"]["pp"]["current"], before - 2);
    assert_eq!(entered["P1"]["field"], json!(["source", "good"]));
    game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
        .unwrap();
    let attack = trigger(&mut game, "BP20-108", 2);
    resolved(attack);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand_count"],
        1_u8
    );
}
