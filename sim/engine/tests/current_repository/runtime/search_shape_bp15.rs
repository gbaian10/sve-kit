#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed searches."
)]
use super::*;
use alloc::collections::BTreeMap;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::SearchAnswer;

const SOURCES: [(&str, &str, &str); 3] = [
    ("BP15-015", "不殺の隠者", "エルフ"),
    ("BP15-048", "絶尽の崇拝者", "ウィッチ"),
    ("BP15-059", "ブリザードハート・フィルレイン", "ドラゴン"),
];
const CONTROLS: [(&str, &str, bool); 8] = [
    ("ezdia-follower", "若きエズディア", false),
    ("ezdia-spell", "エズディアの魔法", true),
    ("raio-follower", "若きライオ", false),
    ("raio-spell", "ライオの魔法", true),
    ("silver-follower", "銀氷の仲間", false),
    ("silver-spell", "銀氷の魔法", true),
    ("plain", "無関係な仲間", false),
    ("enemy", "大きな相手", false),
];

fn prepared() -> &'static (SelectedEffects, BTreeMap<String, WellFormedCardProgram>) {
    static READY: OnceLock<(SelectedEffects, BTreeMap<String, WellFormedCardProgram>)> =
        OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let mut programs = BTreeMap::new();
        for (number, name, class, spell, base) in CONTROLS
            .iter()
            .map(|(number, name, spell)| (number.to_string(), *name, "ニュートラル", *spell, false))
            .chain(SOURCES.iter().map(|(number, name, class)| (format!("{number}-base"), *name, *class, false, true)))
        {
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":class,"card_type":if spell {"スペル"} else {"フォロワー"},
                "traits":[],"cost":"0","power":if spell {"-"} else {"1"},
                "hp":if spell {"-"} else {"9"},"text":null,"sections":[]
            }]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([
                format!("{number}.yaml"),
                json!({"format":1_u8,"kind":"effect_set","cards":{number.clone():{"status":"complete","abilities":[]}}}).to_string()
            ]));
            let program = if base {
                let mut tree = expanded::tree().clone();
                tree["cards"] = json!({number.clone():tree["cards"]["synthetic-evolution"]});
                lowerer().lower(&tree).unwrap().remove(&number).unwrap()
            } else {
                WellFormedCardProgram::check(CardProgram { abilities: vec![], card_play_costs: vec![] }).unwrap()
            };
            programs.insert(number, program);
        }
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../authored/rules/engine/index.yaml");
        let mut rules: Value = serde_saphyr::from_str(&read_to_string(path).unwrap()).unwrap();
        rules["input"] = rule_fixtures::rules(&snapshot, json!([]), json!([]))["input"].clone();
        let selected = Catalog::from_current_selection_with_rules(
            &snapshot, &keywords, &input, &rules.to_string(), &EngineIdentityInput::LegacyJp,
        ).unwrap();
        (selected, programs)
    })
}

fn position(number: &str, deck: &[&str]) -> Value {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([]);
    state["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":format!("{number}-base")}]);
    state["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
    state["players"]["P1"]["zones"]["deck"] = json!(
        deck.iter()
            .map(|card| json!({"id":card,"card":card}))
            .collect::<Vec<_>>()
    );
    state
}

fn fixture(state: &Value) -> Game {
    let (selected, programs) = prepared();
    let mut game = Game::from_selected_effects(
        selected.clone(),
        state,
        &Value::Null,
        &Value::Null,
        "shape-bp15",
    )
    .unwrap();
    for (number, program) in programs {
        game.load_internal_card(number, program).unwrap();
    }
    game
}

fn evolve(game: &mut Game, number: &str, line: u64) -> CardPlay {
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
    )
    .unwrap();
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending evolution ability")
    };
    let handle = options
        .iter()
        .find(|o| o.card == number && o.line == line)
        .unwrap()
        .handle;
    game.answer_internal_card(question.actor, question.token, Answer::Pending(handle))
        .unwrap()
}

fn search(game: &mut Game, play: CardPlay, expected: &[&str], choose: &str) {
    let CardPlay::Awaiting(q) = play else {
        panic!("printed search")
    };
    let InputDomain::Search(domain) = &q.domain else {
        panic!("private search domain")
    };
    let mut actual = domain
        .cards
        .iter()
        .map(|c| c.card.as_str())
        .collect::<Vec<_>>();
    actual.sort_unstable();
    assert_eq!(
        actual, expected,
        "the printed name fragment and kind filter define the domain"
    );
    let handle = domain
        .cards
        .iter()
        .find(|c| c.card == choose)
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![handle])),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!([choose]),
        "the searched card is added to the previously empty hand"
    );
}

fn mode(game: &mut Game, play: CardPlay, index: u64) -> CardPlay {
    let CardPlay::Awaiting(q) = play else {
        panic!("printed modes")
    };
    assert!(
        matches!(q.domain, InputDomain::Modes { .. }),
        "the evolution ability offers its printed modes"
    );
    game.answer_internal_card(q.actor, q.token, Answer::Modes(vec![ModeId(index)]))
        .unwrap()
}

fn pass_quick(game: &mut Game) {
    let question = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
        .unwrap();
}

#[test]
fn bp15_015_evolution_searches_a_name_fragment_but_only_on_followers() {
    let mut game = fixture(&position(
        "BP15-015",
        &["ezdia-follower", "ezdia-spell", "plain"],
    ));
    let play = evolve(&mut game, "BP15-015", 1);
    search(&mut game, play, &["ezdia-follower"], "ezdia-follower");
}

#[test]
fn bp15_048_evolution_offers_targeted_damage_or_a_filtered_search() {
    let mut state = position("BP15-048", &["raio-follower", "raio-spell", "plain"]);
    let mut game = fixture(&state);
    let play = evolve(&mut game, "BP15-048", 1);
    let play = mode(&mut game, play, 2);
    search(&mut game, play, &["raio-follower"], "raio-follower");

    state["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"enemy"}]);
    let mut damage_game = fixture(&state);
    let damage_play = evolve(&mut damage_game, "BP15-048", 1);
    let CardPlay::Awaiting(q) = mode(&mut damage_game, damage_play, 1) else {
        panic!("formal damage target")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("target domain")
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card.as_deref(), Some("enemy"));
    resolved(
        damage_game
            .answer_internal_card(
                q.actor,
                q.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
    );
    let view = damage_game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["enemy"]["hp"], 7_u8);
    assert_eq!(view["P1"]["hand"], json!([]));
}

#[test]
fn bp15_059_evolution_searches_any_silver_ice_card_and_keeps_bane() {
    let mut state = position("BP15-059", &["silver-follower", "silver-spell", "plain"]);
    state["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"enemy","state":{"acted":true}}]);
    let mut game = fixture(&state);
    let play = evolve(&mut game, "BP15-059", 2);
    search(
        &mut game,
        play,
        &["silver-follower", "silver-spell"],
        "silver-spell",
    );
    assert!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("必殺")),
        "the evolved real card retains its printed Bane"
    );
    game.resume_internal_card().unwrap();
    game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
        .unwrap();
    pass_quick(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["enemy"])
    );
}
