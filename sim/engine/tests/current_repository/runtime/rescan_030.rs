#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the real card scenarios."
)]
use super::ex_select::{expanded, rule_fixtures};
use super::*;
use alloc::collections::BTreeMap;
use serde_json::Map;
use std::{fs::read_to_string, path::PathBuf, sync::OnceLock};
use sve_engine::catalog::EngineIdentityInput;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str, field: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"][if field { "field" } else { "hand" }] =
        json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"}
    ]);
    initial
}

fn play(game: &mut Game) {
    let result = game.play_internal_card("source", actor()).unwrap();
    if let CardPlay::Awaiting(question) = result {
        assert!(
            matches!(question.domain, InputDomain::WardEntry { .. }),
            "entry offers only the Ward posture decision"
        );
        assert!(
            matches!(
                game.answer_internal_card(question.actor, question.token, Answer::WardEntry(false))
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "choosing Ward posture completes entry"
        );
    } else {
        assert!(
            matches!(result, CardPlay::Entered(_)),
            "the real follower enters the field"
        );
    }
}

fn trigger(game: &mut Game, card: &str) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = question.domain else {
        panic!("trigger");
    };
    assert_eq!(options.len(), 1, "exactly one real card trigger is pending");
    assert_eq!(
        options[0].card, card,
        "the pending ability belongs to the real printing"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
}

fn evolve_action(game: &mut Game, cost: u8) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the permitted intrinsic evolution resolves"
    );
}

fn evolve_base(card: &str, form: &str) {
    let mut initial = position(card, true);
    initial["players"]["P1"]["pp"]["current"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    let mut insufficient = initial.clone();
    insufficient["players"]["P1"]["pp"]["current"] = json!(0_u8);
    let mut denied = game(&insufficient);
    if let Ok(step) = denied.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":0_u8}}),
    ) {
        assert_ne!(
            step.outcome, "resolved",
            "less than the printed cost cannot evolve"
        );
    }
    assert_eq!(
        denied.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        0_u8,
        "rejected evolution preserves PP"
    );
    let mut game = game(&initial);
    evolve_action(&mut game, 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["pp"]["current"], 0_u8,
        "the printed evolution cost is paid"
    );
    assert_eq!(
        state["P1"]["evolve_deck"],
        json!([]),
        "the matching evolution resource leaves its deck"
    );
}

// Synthetic textless bases produce evolution without admitting the unrelated real bases.
struct EvolutionFixture {
    selected: SelectedEffects,
    programs: BTreeMap<String, WellFormedCardProgram>,
}

fn evolution_fixture() -> &'static EvolutionFixture {
    static FIXTURE: OnceLock<EvolutionFixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let (snapshot, keywords, mut input) = inputs().clone();
        let mut snapshot = snapshot.trim_end().to_owned();
        let mut cards = Map::new();
        let mut tree = expanded::tree().clone();
        let template = tree["cards"]["synthetic-evolution"].clone();
        let mut programs = Map::new();
        for (base, form) in [
            ("rescan030-tony", "BP21-028"),
            ("rescan030-striker", "BP21-064"),
            ("rescan030-marron", "BP21-082"),
        ] {
            let record = snapshot
                .lines()
                .map(|line| serde_json::from_str::<Value>(line).unwrap())
                .find(|record| record["number"] == form)
                .unwrap();
            let companion = json!({
                "number": base,
                "faces": [{
                    "name": record["faces"][0]["name"],
                    "card_class": record["faces"][0]["card_class"],
                    "card_type": "フォロワー", "cost": "1", "power": "1", "hp": "1",
                    "traits": [], "text": null, "sections": []
                }]
            });
            snapshot.push('\n');
            snapshot.push_str(&companion.to_string());
            cards.insert(base.into(), json!({"status":"complete","abilities":[]}));
            programs.insert(base.into(), template.clone());
        }
        let document = json!({"format":1_u8,"kind":"effect_set","cards":cards}).to_string();
        input["documents"]
            .as_array_mut()
            .unwrap()
            .push(json!(["rescan030-producers.yaml", document]));
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let rules_text = read_to_string(root.join("authored/rules/engine/index.yaml")).unwrap();
        let mut rules: Value = serde_saphyr::from_str(&rules_text).unwrap();
        rules["input"] = rule_fixtures::rules(&snapshot, json!([]), json!([]))["input"].clone();
        let selected = Catalog::from_current_selection_with_rules(
            &snapshot,
            &keywords,
            &input,
            &rules.to_string(),
            &EngineIdentityInput::LegacyJp,
        )
        .unwrap();
        tree["cards"] = Value::Object(programs);
        EvolutionFixture {
            selected,
            programs: lowerer().lower(&tree).unwrap(),
        }
    })
}

fn form_game(base: &str, form: &str) -> Game {
    let mut initial = position(base, true);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":base,"state":{"entered_this_turn":true}}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    let fixture = evolution_fixture();
    let mut game = Game::from_selected_effects(
        fixture.selected.clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "rescan030-evolution",
    )
    .unwrap();
    game.load_internal_card(base, &fixture.programs[base])
        .unwrap();
    game
}

fn storm(base: &str, form: &str, life: u8) {
    let mut game = form_game(base, form);
    let attack = json!({"do":"attack","attacker":"source","target":"P2.leader"});
    assert_eq!(
        game.internal_rule_action(&attack).unwrap().outcome,
        "cannot-attack",
        "the unevolved source cannot attack on its entry turn"
    );
    evolve_action(&mut game, 1);
    assert_eq!(
        game.internal_rule_action(&attack).unwrap().outcome,
        "resolved",
        "Storm permits an immediate leader attack"
    );
    let question = game.internal_quick_choice().unwrap().unwrap();
    assert!(
        matches!(
            game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
                .unwrap(),
            CardPlay::Continued(_)
        ),
        "passing the Quick window resumes combat"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P2"]["leader"]["life"], life,
        "the evolved power damages the opposing leader"
    );
    assert_eq!(
        state["P1"]["pp"]["current"], 9_u8,
        "evolution consumes one PP"
    );
}

#[test]
fn bp21_009_and_alias_evolve_for_one_pp() {
    for card in ["BP21-009", "BP21-P04"] {
        evolve_base(card, "BP21-010");
    }
}
#[test]
fn bp21_049_evolves_for_one_pp() {
    evolve_base("BP21-049", "BP21-050");
}
#[test]
fn bp21_055_and_alias_evolve_for_one_pp() {
    for card in ["BP21-055", "BP21-SL13"] {
        evolve_base(card, "BP21-056");
    }
}

#[test]
fn bp21_017_damages_every_opposing_follower_and_no_ally() {
    let mut initial = position("BP21-017", false);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy-a","card":"BP01-037"},{"id":"enemy-b","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    trigger(&mut game, "BP21-017");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!([]));
    assert_eq!(state["P2"]["cemetery"], json!(["enemy-a", "enemy-b"]));
    assert_eq!(state["objects"]["ally"]["damage"], 0_u8);
    assert_eq!(state["objects"]["source"]["damage"], 0_u8);
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    assert_eq!(state["P1"]["pp"]["current"], 4_u8);
}

#[test]
fn bp21_028_and_alias_draw_one_on_real_evolution() {
    for card in ["BP21-028", "BP21-P14"] {
        let mut game = form_game("rescan030-tony", card);
        evolve_action(&mut game, 1);
        trigger(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["first"]));
        assert_eq!(state["P1"]["deck"], json!(["second"]));
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    }
}
#[test]
fn bp21_064_and_alias_storm_attack_after_evolving_this_turn() {
    for card in ["BP21-064", "BP21-P33"] {
        storm("rescan030-striker", card, 17);
    }
}
#[test]
fn bp21_082_and_promo_storm_attack_after_evolving_this_turn() {
    for card in ["BP21-082", "PR-574"] {
        storm("rescan030-marron", card, 16);
    }
}

#[test]
fn bp21_110_and_alias_heal_draw_and_evolve() {
    for card in ["BP21-110", "BP21-SL26"] {
        let mut initial = position(card, false);
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        let mut game = game(&initial);
        play(&mut game);
        trigger(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 11_u8);
        assert_eq!(state["P2"]["leader"]["life"], 20_u8);
        assert_eq!(state["P1"]["hand"], json!(["first"]));
        assert_eq!(state["P1"]["deck"], json!(["second"]));
        assert_eq!(state["P1"]["pp"]["current"], 7_u8);
        evolve_base(card, "BP21-111");
    }
}

#[test]
fn bp21_102_and_alias_ward_draw_and_restore_three_pp() {
    for card in ["BP21-102", "BP21-P55"] {
        let mut game = game(&position(card, false));
        play(&mut game);
        trigger(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["first"]));
        assert_eq!(state["P1"]["deck"], json!(["second"]));
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        assert_eq!(state["P1"]["pp"]["max"], 10_u8);
        let mut initial = draw_position();
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"ward","card":card,"state":{"acted":true}}]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"attacker","card":"BP01-037"}]);
        assert_eq!(
            super::game(&initial)
                .internal_rule_action(
                    &json!({"do":"attack","attacker":"attacker","target":"P2.leader"})
                )
                .unwrap()
                .outcome,
            "cannot-attack"
        );
        initial["players"]["P2"]["zones"]["field"][0]["state"]["acted"] = json!(false);
        assert_eq!(
            super::game(&initial)
                .internal_rule_action(
                    &json!({"do":"attack","attacker":"attacker","target":"P2.leader"})
                )
                .unwrap()
                .outcome,
            "resolved"
        );
    }
}

#[test]
fn bp21_p66_and_t06_ward_protects_and_lastword_heals_its_controller() {
    for card in ["BP21-P66", "BP21-T06"] {
        let mut initial = position(card, true);
        initial["turn"]["active"] = json!("P2");
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":card,"state":{"acted":true}}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"attacker","card":"BP01-037"}]);
        let mut unguarded = initial.clone();
        unguarded["players"]["P1"]["zones"]["field"][0]["state"]["acted"] = json!(false);
        assert_eq!(
            game(&unguarded)
                .internal_rule_action(
                    &json!({"do":"attack","attacker":"attacker","target":"P1.leader"})
                )
                .unwrap()
                .outcome,
            "resolved"
        );
        let mut game = with_producer(
            &initial,
            Action::Destroy {
                objects: subjects(),
            },
        );
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"attacker","target":"P1.leader"})
            )
            .unwrap()
            .outcome,
            "cannot-attack"
        );
        activate(&mut game, "P2");
        game.resume_internal_card().unwrap();
        let question = game.internal_pending_choice().unwrap().unwrap();
        assert_eq!(question.actor, actor());
        let InputDomain::Pending(options) = question.domain else {
            panic!("lastword");
        };
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].card, card);
        game.answer_internal_card(
            PlayerId { seat: 2 },
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap_err();
        resolved(
            game.answer_internal_card(actor(), question.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 11_u8);
        assert_eq!(state["P2"]["leader"]["life"], 20_u8);
        assert_eq!(state["P1"]["cemetery"], json!([]));
    }
}
