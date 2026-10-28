#![expect(
    clippy::panic,
    reason = "A different question invalidates the scenario."
)]

use super::*;
use crate::{expanded, rule_fixtures};
use std::{fs::read_to_string, path::PathBuf, sync::OnceLock};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::DecisionView;

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected a target question")
    };
    question
}

fn evolution_selection() -> &'static (SelectedEffects, WellFormedCardProgram) {
    static PREPARED: OnceLock<(SelectedEffects, WellFormedCardProgram)> = OnceLock::new();
    PREPARED.get_or_init(|| {
        let (snapshot, keywords, mut input) = inputs().clone();
        let form: Value = snapshot.lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|record| record["number"] == "BP04-102").unwrap();
        // The producer supplies only evolution; the real evolved program is unchanged.
        let companion_card = json!({"number":"synthetic-evolution","faces":[{
            "name":form["faces"][0]["name"],"card_class":"ニュートラル","card_type":"フォロワー",
            "cost":"1","power":"1","hp":"1","traits":[],"text":null,"sections":[]
        }]});
        let snapshot = format!("{}\n{companion_card}", snapshot.trim_end());
        input["documents"].as_array_mut().unwrap().push(json!(["trigger-companion.yaml",json!({
            "format":1_u8,"kind":"effect_set","cards":{"synthetic-evolution":{"status":"complete","abilities":[]}}
        }).to_string()]));
        let mut tree = expanded::tree().clone();
        tree["cards"] = json!({"synthetic-evolution":tree["cards"]["synthetic-evolution"]});
        let companion = lowerer().lower(&tree).unwrap().remove("synthetic-evolution").unwrap();
        let mut rules: Value = serde_saphyr::from_str(&read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../authored/rules/engine/index.yaml")
        ).unwrap()).unwrap();
        rules["input"] = rule_fixtures::rules(&snapshot, json!([]), json!([]))["input"].clone();
        let selected = Catalog::from_current_selection_with_rules(&snapshot, &keywords, &input, &rules.to_string(), &EngineIdentityInput::LegacyJp).unwrap();
        (selected, companion)
    })
}

#[test]
fn bp04_102_evolution_targets_one_opposing_follower_and_banishes_it() {
    let (selected, companion) = evolution_selection();
    for available in [false, true] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"base","card":"synthetic-evolution"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP04-102"}]);
        initial["players"]["P2"]["zones"]["field"] = if available {
            json!([{"id":"victim","card":"BP01-037"}])
        } else {
            json!([])
        };
        let mut game = Game::from_selected_effects(
            selected.clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "trigger-target-evolution",
        )
        .unwrap();
        game.load_internal_card("synthetic-evolution", companion)
            .unwrap();
        game.internal_rule_action(
            &json!({"do":"evolve","source":"base","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
        let play = select_pending(&mut game, "BP04-102");
        if available {
            let targets = awaiting(play);
            let result = resolved(
                game.answer_internal_card(targets.actor, targets.token, Answer::Targets(vec![0]))
                    .unwrap(),
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P2"]["field"], json!([]));
            assert_eq!(state["P2"]["banish"], json!(["victim"]));
            assert!(
                result
                    .progress
                    .facts
                    .iter()
                    .any(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
            );
        } else {
            assert_eq!(play, CardPlay::Cancelled);
        }
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["evolution"],
            json!(["form"])
        );
    }
}

fn select_pending(game: &mut Game, card: &str) -> CardPlay {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("expected a pending trigger")
    };
    assert_eq!(options.len(), 1, "one trigger belongs to the event subject");
    assert_eq!(
        options[0].card, card,
        "the selected trigger is the real card"
    );
    game.answer_internal_card(
        pending.actor,
        pending.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

#[test]
fn bp04_017_attack_declares_an_opposing_target_then_reduces_its_power() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"dryad","card":"BP04-017"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"victim","card":"BP01-037"}]);
    let baseline = game(&initial);
    for available in [false, true] {
        let mut game = if available {
            baseline.clone()
        } else {
            // A separate legal starting position tests the unplayable-trigger path.
            let mut empty = initial.clone();
            empty["players"]["P2"]["zones"]["field"] = json!([]);
            game(&empty)
        };
        game.internal_rule_action(&json!({"do":"attack","attacker":"dryad","target":"P2.leader"}))
            .unwrap();
        let play = select_pending(&mut game, "BP04-017");
        if !available {
            assert_eq!(play, CardPlay::Cancelled);
            game.resume_internal_card().unwrap();
            assert!(game.internal_pending_choice().unwrap().is_none());
            continue;
        }
        let question = awaiting(play);
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("targets")
        };
        assert_eq!(domain.members.len(), 1);
        let before = game.projection(View::Referee).unwrap();
        let power = before["objects"]["victim"]["power"].as_i64().unwrap();
        let authority = format!("{game:?}");
        game.answer_internal_card(
            PlayerId { seat: 2 },
            question.token,
            Answer::Targets(vec![0]),
        )
        .unwrap_err();
        assert_eq!(format!("{game:?}"), authority);
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![99]))
            .unwrap_err();
        assert_eq!(format!("{game:?}"), authority);
        let result = resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["objects"]["victim"]["power"], power - 1);
        assert_eq!(
            after["objects"]["dryad"]["power"],
            before["objects"]["dryad"]["power"]
        );
        assert_eq!(after["P1"]["pp"], before["P1"]["pp"]);
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
                .count(),
            1
        );
    }
}
