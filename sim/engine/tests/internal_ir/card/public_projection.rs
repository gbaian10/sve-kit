use super::*;
use std::env::var;
use std::fs::read_to_string;
use sve_engine::game::player::Viewer;
use sve_engine::game::player::observation::ObservedZone;

fn position(snapshot: &str) -> Game {
    let rules = rule_fixtures::rules(
        snapshot,
        json!([rule_fixtures::title("synthetic", "TEST-001Ⓢa", json!([]))]),
        json!([]),
    );
    let catalog = rule_fixtures::catalog(
        snapshot,
        "format: 1\nkind: keyword_registry\nrule_names: {}\n",
        &[("synthetic".into(), json!({"format":1,"kind":"fixture_cards","cards":{"TEST-001Ⓢa":{"status":"complete"},"TEST-002":{"status":"complete"}}}).to_string())],
        &rules.to_string(),
    )
    .unwrap();
    let mut initial = setup(1, json!([]));
    for actor in ["P1", "P2"] {
        initial["players"][actor]["zones"]["hand"] = json!([]);
        initial["players"][actor]["zones"]["field"] = json!([]);
    }
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"s","card":"TEST-001Ⓢa"},{"id":"amulet","card":"TEST-002"}]);
    let mut game = Game::new(
        Arc::new(catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "public-projection",
    )
    .unwrap();
    for number in ["TEST-001Ⓢa", "TEST-002"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

#[test]
fn public_producer_bytes_support_native_observation_and_characteristics() {
    let path = var("SVE_TEST_BATTLE_PROJECTION").unwrap();
    let snapshot = read_to_string(path).unwrap();
    let mut game = position(&snapshot);
    let observed = game.observe(Viewer::P1).unwrap();
    let field = observed.players[0]
        .zones
        .iter()
        .find(|zone| zone.kind == ObservedZone::Field)
        .unwrap();
    let properties = &field
        .cards
        .iter()
        .find(|card| card.card.properties.card == "TEST-001Ⓢa")
        .unwrap()
        .card
        .properties;
    let amulet = &field
        .cards
        .iter()
        .find(|card| card.card.properties.card == "TEST-002")
        .unwrap()
        .card
        .properties;
    assert_eq!(amulet.kind, "アミュレット");
    assert_eq!(amulet.state.as_ref().unwrap().stats, None);
    assert_eq!(properties.names, ["Synthetic text"]);
    assert!(properties.kind.starts_with("フォロワー"));
    assert_eq!(properties.traits, ["Synthetic text"]);
    let stats = properties.state.as_ref().unwrap().stats.as_ref().unwrap();
    assert_eq!((stats.attack, stats.life), (2, 3));
    let destroy = checked(vec![instruction(
        1,
        Action::Destroy {
            objects: zone(ZoneKind::Field, 1),
        },
    )]);
    let result = game
        .execute_internal(&destroy, &source("synthetic"), context(1))
        .unwrap();
    assert!(
        matches!(&result.receipts[&ReceiptId(1)], InstructionOutcome::Destroy(outcome) if outcome.successful.len() == 2)
    );
    let mut rows = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    rows.iter_mut()
        .find(|row| row["number"] == "TEST-001Ⓢa")
        .unwrap()["faces"][0]["card_class"] = Value::Null;
    let mut missing_class = rows
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    missing_class.push('\n');
    let mut broken = position(&missing_class);
    broken
        .execute_internal(&destroy, &source("synthetic"), context(1))
        .unwrap_err();
}
