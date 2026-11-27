use super::*;
use crate::{expanded, rule_fixtures};
use alloc::collections::BTreeMap;
use serde_json::Map;
use std::{fs::read_to_string, path::PathBuf, sync::OnceLock};

fn position(card: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"}
    ]);
    initial
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
            ("play-targets-base", "BP20-047"),
            ("evolve-choice-base", "BP17-075"),
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
            cards.insert(base.into(), json!({"status":"complete"}));
            programs.insert(base.into(), template.clone());
        }
        let document = json!({"format":1_u8,"kind":"fixture_cards","cards":cards}).to_string();
        input["documents"]
            .as_array_mut()
            .unwrap()
            .push(json!(["evolution-form-producers.yaml", document]));
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let rules_text = read_to_string(root.join("authored/rules/engine/index.yaml")).unwrap();
        let mut rules: Value = serde_saphyr::from_str(&rules_text).unwrap();
        rules["input"] = rule_fixtures::rules(&snapshot, json!([]), json!([]))["input"].clone();
        let selected = repository::select_with_rules(
            &snapshot,
            &keywords,
            &input,
            &rules.to_string(),
        )
        .unwrap();
        tree["cards"] = Value::Object(programs);
        EvolutionFixture {
            selected,
            programs: lowerer().lower(&tree).unwrap(),
        }
    })
}

pub(super) fn form_game(base: &str, form: &str, enemy: bool) -> Game {
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
        "evolution-form",
    )
    .unwrap();
    game.load_internal_card(base, &fixture.programs[base])
        .unwrap();
    evolve_action(&mut game, 1);
    game
}
