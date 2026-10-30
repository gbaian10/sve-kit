#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate the printed evolution scenarios."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;

fn selected() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let row = snapshot.lines().map(|line| serde_json::from_str::<Value>(line).unwrap()).find(|row| row["number"] == "BP01-037").unwrap();
        let mut face = row["faces"][0].clone();
        face["name"] = json!("evolution material");
        face["text"] = Value::Null;
        face["sections"] = json!([]);
        face["power"] = json!("10");
        face["hp"] = json!("20");
        snapshot.push_str(&json!({"number":"evolve-enemy","faces":[face]}).to_string());
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!(["evolve-enemy.yaml","format: 1\nkind: effect_set\ncards:\n  evolve-enemy:\n    status: complete\n    abilities: []\n"]));
        let text = read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../authored/rules/engine/index.yaml")).unwrap();
        let mut rules:Value = serde_saphyr::from_str(&text).unwrap();
        rules["input"]["snapshot_sha256"] = json!(rule_fixtures::hash(snapshot.as_bytes()));
        Catalog::from_current_selection_with_rules(&snapshot,&keywords,&input,&rules.to_string(),&EngineIdentityInput::LegacyJp).unwrap()
    })
}

fn position(card: &str, side: &str) -> Value {
    let mut value = draw_position();
    value["turn"]["active"] = json!(side);
    value["players"][side]["pp"] = json!({"current":10_u8,"max":10_u8});
    value["players"]["P1"]["zones"]["field"] = json!([{"id":"observer","card":card}]);
    value["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"evolve-enemy"}]);
    value["players"][side]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"subject","card":"BP02-118"}));
    value["players"][side]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP02-119"}]);
    value
}

pub(super) fn initial_game(card: &str, side: &str) -> Game {
    let mut game = Game::from_selected_effects(
        selected().clone(),
        &position(card, side),
        &Value::Null,
        &Value::Null,
        "printed-evolution-observer",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    game.load_internal_card("evolve-enemy", &empty).unwrap();
    game
}

pub(super) fn evolved(card: &str, side: &str) -> Game {
    let mut game = initial_game(card, side);
    let result = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"subject","evolve_card":"form","pay":{"pp":3_u8}}),
        )
        .unwrap();
    assert_eq!(
        result.outcome, "resolved",
        "the real Ward-only form evolves for its printed three PP"
    );
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    game
}

pub(super) fn resolve_observer(game: &mut Game, card: &str, expected: bool) {
    let question = game.internal_pending_choice().unwrap();
    if !expected {
        assert!(
            question.is_none(),
            "the opposing evolution has no observer or Ward-only form trigger"
        );
        return;
    }
    let question = question.unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("evolution occurrences")
    };
    assert_eq!(
        options.len(),
        1,
        "the Ward-only form produces just the printed external observer"
    );
    let observer = options.iter().find(|option| option.card == card);
    assert_eq!(
        observer.is_some(),
        expected,
        "only the controller's follower evolution matches"
    );
    let Some(observer) = observer else { return };
    let mut play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(observer.handle),
        )
        .unwrap();
    while let CardPlay::Awaiting(declaration) = play {
        let InputDomain::Targets(domain) = &declaration.domain else {
            panic!("formal damage target")
        };
        let enemy = domain
            .members
            .iter()
            .find(|m| m.card.as_deref() == Some("evolve-enemy"))
            .unwrap();
        play = game
            .answer_internal_card(
                declaration.actor,
                declaration.token,
                Answer::Targets(vec![enemy.handle]),
            )
            .unwrap();
    }
    resolved(play);
}

#[test]
fn bp02_012_printings_observe_own_evolution_and_declare_a_damage_target() {
    for card in ["BP02-012", "ETD03-011"] {
        for side in ["P1", "P2"] {
            let mut game = evolved(card, side);
            resolve_observer(&mut game, card, side == "P1");
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                if side == "P1" { 2_u8 } else { 0_u8 }
            );
        }
    }
}

#[test]
fn bp02_029_gains_stats_only_for_its_controllers_evolution() {
    for side in ["P1", "P2"] {
        let before = initial_game("BP02-029", side);
        let stats = before.projection(View::Referee).unwrap()["objects"]["observer"].clone();
        let mut game = evolved("BP02-029", side);
        resolve_observer(&mut game, "BP02-029", side == "P1");
        let after = game.projection(View::Referee).unwrap();
        for stat in ["power", "hp"] {
            assert_eq!(
                after["objects"]["observer"][stat],
                stats[stat].as_u64().unwrap() + u64::from(side == "P1")
            );
        }
    }
}

#[test]
fn bp02_083_printings_damage_only_the_opposing_leader_on_own_evolution() {
    for card in ["BP02-083", "ETD02-012", "PR-025"] {
        for side in ["P1", "P2"] {
            let mut game = evolved(card, side);
            resolve_observer(&mut game, card, side == "P1");
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P2"]["leader"]["life"],
                if side == "P1" { 18_u8 } else { 20_u8 }
            );
            assert_eq!(state["P1"]["leader"]["life"], 20_u8);
        }
    }
}
