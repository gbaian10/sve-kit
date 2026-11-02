#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed observed remainder scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use serde_json::Map;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;

const CONTROLS: [(&str, &str, &str, &str, u8); 9] = [
    ("fairy", "外部童話", "フォロワー", "童話", 1),
    ("hunter", "外部狩人", "フォロワー", "狩人", 1),
    ("nature", "外部自然", "フォロワー", "自然", 1),
    ("machine", "外部機械", "フォロワー", "機械", 1),
    ("academy", "外部学院", "フォロワー", "学院", 1),
    ("levion", "外部レヴィオン", "フォロワー", "レヴィオン", 1),
    ("wrong", "外部高コスト", "フォロワー", "", 6),
    ("amulet", "外部護符", "アミュレット", "", 1),
    ("producer", "外部破壊元", "フォロワー", "", 1),
];

struct Fixture {
    selected: SelectedEffects,
    bases: BTreeMap<String, WellFormedCardProgram>,
}

fn add_controls(snapshot: &mut String, input: &mut Value) {
    for (number, name, kind, trait_name, cost) in CONTROLS {
        let follower = kind == "フォロワー";
        snapshot.push_str(&json!({"number":number,"faces":[{
            "name":name,"card_class":"ニュートラル","card_type":kind,
            "traits":if trait_name.is_empty() {vec![]} else {vec![trait_name]},"cost":cost.to_string(),
            "power":if follower {"1"} else {"-"},"hp":if follower {"2"} else {"-"},"text":null,"sections":[]
        }]}).to_string());
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!([
            format!("{number}.yaml"), json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()
        ]));
    }
}

fn add_bases(snapshot: &mut String, input: &mut Value) -> BTreeMap<String, WellFormedCardProgram> {
    let mut tree = expanded::tree().clone();
    let template = tree["cards"]["synthetic-evolution"].clone();
    let mut cards = Map::new();
    let mut programs = Map::new();
    let (base, form) = ("regina-base", "CP02-SP09a");
    let record = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|record| record["number"] == form)
        .unwrap();
    // The base only produces evolution; unrelated real base abilities remain unadmitted.
    snapshot.push_str(
        &json!({"number":base,"faces":[{
            "name":record["faces"][0]["name"],"card_class":record["faces"][0]["card_class"],
            "card_type":"フォロワー","cost":"1","power":"1","hp":"1",
            "traits":[],"text":null,"sections":[]
        }]})
        .to_string(),
    );
    snapshot.push('\n');
    cards.insert(base.into(), json!({"status":"complete","abilities":[]}));
    programs.insert(base.into(), template);
    input["documents"].as_array_mut().unwrap().push(json!([
        "observed-batch-base.yaml",
        json!({"format":1_u8,"kind":"effect_set","cards":cards}).to_string()
    ]));
    tree["cards"] = Value::Object(programs);
    lowerer().lower(&tree).unwrap()
}

fn prepared() -> &'static Fixture {
    static READY: OnceLock<Fixture> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        add_controls(&mut snapshot, &mut input);
        let bases = add_bases(&mut snapshot, &mut input);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../authored/rules/engine/index.yaml");
        let mut rules: Value = serde_saphyr::from_str(&read_to_string(path).unwrap()).unwrap();
        rules["input"] = rule_fixtures::rules(&snapshot, json!([]), json!([]))["input"].clone();
        let selected = repository::select_with_rules(
            &snapshot,
            &keywords,
            &input,
            &rules.to_string(),
            &EngineIdentityInput::LegacyJp,
        )
        .unwrap();
        Fixture { selected, bases }
    })
}

fn position(number: &str, zone: &str, picked: &str, n: u8) -> Value {
    let mut state = draw_position();
    state["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    let mut deck = vec![json!({"id":"picked","card":picked})];
    deck.extend((1..n).map(|i| json!({"id":format!("rest{i}"),"card":"wrong"})));
    deck.push(json!({"id":"tail","card":picked}));
    state["players"]["P1"]["zones"]["deck"] = json!(deck);
    state
}

fn game(state: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().selected.clone(),
        state,
        &Value::Null,
        &Value::Null,
        "observed-batch",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in CONTROLS {
        if number != "producer" {
            game.load_internal_card(number, &empty).unwrap();
        }
    }
    for (number, program) in &prepared().bases {
        game.load_internal_card(number, program).unwrap();
    }
    game.load_internal_card(
        "producer",
        &producer(Action::Destroy {
            objects: subjects(),
        }),
    )
    .unwrap();
    game
}

fn accept(game: &mut Game, mut play: CardPlay) -> CardPlay {
    loop {
        let CardPlay::Awaiting(q) = &play else {
            return play;
        };
        let answer = match q.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::MealPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::CostMaterials(_)
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::Order(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::Search(_)
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::Targets(_)
            | InputDomain::TokenCapacity { .. }
            | InputDomain::Modes { .. } => return play,
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
}

fn trigger(game: &mut Game, number: &str, line: u64) -> CardPlay {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed trigger")
    };
    let o = options
        .iter()
        .find(|o| o.card == number && o.line == line)
        .unwrap();
    let play = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(o.handle))
        .unwrap();
    accept(game, play)
}

fn choose(game: &mut Game, play: CardPlay, expected: &str) {
    let CardPlay::Awaiting(q) = play else {
        panic!("private observed selection")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("private candidates")
    };
    assert!(
        game.internal_decision(View::P2).is_none(),
        "opponents cannot see private choices"
    );
    let handle = domain
        .members
        .iter()
        .find(|m| m.card.as_deref() == Some(expected))
        .unwrap()
        .handle;
    game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![u64::MAX]))
        .unwrap_err();
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![handle]))
            .unwrap(),
    );
}

fn assert_remainder(game: &Game, n: u8, destination: &str) {
    let view = game.projection(View::Referee).unwrap();
    assert!(
        view["P1"][destination]
            .as_array()
            .unwrap()
            .contains(&json!("picked")),
        "the chosen observed card reaches its destination"
    );
    for i in 1..n {
        assert!(
            view["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!(format!("rest{i}"))),
            "only unchosen observed cards go to cemetery"
        );
    }
    assert_eq!(
        view["P1"]["deck"],
        json!(["tail"]),
        "the unobserved tail stays in the deck"
    );
    assert!(
        !game.projection(View::P2).unwrap()["known_cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == "tail"),
        "the unobserved tail stays hidden"
    );
}

fn fanfare(number: &str, picked: &str, n: u8, line: u64, destination: &str) {
    let mut game = game(&position(number, "hand", picked, n));
    let entry_play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(accept(&mut game, entry_play), CardPlay::Entered(_)),
        "the real source enters first"
    );
    let play = trigger(&mut game, number, line);
    choose(&mut game, play, picked);
    assert_remainder(&game, n, destination);
}

fn attack(number: &str) {
    let mut state = position(number, "field", "fairy", 4);
    state["players"]["P1"]["zones"]["field"][0]["state"]["counters"] =
        json!({"童話カウンター":1_u8});
    let mut game = game(&state);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "the real printed source declares its attack"
    );
    let play = trigger(&mut game, number, 2);
    choose(&mut game, play, "fairy");
    assert_remainder(&game, 4, "field");
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["counters"]["童話カウンター"],
        0_u8,
        "one printed counter is paid before the observed movement"
    );
}

fn destroy_all(game: &mut Game) {
    resolved(
        game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
}

fn evolution(base: &str, form: &str, picked: &str, n: u8, pp: u8) -> Game {
    let mut state = position(base, "field", picked, n);
    state["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    let mut game = game(&state);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":pp}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the real printed source evolves"
    );
    game
}

#[test]
fn bp03_018_attack_spends_a_counter_to_put_a_low_cost_observed_follower_on_field() {
    attack("BP03-018");
}
#[test]
fn bp03_019_attack_uses_its_own_counter_and_the_same_printed_remainder() {
    attack("BP03-019");
}

#[test]
fn bp06_010_lastword_adds_a_revealed_hunter_and_mills_only_the_remainder() {
    let mut state = position("BP06-010", "field", "hunter", 2);
    state["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"producer","card":"producer"}));
    let mut game = game(&state);
    destroy_all(&mut game);
    let play = trigger(&mut game, "BP06-010", 2);
    choose(&mut game, play, "hunter");
    assert_remainder(&game, 2, "hand");
}

#[test]
fn bp07_052_fanfare_adds_an_observed_nature_card_and_mills_the_other_two() {
    fanfare("BP07-052", "nature", 3, 3, "hand");
}
#[test]
fn bp17_083_ward_fanfare_adds_an_observed_machine_and_mills_the_other_card() {
    fanfare("BP17-083", "machine", 2, 2, "hand");
}

#[test]
fn bp18_054_real_evolution_adds_an_observed_academy_card_and_mills_the_remainder() {
    let mut game = evolution("BP18-053", "BP18-054", "academy", 2, 1);
    let play = trigger(&mut game, "BP18-054", 1);
    choose(&mut game, play, "academy");
    assert_remainder(&game, 2, "hand");
}
#[test]
fn bp21_034_fanfare_adds_an_observed_levion_card_and_mills_the_other_card() {
    fanfare("BP21-034", "levion", 2, 1, "hand");
}
#[test]
fn cp02_036_fanfare_places_one_of_three_observed_cards_into_ex_and_mills_the_other_two() {
    fanfare("CP02-036", "fairy", 3, 2, "ex");
}

#[test]
fn cp02_070_real_evolution_puts_one_observed_card_in_ex_and_mills_the_other_three() {
    let mut game = evolution("CP02-069", "CP02-070", "fairy", 4, 1);
    let play = trigger(&mut game, "CP02-070", 1);
    choose(&mut game, play, "fairy");
    assert_remainder(&game, 4, "ex");
}
#[test]
fn cp02_sp09a_real_form_keeps_its_distinct_identity_and_mills_the_remainder() {
    let mut game = evolution("regina-base", "CP02-SP09a", "fairy", 4, 1);
    let play = trigger(&mut game, "CP02-SP09a", 1);
    choose(&mut game, play, "fairy");
    assert_remainder(&game, 4, "ex");
}
#[test]
fn pr_439_fanfare_puts_an_observed_passion_follower_on_field_and_mills_the_remainder() {
    fanfare("PR-439", "CSD02c-001", 2, 2, "field");
}
#[test]
fn sd06_001_fanfare_puts_an_observed_amulet_on_field_then_its_own_simultaneous_departure_still_triggers()
 {
    let mut state = position("SD06-001", "hand", "amulet", 4);
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"producer","card":"producer"}]);
    let mut game = game(&state);
    let entry_play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(entry_play, CardPlay::Entered(_)),
        "the printed follower enters"
    );
    let play = trigger(&mut game, "SD06-001", 2);
    choose(&mut game, play, "amulet");
    assert_remainder(&game, 4, "field");
    game.resume_internal_card().unwrap();
    destroy_all(&mut game);
    let departure_play = trigger(&mut game, "SD06-001", 1);
    resolved(departure_play);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        18_u8
    );
}

#[test]
fn cp02_036_full_ex_keeps_a_failed_selection_in_the_milled_remainder() {
    let mut state = position("CP02-036", "hand", "fairy", 3);
    state["players"]["P1"]["zones"]["ex"] = json!(
        (0_u8..5)
            .map(|i| json!({"id":format!("ex{i}"),"card":"amulet"}))
            .collect::<Vec<_>>()
    );
    let mut game = game(&state);
    let entry_play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(entry_play, CardPlay::Entered(_)),
        "the printed follower enters"
    );
    let play = trigger(&mut game, "CP02-036", 2);
    choose(&mut game, play, "fairy");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["picked", "rest1", "rest2"])
    );
}

#[test]
fn bp08_001_returns_the_declared_opposing_follower_to_the_top_then_buffs_only_other_allies_at_end()
{
    let mut state = position("BP08-001", "hand", "nature", 1);
    state["players"]["P1"]["leader"]["life"] = json!(10_u8);
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"nature"}]);
    state["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"hunter"}]);
    state["players"]["P2"]["zones"]["deck"] = json!([{"id":"enemy-tail","card":"wrong"}]);
    let mut game = game(&state);
    let entry_play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(accept(&mut game, entry_play), CardPlay::Entered(_)),
        "the actual Ward source enters"
    );
    let CardPlay::Awaiting(q) = trigger(&mut game, "BP08-001", 3) else {
        panic!("opposing follower target")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("formal public target")
    };
    assert_eq!(domain.members.len(), 1);
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P2"]["deck"], json!(["enemy", "enemy-tail"]));
    assert_eq!(view["objects"]["enemy"]["generation"], 1_u8);
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    let play = trigger(&mut game, "BP08-001", 2);
    resolved(play);
    let end_view = game.projection(View::Referee).unwrap();
    assert_eq!(end_view["objects"]["ally"]["power"], 3_u8);
    assert_eq!(end_view["objects"]["ally"]["hp"], 4_u8);
    assert_eq!(end_view["objects"]["source"]["power"], 4_u8);
    assert_eq!(end_view["P1"]["leader"]["life"], 14_u8);
}
