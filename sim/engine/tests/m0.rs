//! M0/M1 regression tests: one test per fixed known error (docs/m0/known-errors.md)
//! and the R1–R3 properties from the design cross-review.

#![expect(
    clippy::unwrap_used,
    clippy::default_numeric_fallback,
    reason = "Synthetic fixtures are literal JSON; a construction error or missing field must fail the test."
)]

extern crate alloc;

use alloc::sync::Arc;

use serde_json::{Value, json};
use sve_engine::catalog::Catalog;
use sve_engine::game::Game;

fn snapshot() -> String {
    [("unit-follower", "フォロワー"), ("unit-spell", "スペル")]
        .iter()
        .map(|(number, kind)| {
            json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,"cost":"1","power":"2","hp":"3","traits":[],"text":null,"sections":[]}]})
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn registry() -> String {
    json!({"version":"astra/1","keywords":{
        "guard":{"ja":"守護","rule":"12.8","expansion":{"op":"keyword","name":"guard"}},
        "fanfare":{"ja":"ファンファーレ","role":"ability-label"},
        "last_words":{"ja":"ラストワード","role":"ability-label"}
    }})
    .to_string()
}

fn document(follower: &Value, spell: &Value) -> Value {
    json!({"version":"astra/1","cards":{
        "unit-follower":{"status":"complete","review":"synthetic","abilities":follower},
        "unit-spell":{"status":"complete","review":"synthetic","abilities":spell}
    }})
}

fn load(follower: &Value, spell: &Value) -> Result<Catalog, sve_engine::EngineFailure> {
    Catalog::from_documents(
        &snapshot(),
        &registry(),
        &[("unit.yaml".into(), document(follower, spell).to_string())],
    )
}

fn player(field: &Value, cemetery: &Value) -> Value {
    json!({"leader":{"class":"ニュートラル","life":20},"pp":{"current":5,"max":5},"ep":0,"sep":0,
        "construction":"class","zones":{"deck":[{"filler":10}],"field":field,"cemetery":cemetery}})
}

fn start(catalog: Catalog, setup: &Value) -> Game {
    Game::new(Arc::new(catalog), setup, &Value::Null, &Value::Null, "m0").unwrap()
}

/// KE-03 (contract 34): a pending trigger named by a setup label is still offered by
/// its ability reference; the label stays usable as decision shorthand.
#[test]
fn ke03_setup_labelled_pending_is_offered_by_ability_reference() {
    let follower = json!([{"line":1,"kind":"trigger","event":"field_to_cemetery","subject":"self",
        "body":{"op":"damage","subjects":"opponent.leader","amount":1}}]);
    let setup = json!({
        "turn":{"active":"P1","first_player":"P1","elapsed_turns":{"P1":3,"P2":2},"phase":"main"},
        "history":"explicit",
        "players":{"P1":player(&json!([]),&json!([{"id":"a","card":"unit-follower"}])),
                   "P2":player(&json!([]),&json!([]))},
        "semantic_state":{"pending_triggers":[
            {"id":"ta","controller":"P1","ability":{"source":"a","line":1},"event":{"left_field":"a"}}
        ],"check_timing":{"in_progress":true,"rules_processed":true}}
    });
    let mut engine = start(load(&follower, &json!([])).unwrap(), &setup);
    let legal = engine.legal().unwrap();
    assert_eq!(legal.len(), 1, "{legal:?}");
    assert_eq!(
        legal[0]["pending"],
        json!({"ability":{"source":"a","line":1}}),
        "{legal:?}"
    );
    let step = engine
        .decide(&json!({"do":"choose-pending","pending":"ta"}), "1")
        .unwrap();
    assert_eq!(step.outcome, "resolved");
}

fn pending_setup(ability: &Value) -> Value {
    json!({
        "turn":{"active":"P1","first_player":"P1","elapsed_turns":{"P1":3,"P2":2},"phase":"main"},
        "history":"explicit",
        "players":{"P1":player(&json!([]),&json!([{"id":"a","card":"unit-follower"}])),
                   "P2":player(&json!([]),&json!([]))},
        "semantic_state":{"pending_triggers":[
            {"controller":"P1","ability":ability,"event":{"left_field":"a"}}
        ],"check_timing":{"in_progress":true,"rules_processed":true}}
    })
}

/// KE-04 (contract 3.1): abilities sharing a printed line are told apart by keyword,
/// and a reference that names a different keyword never matches.
#[test]
fn ke04_shared_line_references_carry_and_check_the_keyword() {
    let hit = json!({"op":"damage","subjects":"opponent.leader","amount":1});
    let shared = json!([
        {"line":1,"kind":"trigger","keyword":"fanfare","event":"enter","subject":"self","body":hit},
        {"line":1,"kind":"trigger","keyword":"last_words","event":"field_to_cemetery","subject":"self","body":hit}
    ]);
    let last_words = json!({"source":"a","line":1,"keyword":"ラストワード"});
    let mut engine = start(
        load(&shared, &json!([])).unwrap(),
        &pending_setup(&last_words),
    );
    assert_eq!(engine.legal().unwrap()[0]["pending"]["ability"], last_words);
    let wrong = json!({"do":"choose-pending","pending":{"ability":{"source":"a","line":1,"keyword":"ファンファーレ"}}});
    engine.decide(&wrong, "wrong").unwrap_err();
    let alone = json!([
        {"line":1,"kind":"trigger","keyword":"last_words","event":"field_to_cemetery","subject":"self","body":hit}
    ]);
    let short = json!({"source":"a","line":1});
    let mut single = start(load(&alone, &json!([])).unwrap(), &pending_setup(&short));
    assert_eq!(single.legal().unwrap()[0]["pending"]["ability"], short);
    single.decide(&wrong, "wrong").unwrap_err();
    let named = json!({"do":"choose-pending","pending":{"ability":last_words}});
    assert_eq!(single.decide(&named, "named").unwrap().outcome, "resolved");
}
