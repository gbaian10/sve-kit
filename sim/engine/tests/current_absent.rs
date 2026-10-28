//! Proven empty effects remain distinct from unknown or nonempty snapshot text.

#![expect(
    clippy::unwrap_used,
    reason = "Small synthetic handoffs fail immediately if their construction is invalid."
)]

use core::fmt::Write as _;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use sve_engine::catalog::Catalog;
use sve_engine::game::Game;
use sve_engine::{EngineFailure, Result};

fn digest(bytes: &[u8]) -> String {
    let mut result = String::new();
    for byte in Sha256::digest(bytes) {
        write!(&mut result, "{byte:02x}").unwrap();
    }
    result
}

fn source() -> Value {
    json!({
        "recipe":"effect-source-v1", "normalizer":"synthetic-v1", "region":"jp",
        "token_links":[],
        "faces":[{
            "face_id":"f:22222222222222222222222222222222",
            "revision_id":"rev:33333333333333333333333333333333",
            "rules_names":[],
            "attributes":{
                "class_code":{"state":"known","value":"neutral"},
                "type_code":{"state":"known","value":"follower"},
                "cost":{"state":"known","value":1_u8},
                "attack":{"state":"known","value":1_u8},
                "defense":{"state":"known","value":1_u8},
                "traits":{"state":"known","value":[]},
                "titles":{"state":"known","value":[]},
                "special_kinds":{"state":"known","value":[]}
            },
            "texts":[
                {"field":"name","ordinal":null,"state":"present","source_hash":digest(b"synthetic")},
                {"field":"effect","ordinal":null,"state":"absent","source_hash":digest(b"")}
            ]
        }]
    })
}

fn printed() -> Value {
    json!({"name":"synthetic","card_type":"フォロワー",
        "cost":"1","power":"1","hp":"1","text":null,"sections":[]})
}

fn load(source: &Value, printed: &Value) -> Result<Game> {
    let version = format!(
        "effect-source-v1:sha256:{}",
        digest(source.to_string().as_bytes())
    );
    let handoff = json!({"documents":[],"authors":[{"status":"accepted","printings":["SYN-001"],"input":{
        "card_id":"c:11111111111111111111111111111111","card_no":"SYN-001",
        "source_version":version,"source_payload":source,
        "source_inventory":{"abilities":[],"non_body":[]},
        "sidecar":[],"declaration_sidecar":[],
        "tree":{"format":1_u8,"kind":"author_effect_set","scope":"shared","cards":{
            "SYN-001":{"faces":{},"meta":{
                "dsl":"1.0","source":version,"status":"draft","reviewed_by":[],
                "rulings":[],"verified_by_exam":[],
                "written_by":[{"actor":{"who":"synthetic test","model":null},"date":"2026-10-27"}]
            }}
        }}
    }}]});
    let snapshot = json!({"number":"SYN-001","faces":[printed]}).to_string();
    let keywords = r#"{"format":1,"kind":"keyword_registry","keywords":{}}"#;
    let selected = Catalog::from_current_selection(&snapshot, keywords, &handoff)?;
    let initial = json!({
        "history":"explicit",
        "turn":{"active":"P1","first_player":"P1","phase":"main",
            "elapsed_turns":{"P1":3_u8,"P2":3_u8}},
        "players":{
            "P1":{"leader":{"class":"ニュートラル","life":20_u8},
                "pp":{"current":1_u8,"max":1_u8},"ep":0_u8,"sep":0_u8,"zones":{}},
            "P2":{"leader":{"class":"ニュートラル","life":20_u8},
                "pp":{"current":1_u8,"max":1_u8},"ep":0_u8,"sep":0_u8,"zones":{}}
        }
    });
    Game::from_selected_effects(selected, &initial, &Value::Null, &Value::Null, "synthetic")
}

fn rejected(source: &Value, printed: &Value) {
    assert!(
        matches!(load(source, printed), Err(EngineFailure::Invalid(message))
        if message == "current face has no unique snapshot source"),
        "an invalid absence must fail the snapshot source match"
    );
}

#[test]
fn proven_absent_effect_matches_missing_null_or_empty_snapshot_text() {
    for value in [None, Some(Value::Null), Some(json!(""))] {
        let mut face = printed();
        if let Some(value) = value {
            face["text"] = value;
        } else {
            face.as_object_mut().unwrap().remove("text");
        }
        let result = load(&source(), &face);
        assert!(result.is_ok(), "{:?}", result.err());
    }
}

#[test]
fn proven_absent_effect_rejects_nonempty_snapshot_text() {
    for value in ["nonempty", " "] {
        let mut face = printed();
        face["text"] = json!(value);
        rejected(&source(), &face);
    }
}

#[test]
fn proven_absent_effect_requires_the_empty_source_hash() {
    for value in [Value::Null, json!(digest(b"nonempty"))] {
        let mut current = source();
        current["faces"][0]["texts"][1]["source_hash"] = value;
        rejected(&current, &printed());
    }
}

#[test]
fn proven_absence_does_not_apply_to_a_missing_name() {
    let mut current = source();
    current["faces"][0]["texts"][0]["state"] = json!("absent");
    current["faces"][0]["texts"][0]["source_hash"] = json!(digest(b""));
    let mut face = printed();
    face["name"] = Value::Null;
    rejected(&current, &face);
}
