#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed top-to-EX scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use serde_json::Map;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;

const CONTROLS: [(&str, &str, &str, bool); 3] = [
    ("ocean", "海洋の魔法", "スペル", true),
    ("other", "別の仲間", "フォロワー", false),
    ("amulet", "別の護符", "アミュレット", false),
];

struct Fixture {
    selected: SelectedEffects,
    bases: BTreeMap<String, WellFormedCardProgram>,
}

fn add_controls(snapshot: &mut String, input: &mut Value) {
    for (number, name, kind, banquet) in CONTROLS {
        let follower = kind == "フォロワー";
        snapshot.push_str(
            &json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":kind,
                "traits":if banquet {vec!["海洋"]} else {vec!["兵士"]},"cost":"1",
                "power":if follower {"1"} else {"-"},"hp":if follower {"9"} else {"-"},
                "text":null,"sections":[]
            }]})
            .to_string(),
        );
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!([
            format!("{number}.yaml"),
            json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()
        ]));
    }
}

fn add_bases(snapshot: &mut String, input: &mut Value) -> BTreeMap<String, WellFormedCardProgram> {
    let mut tree = expanded::tree().clone();
    let template = tree["cards"]["synthetic-evolution"].clone();
    let mut cards = Map::new();
    let mut programs = Map::new();
    for (base, form) in [
        ("top-bishop-base", "BP06-109"),
        ("top-soldier-base", "BP16-020"),
    ] {
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
        programs.insert(base.into(), template.clone());
    }
    input["documents"].as_array_mut().unwrap().push(json!([
        "top-bp14-bases.yaml",
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

fn position(number: &str, zone: &str) -> Value {
    let mut state = draw_position();
    state["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    state["players"]["P1"]["leader"]["life"] = json!(10_u8);
    state["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    state["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"top","card":"other"},{"id":"tail","card":"amulet"}
    ]);
    state["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"other"}]);
    state
}

fn game(state: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().selected.clone(),
        state,
        &Value::Null,
        &Value::Null,
        "top-ex-batch",
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
    for (number, program) in &prepared().bases {
        game.load_internal_card(number, program).unwrap();
    }
    game
}

fn finish(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
            InputDomain::CostMaterials(domain) => {
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            InputDomain::ResourceEntryPayment { .. }
            | InputDomain::Quick { .. }
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
            | InputDomain::TokenCapacity { .. }
            | InputDomain::Modes { .. } => panic!("unexpected printed choice"),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    play
}

fn enter(game: &mut Game) {
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(finish(game, play), CardPlay::Entered(_)),
        "the printed permanent enters"
    );
}

fn trigger_line(game: &mut Game, number: &str, line: u64) -> CardPlay {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed pending trigger")
    };
    let option = options
        .iter()
        .find(|o| o.card == number && o.line == line)
        .unwrap();
    game.answer_internal_card(q.actor, q.token, Answer::Pending(option.handle))
        .unwrap()
}

fn fanfare(game: &mut Game, number: &str, line: u64) {
    enter(game);
    let play = trigger_line(game, number, line);
    resolved(finish(game, play));
}

fn assert_top(game: &Game) {
    let view = game.projection(View::Referee).unwrap();
    assert!(
        view["P1"]["ex"].as_array().unwrap().contains(&json!("top")),
        "the literal top card enters EX"
    );
    assert_eq!(
        view["P1"]["deck"],
        json!(["tail"]),
        "the unobserved tail stays in the deck"
    );
    let public = game.projection(View::P2).unwrap();
    assert!(
        !public["known_cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == "tail"),
        "the untouched tail stays private"
    );
}

fn evolve(base: &str, form: &str, super_evolution: bool) -> Game {
    let mut state = position(base, "field");
    state["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"discard","card":"amulet"}]);
    state["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"ally","card":"other"}));
    state["players"]["P1"]["sep"] = json!(u8::from(super_evolution));
    let mut game = game(&state);
    let outcome = game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8,"sep":u8::from(super_evolution)}})).unwrap();
    assert_eq!(
        outcome.outcome, "resolved",
        "an actual evolution produces the real form's trigger"
    );
    game
}

#[test]
fn bp04_130_activates_only_into_an_empty_ex() {
    for occupied in [false, true] {
        let mut state = position("BP04-130", "field");
        if occupied {
            state["players"]["P1"]["zones"]["ex"] = json!([{"id":"old","card":"amulet"}]);
        }
        let mut game = game(&state);
        let id = prepared().selected.current["BP04-130"]
            .program
            .card()
            .abilities[0]
            .node
            .id;
        resolved(
            game.activate_internal_card("source", id, PlayerId { seat: 1 })
                .unwrap(),
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["ex"],
            if occupied {
                json!(["old"])
            } else {
                json!(["top"])
            }
        );
        assert_eq!(view["objects"]["source"]["acted"], true);
        if !occupied {
            assert_top(&game);
        }
    }
}

#[test]
fn bp06_109_evolution_selects_top_to_ex_instead_of_its_other_modes() {
    let mut game = evolve("top-bishop-base", "BP06-109", false);
    let CardPlay::Awaiting(q) = trigger_line(&mut game, "BP06-109", 1) else {
        panic!("printed modes")
    };
    let InputDomain::Modes { modes, .. } = &q.domain else {
        panic!("three printed modes")
    };
    assert_eq!(modes.len(), 3);
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Modes(vec![modes[2]]))
            .unwrap(),
    );
    assert_top(&game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P2"]["leader"]["life"], 20_u8);
    assert_eq!(view["objects"]["enemy"]["acted"], false);
}

#[test]
fn bp10_067_moves_the_top_and_damages_only_opposing_followers() {
    let mut game = game(&position("BP10-067", "hand"));
    resolved(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
    );
    assert_top(&game);
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        4_u8
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["field"],
        json!(["enemy"])
    );
}

#[test]
fn bp10_117_counts_itself_towards_four_followers() {
    for others in [2_u8, 3] {
        let mut state = position("BP10-117", "hand");
        state["players"]["P1"]["zones"]["field"] = json!(
            (0..others)
                .map(|i| json!({"id":format!("ally{i}"),"card":"other"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&state);
        fanfare(&mut game, "BP10-117", 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["ex"],
            if others == 3 {
                json!(["top"])
            } else {
                json!([])
            }
        );
    }
}

#[test]
fn bp14_072_fanfare_moves_the_top_then_discards_a_hand_card() {
    let mut state = position("BP14-072", "hand");
    state["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"discard","card":"amulet"}));
    let mut game = game(&state);
    fanfare(&mut game, "BP14-072", 2);
    assert_top(&game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["discard"])
    );
}

#[test]
fn bp14_073_real_evolution_damages_the_target_and_its_leader_heals_and_moves_the_top() {
    let mut state = position("BP14-072", "field");
    state["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP14-073"}]);
    let mut game = game(&state);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    let play = trigger_line(&mut game, "BP14-073", 1);
    resolved(finish(&mut game, play));
    assert_top(&game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["enemy"]["damage"], 5_u8);
    assert_eq!(view["P2"]["leader"]["life"], 18_u8);
    assert_eq!(view["P1"]["leader"]["life"], 12_u8);
}

#[test]
fn bp14_079_real_evolution_moves_the_top_then_discards() {
    let mut game = evolve("BP14-078", "BP14-079", false);
    let play = trigger_line(&mut game, "BP14-079", 1);
    resolved(finish(&mut game, play));
    assert_top(&game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["discard"])
    );
}

#[test]
fn bp14_112_fanfare_moves_the_top() {
    let mut game = game(&position("BP14-112", "hand"));
    fanfare(&mut game, "BP14-112", 2);
    assert_top(&game);
}

#[test]
fn bp16_020_super_evolution_moves_the_top_and_buffs_the_selected_soldier() {
    let mut game = evolve("top-soldier-base", "BP16-020", true);
    let evolve_play = trigger_line(&mut game, "BP16-020", 2);
    resolved(evolve_play);
    game.resume_internal_card().unwrap();
    let play = trigger_line(&mut game, "BP16-020", 3);
    let CardPlay::Awaiting(q) = play else {
        panic!("soldier target")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("own soldier candidates")
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
    assert_top(&game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["ally"]["power"],
        5_u8
    );
}

#[test]
fn bp19_024_ward_fanfare_moves_the_top() {
    let mut game = game(&position("BP19-024", "hand"));
    fanfare(&mut game, "BP19-024", 3);
    assert_top(&game);
    assert!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("守護"))
    );
}

#[test]
fn bp19_067_creates_two_orcas_with_rush_and_assail_then_moves_the_top() {
    let mut game = game(&position("BP19-067", "hand"));
    fanfare(&mut game, "BP19-067", 2);
    assert_top(&game);
    let view = game.projection(View::Referee).unwrap();
    let field = view["P1"]["field"].as_array().unwrap();
    assert_eq!(field.len(), 3);
    for id in field.iter().filter(|id| **id != json!("source")) {
        let keywords = view["objects"][id.as_str().unwrap()]["keywords"]
            .as_array()
            .unwrap();
        assert!(keywords.contains(&json!("突進")));
        assert!(keywords.contains(&json!("指定攻撃")));
    }
}

#[test]
fn bp19_070_pays_an_ocean_hand_card_before_moving_the_top_and_checking_ex_count() {
    let mut state = position("BP19-070", "hand");
    state["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"fee","card":"ocean"}));
    state["players"]["P1"]["zones"]["ex"] = json!([{"id":"old","card":"amulet"}]);
    let mut game = game(&state);
    fanfare(&mut game, "BP19-070", 1);
    assert_top(&game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["cemetery"], json!(["fee"]));
    assert_eq!(view["P1"]["leader"]["life"], 12_u8);
}

#[test]
fn bp19_071_destroys_the_declared_opposing_follower_then_moves_the_top() {
    let mut game = game(&position("BP19-071", "hand"));
    fanfare(&mut game, "BP19-071", 1);
    assert_top(&game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["enemy"])
    );
}
