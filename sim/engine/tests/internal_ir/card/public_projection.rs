use super::*;
use std::env::var;
use std::fs::read_to_string;
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::player::Viewer;
use sve_engine::game::player::observation::ObservedZone;

fn position(snapshot: &str) -> Game {
    let rules = rule_fixtures::rules(
        snapshot,
        json!([rule_fixtures::title("synthetic", "TEST-001Ⓢa", json!([]))]),
        json!([]),
    );
    let catalog = Catalog::from_documents_with_rules(
        snapshot,
        "format: 1\nkind: keyword_registry\nkeywords: {}\n",
        &[("synthetic".into(), json!({"format":1,"kind":"effect_set","cards":{"TEST-001Ⓢa":{"status":"complete","abilities":[]}}}).to_string())],
        &rules.to_string(),
        &EngineIdentityInput::LegacyJp,
    )
    .unwrap();
    let mut initial = setup(1, json!([]));
    for actor in ["P1", "P2"] {
        initial["players"][actor]["zones"]["hand"] = json!([]);
        initial["players"][actor]["zones"]["field"] = json!([]);
    }
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"TEST-001Ⓢa"}]);
    let mut game = Game::new(
        Arc::new(catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "public-projection",
    )
    .unwrap();
    game.load_internal_card("TEST-001Ⓢa", &card(vec![]))
        .unwrap();
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
    let properties = &field.cards[0].card.properties;
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
        matches!(&result.receipts[&ReceiptId(1)], InstructionOutcome::Destroy(outcome) if outcome.successful.len() == 1)
    );
    let mut missing_class: Value = serde_json::from_str(snapshot.trim()).unwrap();
    missing_class["faces"][0]["card_class"] = Value::Null;
    let mut broken = position(&format!("{missing_class}\n"));
    broken
        .execute_internal(&destroy, &source("synthetic"), context(1))
        .unwrap_err();
}
