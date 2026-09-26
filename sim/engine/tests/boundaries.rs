//! Synthetic cards exercise rules independently of printed-card identities.

extern crate alloc;

use alloc::sync::Arc;

use serde_json::{Value, json};
use sve_engine::EngineFailure;
use sve_engine::catalog::Catalog;
use sve_engine::game::{Game, View};

fn snapshot() -> String {
    [("unit-follower","フォロワー"),("unit-spell","スペル")].iter().map(|(number,kind)|json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,"cost":"1","power":"2","hp":"3","traits":[],"text":null,"sections":[]}]}).to_string()).collect::<Vec<_>>().join("\n")
}
fn registry() -> String {
    json!({"version":"astra/1","keywords":{"guard":{"ja":"守護","rule":"12.8","expansion":{"op":"keyword","name":"guard"}}}}).to_string()
}
fn document(body: &Value) -> Value {
    json!({"version":"astra/1","cards":{"unit-follower":{"status":"complete","review":"synthetic","abilities":[]},"unit-spell":{"status":"complete","review":"synthetic","abilities":[{"kind":"spell","line":1,"targets":[{"key":"1","select":{"zone":"field","side":"opponent","type":"follower"},"min":1,"max":1}],"body":body}]}}})
}
#[expect(
    clippy::unwrap_used,
    reason = "A synthetic fixture construction error must fail its test."
)]
fn catalog(body: &Value) -> Catalog {
    Catalog::from_documents(
        &snapshot(),
        &registry(),
        &[("unit.yaml".into(), document(body).to_string())],
    )
    .unwrap()
}
fn setup() -> Value {
    json!({"turn":{"active":"P1","first_player":"P1","elapsed_turns":{"P1":3,"P2":2},"phase":"main"},"players":{"P1":{"leader":{"class":"ロイヤル","life":20},"pp":{"current":2,"max":2},"ep":0,"sep":0,"construction":"class","zones":{"field":[{"id":"a","card":"unit-follower"}],"hand":[{"id":"s","card":"unit-spell"}]}},"P2":{"leader":{"class":"ウィッチ","life":20},"pp":{"current":0,"max":2},"ep":0,"sep":0,"construction":"class","zones":{"field":[{"id":"b","card":"unit-follower"}]}}}})
}
#[expect(
    clippy::unwrap_used,
    reason = "A synthetic fixture construction error must fail its test."
)]
fn game(body: &Value) -> Game {
    Game::new(
        Arc::new(catalog(body)),
        &setup(),
        &Value::Null,
        &Value::Null,
        "unit-seed",
    )
    .unwrap()
}

#[test]
fn loader_rejects_invalid_grammar_duplicates_and_macro_cycles() {
    let valid = document(&json!({"op":"damage","subjects":"target.1","amount":2_i64}));
    let mut bad = valid.clone();
    bad["cards"]["unit-spell"]["abilities"][0]["body"] =
        json!({"op":"damage","subjects":"target.1"});
    Catalog::from_documents(
        &snapshot(),
        &registry(),
        &[("bad.yaml".into(), bad.to_string())],
    )
    .unwrap_err();
    let mut unknown = valid.clone();
    unknown["cards"]["unit-spell"]["abilities"][0]["body"] = json!({"op":"macro","name":"missing"});
    Catalog::from_documents(
        &snapshot(),
        &registry(),
        &[("bad.yaml".into(), unknown.to_string())],
    )
    .unwrap_err();
    let mut cycle = valid.clone();
    cycle["cards"]["unit-spell"]["abilities"][0]["body"] = json!({"op":"macro","name":"guard"});
    let registry_cycle = json!({"version":"astra/1","keywords":{"guard":{"ja":"守護","expansion":{"op":"macro","name":"guard"}}}});
    Catalog::from_documents(
        &snapshot(),
        &registry_cycle.to_string(),
        &[("cycle.yaml".into(), cycle.to_string())],
    )
    .unwrap_err();
    let docs = vec![
        ("first.yaml".into(), valid.to_string()),
        ("second.yaml".into(), valid.to_string()),
    ];
    Catalog::from_documents(&snapshot(), &registry(), &docs).unwrap_err();
    Catalog::from_documents(&format!("{}\n{}", snapshot(), snapshot()), &registry(), &[])
        .unwrap_err();
}

#[test]
fn illegal_and_unsupported_decisions_rollback_every_state_component() {
    for body in [
        json!({"op":"damage","subjects":"target.1","amount":2_i64}),
        json!({"op":"unsupported","reason":"synthetic unsupported frontier"}),
    ] {
        let mut engine = game(&body);
        let before = engine.digest().unwrap();
        engine.decide(&json!(false), "malformed").unwrap_err();
        assert_eq!(engine.digest().unwrap(), before);
        let denied = engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["a"]}}),
                "illegal-target",
            )
            .unwrap();
        assert_eq!(denied.outcome, "cannot-play");
        assert_eq!(engine.digest().unwrap(), before);
        let wrong_window = engine
            .decide(&json!({"do":"pass"}), "wrong-window")
            .unwrap();
        assert_eq!(wrong_window.outcome, "cannot-play");
        assert_eq!(engine.digest().unwrap(), before);
        let result = engine.decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "valid-target",
        );
        if let Err(error) = result {
            assert!(matches!(error, EngineFailure::Unsupported(_)));
            assert_eq!(engine.digest().unwrap(), before);
        } else {
            assert_eq!(
                engine.query(View::P1, "P2.field.b.hp").unwrap(),
                Some(json!(1_i64))
            );
        }
    }
}

#[test]
fn partial_programs_fail_closed_and_unseen_hand_has_no_identity() {
    let mut partial = document(&json!({"op":"draw","count":1_i64}));
    partial["cards"]["unit-spell"]["status"] = json!("partial");
    let catalog = Catalog::from_documents(
        &snapshot(),
        &registry(),
        &[("partial.yaml".into(), partial.to_string())],
    )
    .unwrap();
    assert_eq!(catalog.authored_count(), 2);
    let partial_engine = Game::new(
        Arc::new(catalog),
        &setup(),
        &Value::Null,
        &Value::Null,
        "unit",
    )
    .unwrap();
    assert!(matches!(
        partial_engine.legal(),
        Err(EngineFailure::Unsupported(_))
    ));
    let engine = game(&json!({"op":"damage","subjects":"target.1","amount":2_i64}));
    let packet = engine.projection(View::P2).unwrap();
    assert!(packet["objects"].get("s").is_none());
    assert_eq!(packet["P1"]["hand"], json!([{"filler":1_i64}]));
    assert_eq!(packet["awaiting"], json!({"by":"P1"}));
    assert!(packet.get("legal").is_none());
    assert!(engine.query(View::P2, "P1.hand.s.card").unwrap().is_none());
}
