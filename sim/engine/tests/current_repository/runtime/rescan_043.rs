#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the real card scenarios."
)]
use super::*;
use crate::{expanded, rule_fixtures};
use alloc::collections::BTreeMap;
use serde_json::Map;
use std::{fs::read_to_string, path::PathBuf, sync::OnceLock};
use sve_engine::catalog::EngineIdentityInput;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"}
    ]);
    initial
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "rescan-043",
    )
    .unwrap()
}

fn trigger(game: &mut Game, card: &str) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = question.domain else {
        panic!("real evolution trigger");
    };
    assert_eq!(options.len(), 1, "one real evolution trigger is pending");
    assert_eq!(
        options[0].card, card,
        "the ability belongs to the real evolved printing"
    );
    let mut play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    while let CardPlay::Awaiting(target_question) = play {
        let InputDomain::Targets(domain) = target_question.domain else {
            panic!("opposing follower decision");
        };
        assert_eq!(
            domain.members.len(),
            1,
            "only the opposing field follower is selectable"
        );
        play = game
            .answer_internal_card(
                target_question.actor,
                target_question.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap();
    }
    resolved(play);
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
        "the allowed intrinsic evolution resolves"
    );
}

fn evolve(card: &str, form: &str) {
    let mut initial = position(card, "field");
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
        "evolution pays the full printed cost"
    );
    assert_eq!(
        state["P1"]["evolve_deck"],
        json!([]),
        "the matching resource leaves its deck"
    );
}

// Synthetic bases provide evolution without admitting the unrelated real base cards.
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
            ("rescan043-hinako", "ECP02-009"),
            ("rescan043-kotoka", "ECP02-068"),
            ("rescan043-chitose", "ECP02-054"),
        ] {
            let record = snapshot
                .lines()
                .map(|line| serde_json::from_str::<Value>(line).unwrap())
                .find(|record| record["number"] == form)
                .unwrap();
            let companion = json!({
                "number":base,
                "faces":[{
                    "name":record["faces"][0]["name"],
                    "card_class":record["faces"][0]["card_class"],
                    "card_type":"フォロワー","cost":"1","power":"1","hp":"1",
                    "traits":[],"text":null,"sections":[]
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
            .push(json!(["rescan043-producers.yaml", document]));
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

fn form_game(base: &str, form: &str, enemy: bool) -> Game {
    let mut initial = position(base, "field");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    if enemy {
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"ally","card":"BP01-037"}));
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"off-zone","card":"BP01-037"}]);
    }
    let fixture = evolution_fixture();
    let mut game = Game::from_selected_effects(
        fixture.selected.clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "rescan043-evolution",
    )
    .unwrap();
    game.load_internal_card(base, &fixture.programs[base])
        .unwrap();
    evolve_action(&mut game, 1);
    game
}

fn magic_item(card: &str) {
    let mut game = game(&position(card, "ex"));
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["name"],
        "魔法のアイテム",
        "printed illustrations share the rules name from Q2215"
    );
    resolved(game.play_internal_card("source", actor()).unwrap());
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["pp"]["current"], 6_u8,
        "the spell token pays its printed four PP"
    );
    assert_eq!(
        state["P1"]["leader"]["life"], 11_u8,
        "the spell restores one life to its controller"
    );
    assert_eq!(
        state["P2"]["leader"]["life"], 20_u8,
        "the opponent does not gain life"
    );
    assert_eq!(
        state["P1"]["hand"],
        json!(["first"]),
        "one own top card is drawn"
    );
    assert_eq!(
        state["P1"]["deck"],
        json!(["second"]),
        "the remaining deck order is preserved"
    );
    assert_eq!(
        state["P1"]["ex"],
        json!([]),
        "the played spell token leaves EX"
    );
    assert_eq!(
        state["P1"]["cemetery"],
        json!([]),
        "the spell token is erased from the cemetery"
    );
    assert!(
        state["objects"]["source"].is_null(),
        "the resolved token no longer exists"
    );
    game.resume_internal_card().unwrap();
}

#[test]
fn ecp02_009_and_alias_draw_then_restore_life_on_real_evolution() {
    for card in ["ECP02-009", "ECP02-P04"] {
        let mut game = form_game("rescan043-hinako", card, false);
        trigger(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["first"]));
        assert_eq!(state["P1"]["deck"], json!(["second"]));
        assert_eq!(state["P1"]["leader"]["life"], 12_u8);
        assert_eq!(state["P2"]["leader"]["life"], 20_u8);
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    }
}

#[test]
fn ecp02_068_and_alias_banish_only_an_opposing_field_follower_on_evolution() {
    for card in ["ECP02-068", "ECP02-P36"] {
        let mut game = form_game("rescan043-kotoka", card, true);
        trigger(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["field"], json!([]));
        assert_eq!(state["P2"]["banish"], json!(["enemy"]));
        assert_eq!(state["P2"]["cemetery"], json!([]));
        assert_eq!(state["P2"]["ex"], json!(["off-zone"]));
        assert_eq!(state["P1"]["field"], json!(["source", "ally"]));
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    }
}

#[test]
fn ecp02_030_and_alias_evolve_for_one_pp() {
    for card in ["ECP02-030", "ECP02-P14"] {
        evolve(card, "ECP02-031");
    }
}
#[test]
fn ecp02_042_and_alias_evolve_for_one_pp() {
    for card in ["ECP02-042", "ECP02-P21"] {
        evolve(card, "ECP02-043");
    }
}

macro_rules! magic_item_tests {
    ($(($test:ident, $primary:literal, $alias:literal)),* $(,)?) => {
        $(#[test]
        fn $test() { for card in [$primary, $alias] { magic_item(card); } })*
    };
}
magic_item_tests!(
    (
        ecp02_sl34_and_t01_heal_draw_and_are_erased_after_play,
        "ECP02-SL34",
        "ECP02-T01"
    ),
    (
        ecp02_sl35_and_t02_heal_draw_and_are_erased_after_play,
        "ECP02-SL35",
        "ECP02-T02"
    ),
    (
        ecp02_sl36_and_t03_heal_draw_and_are_erased_after_play,
        "ECP02-SL36",
        "ECP02-T03"
    ),
    (
        ecp02_sl37_and_t04_heal_draw_and_are_erased_after_play,
        "ECP02-SL37",
        "ECP02-T04"
    ),
    (
        ecp02_sl38_and_t05_heal_draw_and_are_erased_after_play,
        "ECP02-SL38",
        "ECP02-T05"
    ),
    (
        ecp02_sl39_and_t06_heal_draw_and_are_erased_after_play,
        "ECP02-SL39",
        "ECP02-T06"
    ),
    (
        ecp02_sl40_and_t07_heal_draw_and_are_erased_after_play,
        "ECP02-SL40",
        "ECP02-T07"
    ),
    (
        ecp02_sl41_and_t08_heal_draw_and_are_erased_after_play,
        "ECP02-SL41",
        "ECP02-T08"
    ),
    (
        ecp02_sl42_and_t09_heal_draw_and_are_erased_after_play,
        "ECP02-SL42",
        "ECP02-T09"
    ),
    (
        ecp02_sl43_and_t10_heal_draw_and_are_erased_after_play,
        "ECP02-SL43",
        "ECP02-T10"
    ),
    (
        ecp02_sl44_and_t11_heal_draw_and_are_erased_after_play,
        "ECP02-SL44",
        "ECP02-T11"
    ),
    (
        ecp02_sl45_and_t12_heal_draw_and_are_erased_after_play,
        "ECP02-SL45",
        "ECP02-T12"
    ),
    (
        ecp02_sl46_and_t13_heal_draw_and_are_erased_after_play,
        "ECP02-SL46",
        "ECP02-T13"
    ),
    (
        ecp02_sl47_and_t14_heal_draw_and_are_erased_after_play,
        "ECP02-SL47",
        "ECP02-T14"
    ),
);

#[test]
fn magic_item_with_insufficient_pp_preserves_ex_life_and_deck() {
    let mut initial = position("ECP02-SL34", "ex");
    initial["players"]["P1"]["pp"]["current"] = json!(3_u8);
    let mut game = game(&initial);
    game.play_internal_card("source", actor()).unwrap_err();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 3_u8);
    assert_eq!(state["P1"]["leader"]["life"], 10_u8);
    assert_eq!(state["P1"]["ex"], json!(["source"]));
    assert_eq!(state["P1"]["deck"], json!(["first", "second"]));
    assert_eq!(state["P1"]["hand"], json!([]));
}

#[test]
fn ecp02_054_and_alias_require_an_opposing_target_before_damage_and_milling() {
    for card in ["ECP02-054", "ECP02-P27"] {
        for available in [false, true] {
            let mut game = form_game("rescan043-chitose", card, available);
            let pending = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = pending.domain else {
                panic!("real evolution trigger");
            };
            assert_eq!(options.len(), 1);
            assert_eq!(options[0].card, card);
            let play = game
                .answer_internal_card(
                    pending.actor,
                    pending.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap();
            if available {
                let CardPlay::Awaiting(question) = play else {
                    panic!("the mandatory target is selected before resolving either instruction");
                };
                let InputDomain::Targets(domain) = question.domain else {
                    panic!("opposing follower target");
                };
                assert_eq!(domain.members.len(), 1);
                let before = game.projection(View::Referee).unwrap();
                assert_eq!(before["P1"]["deck"], json!(["first", "second"]));
                assert_eq!(before["P1"]["cemetery"], json!([]));
                resolved(
                    game.answer_internal_card(
                        question.actor,
                        question.token,
                        Answer::Targets(vec![domain.members[0].handle]),
                    )
                    .unwrap(),
                );
                game.resume_internal_card().unwrap();
                let after = game.projection(View::Referee).unwrap();
                assert_eq!(after["P2"]["field"], json!([]));
                assert_eq!(after["P2"]["cemetery"], json!(["enemy"]));
                assert_eq!(after["P2"]["ex"], json!(["off-zone"]));
                assert_eq!(after["P1"]["field"], json!(["source", "ally"]));
                assert_eq!(after["P1"]["deck"], json!([]));
                assert_eq!(after["P1"]["cemetery"], json!(["first", "second"]));
            } else {
                assert_eq!(play, CardPlay::Cancelled);
                game.resume_internal_card().unwrap();
                let after = game.projection(View::Referee).unwrap();
                assert_eq!(after["P1"]["deck"], json!(["first", "second"]));
                assert_eq!(after["P1"]["cemetery"], json!([]));
                assert!(game.internal_pending_choice().unwrap().is_none());
            }
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                9_u8
            );
        }
    }
}
