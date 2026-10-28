#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate real-card scenarios."
)]

use super::ex_select::{expanded, rule_fixtures};
use super::*;
use serde_json::Map;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    initial
}

fn trigger(game: &mut Game, card: &str) -> CardPlay {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed trigger")
    };
    assert_eq!(options.len(), 1_usize, "one printed ability is pending");
    assert_eq!(
        options[0].card, card,
        "the actual printing supplies the ability"
    );
    game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap()
}

fn target(game: &mut Game, play: CardPlay) {
    let CardPlay::Awaiting(q) = play else {
        panic!("formal target")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("formal target domain")
    };
    assert_eq!(
        domain.members.len(),
        1_usize,
        "only the enemy follower is eligible"
    );
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
}

fn play_follower(game: &mut Game) {
    assert!(
        matches!(
            game.play_internal_card("source", actor()).unwrap(),
            CardPlay::Entered(_)
        ),
        "the real follower enters for its printed cost"
    );
}

fn pass(game: &mut Game) {
    let q = game.internal_quick_choice().unwrap().unwrap();
    assert!(
        matches!(
            game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
                .unwrap(),
            CardPlay::Continued(_)
        ),
        "Quick Pass completes the stored attack"
    );
}

fn tokens(state: &Value, card: &str) -> Vec<String> {
    state["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| state["objects"][id.as_str().unwrap()]["card"] == card)
        .map(|id| id.as_str().unwrap().to_owned())
        .collect()
}

// Real Repair Modes commit actual card plays without consuming field capacity.
fn add_priors(initial: &mut Value, count: usize) {
    let mut ex = initial["players"]["P1"]["zones"]["ex"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for n in 0..count {
        ex.push(json!({"id":format!("prior{n}"),"card":"BP07-T02"}));
    }
    initial["players"]["P1"]["zones"]["ex"] = json!(ex);
}

fn play_priors(game: &mut Game, count: usize) {
    for n in 0..count {
        resolved(
            game.play_internal_card(&format!("prior{n}"), actor())
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
    }
}

fn destructive_game(card: &str) -> Game {
    let mut initial = position(card, "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    with_producer(
        &initial,
        Action::Destroy {
            objects: subjects(),
        },
    )
}

#[test]
fn bp06_087_each_same_line_trigger_creates_a_real_bat() {
    for card in ["BP06-087", "BP06-P24"] {
        let mut game = destructive_game(card);
        play_follower(&mut game);
        let entry = resolved(trigger(&mut game, card));
        game.resume_internal_card().unwrap();
        let before = game.projection(View::Referee).unwrap();
        let first = tokens(&before, "BP01-T15");
        assert_eq!(first.len(), 1_usize);
        assert_eq!(before["objects"][&first[0]]["power"], 1_u8);
        assert_eq!(before["objects"][&first[0]]["hp"], 1_u8);
        activate(&mut game, "P1");
        game.resume_internal_card().unwrap();
        let exit = resolved(trigger(&mut game, card));
        assert_ne!(entry.source.definition, exit.source.definition);
        assert_eq!(entry.source.holder, exit.source.holder);
        game.resume_internal_card().unwrap();
        let after = game.projection(View::Referee).unwrap();
        let second = tokens(&after, "BP01-T15");
        assert_eq!(second.len(), 1_usize);
        assert_ne!(
            first, second,
            "Lastword creates a new bat after the first was destroyed"
        );
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}

fn dual_life(card: &str) {
    let mut game = destructive_game(card);
    play_follower(&mut game);
    let entry = resolved(trigger(&mut game, card));
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        21_u8,
        "Fanfare restores exactly one life"
    );
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    let exit = resolved(trigger(&mut game, card));
    assert_ne!(
        entry.source.definition, exit.source.definition,
        "same-line scopes remain distinct"
    );
    assert_eq!(
        entry.source.holder, exit.source.holder,
        "both scopes belong to the same follower"
    );
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["leader"]["life"], 22_u8,
        "Lastword restores one more life"
    );
    assert_eq!(
        state["P2"]["leader"]["life"], 20_u8,
        "the opponent is unaffected"
    );
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "each event triggers only once"
    );
}

#[test]
fn bp08_117_fanfare_and_lastword_each_restore_one_life() {
    dual_life("BP08-117");
}

#[test]
fn bp15_126_and_its_alias_each_restore_life_on_entry_and_exit() {
    for card in ["BP15-126", "BP15-P28"] {
        dual_life(card);
    }
}

#[test]
fn bp02_001_combo_includes_tias_own_play_and_creates_crystal_eve() {
    for (card, prior) in [
        ("BP02-001", 1_usize),
        ("BP02-001", 2),
        ("BP02-SL01", 2),
        ("PR-045", 2),
    ] {
        let mut initial = position(card, "hand");
        add_priors(&mut initial, prior);
        let mut game = game(&initial);
        play_priors(&mut game, prior);
        play_follower(&mut game);
        resolved(trigger(&mut game, card));
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        let made = tokens(&state, "BP02-T01");
        assert_eq!(
            made.len(),
            usize::from(prior == 2),
            "Combo 3 includes this follower's committed play"
        );
        for id in made {
            assert_eq!(state["objects"][&id]["power"], 4_u8);
            assert_eq!(state["objects"][&id]["hp"], 4_u8);
            assert_eq!(state["objects"][&id]["name"], "クリスタリア・イヴ");
        }
    }
}

#[test]
fn bp02_t01_and_bp18_t11_are_real_four_power_follower_tokens() {
    for card in ["BP02-T01", "BP18-T11"] {
        let mut game = game(&position(card, "field"));
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        pass(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["card"], card);
        assert_eq!(state["objects"]["source"]["hp"], 4_u8);
        assert_eq!(state["P2"]["leader"]["life"], 16_u8);
    }
}

#[test]
fn bp11_003_aliases_use_combo_five_fanfare_and_combo_three_storm_attack() {
    for card in [
        "BP11-003",
        "BP11-SL03",
        "BP11-U01",
        "BP21-PR02",
        "BP21-PR14",
    ] {
        for prior in [1_usize, 2, 4] {
            let mut initial = position(card, "hand");
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
            add_priors(&mut initial, prior);
            let mut game = game(&initial);
            play_priors(&mut game, prior);
            play_follower(&mut game);
            let choice = trigger(&mut game, card);
            target(&mut game, choice);
            game.resume_internal_card().unwrap();
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if prior == 4 { 3_u8 } else { 0_u8 }
            );
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"P2.leader"})
                )
                .unwrap()
                .outcome,
                "resolved",
                "Storm permits an attack in the entry turn"
            );
            resolved(trigger(&mut game, card));
            game.resume_internal_card().unwrap();
            let boosted = prior >= 2;
            let attacked = game.projection(View::Referee).unwrap();
            assert_eq!(
                attacked["objects"]["source"]["power"],
                if boosted { 2_u8 } else { 1_u8 }
            );
            assert_eq!(
                attacked["objects"]["source"]["hp"],
                if boosted { 2_u8 } else { 1_u8 }
            );
            pass(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
                if boosted { 18_u8 } else { 19_u8 }
            );
        }
    }
}

fn arrow_position(card: &str, prior: usize) -> Value {
    let mut initial = position(card, "ex");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    add_priors(&mut initial, prior);
    initial
}

fn token_gone(state: &Value) {
    assert_eq!(
        state["P1"]["ex"],
        json!([]),
        "all played spell tokens leave EX"
    );
    assert_eq!(
        state["P1"]["cemetery"],
        json!([]),
        "spell tokens cease instead of remaining in the cemetery"
    );
}

#[test]
fn bp17_p29_and_t02_select_before_checking_combo_three() {
    for card in ["BP17-P29", "BP17-T02"] {
        for prior in [1_usize, 2] {
            let mut game = game(&arrow_position(card, prior));
            play_priors(&mut game, prior);
            let play = game.play_internal_card("source", actor()).unwrap();
            target(&mut game, play);
            game.resume_internal_card().unwrap();
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if prior == 2 { 2_u8 } else { 0_u8 }
            );
            assert_eq!(
                state["P1"]["pp"]["current"],
                if prior == 2 { 7_u8 } else { 8_u8 }
            );
            token_gone(&state);
        }
    }
}

#[test]
fn bp17_p30_and_t03_damage_only_opposing_leaders_at_combo_five() {
    for card in ["BP17-P30", "BP17-T03"] {
        for prior in [3_usize, 4] {
            let mut game = game(&arrow_position(card, prior));
            play_priors(&mut game, prior);
            resolved(game.play_internal_card("source", actor()).unwrap());
            game.resume_internal_card().unwrap();
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P2"]["leader"]["life"],
                if prior == 4 { 18_u8 } else { 20_u8 }
            );
            assert_eq!(
                state["P1"]["leader"]["life"],
                if prior == 4 { 24_u8 } else { 23_u8 },
                "Repair Mode's own healing is retained"
            );
            assert_eq!(state["objects"]["enemy"]["damage"], 0_u8);
            token_gone(&state);
        }
    }
}

// Only the external base is synthetic; PR-102 and SD01-004 keep their complete real program.
fn rose_game(initial: &Value) -> Game {
    struct Fixture {
        selected: SelectedEffects,
        base: WellFormedCardProgram,
    }
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    let fixture = FIXTURE.get_or_init(|| {
        let (snapshot, keywords, mut input) = inputs().clone();
        let mut snapshot = snapshot.trim_end().to_owned();
        let card = snapshot.lines().map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|card| card["number"] == "PR-102").unwrap();
        let mut face = card["faces"][0].clone();
        face["card_type"] = json!("フォロワー");
        face["cost"] = json!("1");
        face["text"] = Value::Null;
        face["sections"] = json!([]);
        snapshot.push('\n');
        snapshot.push_str(&json!({"number":"backfill-rose-base","faces":[face]}).to_string());
        input["documents"].as_array_mut().unwrap().push(json!(["backfill-rose-base.yaml",
            "format: 1\nkind: effect_set\ncards:\n  backfill-rose-base:\n    status: complete\n    abilities: []\n"]));
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut rules: Value = serde_saphyr::from_str(&read_to_string(root.join("authored/rules/engine/index.yaml")).unwrap()).unwrap();
        rules["input"] = rule_fixtures::rules(&snapshot, json!([]), json!([]))["input"].clone();
        let selected = Catalog::from_current_selection_with_rules(&snapshot, &keywords, &input,
            &rules.to_string(), &EngineIdentityInput::LegacyJp).unwrap();
        let mut tree = expanded::tree().clone();
        let mut cards = Map::new();
        cards.insert("backfill-rose-base".into(), tree["cards"]["synthetic-evolution"].clone());
        tree["cards"] = Value::Object(cards);
        let mut programs = lowerer().lower(&tree).unwrap();
        Fixture { selected, base: programs.remove("backfill-rose-base").unwrap() }
    });
    let mut game = Game::from_selected_effects(
        fixture.selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "backfill-rose",
    )
    .unwrap();
    game.load_internal_card("backfill-rose-base", &fixture.base)
        .unwrap();
    game
}

#[test]
fn pr102_and_sd01_004_evolution_bounce_then_draw_without_counting_evolution_as_a_play() {
    for card in ["PR-102", "SD01-004"] {
        for prior in [2_usize, 3] {
            let mut initial = position("backfill-rose-base", "field");
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
            add_priors(&mut initial, prior);
            let mut game = rose_game(&initial);
            play_priors(&mut game, prior);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            let choice = trigger(&mut game, card);
            target(&mut game, choice);
            game.resume_internal_card().unwrap();
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P2"]["field"], json!([]));
            assert_eq!(state["P2"]["hand"], json!(["enemy"]));
            assert_eq!(
                state["P1"]["hand"],
                if prior == 3 {
                    json!(["drawn"])
                } else {
                    json!([])
                },
                "evolution is not a CardPlayed event for Combo"
            );
            assert_eq!(
                state["P1"]["deck"],
                if prior == 3 {
                    json!([])
                } else {
                    json!(["drawn"])
                }
            );
        }
    }
}
