use alloc::sync::Arc;
use std::sync::OnceLock;

use serde_json::{Value, json};
use sve_engine::catalog::Catalog;

#[expect(
    clippy::unwrap_used,
    reason = "Invalid synthetic fixture data must fail the test."
)]
pub(crate) fn catalog() -> Arc<Catalog> {
    static CATALOG: OnceLock<Arc<Catalog>> = OnceLock::new();
    Arc::clone(CATALOG.get_or_init(|| {
        let snapshot = [("unit", "フォロワー"), ("spell", "スペル")]
            .map(|(number, kind)| json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,"cost":"0","power":"2","hp":"3","traits":[],"text":null,"sections":[]}]}).to_string()).join("\n");
        let body = json!({"op":"seq","steps":[
            {"op":"shuffle","subjects":{"zone":"deck","side":"self"}},
            {"op":"random","select":{"zone":"deck","side":"self"},"count":2_i64,"bind":"picked"},
            {"op":"reveal","subjects":"picked","to":"all"},
            {"op":"dice","count":1_i64,"bind":"die"},
            {"op":"damage","subjects":"opponent.leader","amount":{"read":"die"}},
            {"op":"optional","then":{"op":"seq","steps":[
                {"op":"dice","count":1_i64,"bind":"again"},
                {"op":"damage","subjects":"opponent.leader","amount":{"read":"again"}}
            ]}}
        ]});
        let document = json!({"format":1_u8,"kind":"effect_set","cards":{
            "unit":{"status":"complete","review":"synthetic","abilities":[]},
            "spell":{"status":"complete","review":"synthetic","abilities":[{"kind":"spell","line":1_i64,"targets":[],"body":body}]}
        }});
        Arc::new(Catalog::from_documents(&snapshot, "{\"format\":1,\"kind\":\"keyword_registry\",\"keywords\":{}}", &[("random.yaml".into(),document.to_string())]).unwrap())
    }))
}

pub(crate) fn setup() -> Value {
    json!({"turn":{"active":"P1","first_player":"P1","elapsed_turns":{"P1":3,"P2":2},"phase":"main"},"players":{
        "P1":{"leader":{"class":"ニュートラル","life":20},"pp":{"current":2,"max":2},"ep":0,"sep":0,"construction":"class","zones":{
            "deck":(0_u32..8).map(|n|json!({"id":format!("d{n}"),"card":"unit"})).collect::<Vec<_>>(),
            "hand":[{"id":"s","card":"spell"}]
        }},
        "P2":{"leader":{"class":"ニュートラル","life":20},"pp":{"current":2,"max":2},"ep":0,"sep":0,"construction":"class","zones":{}}
    }})
}

pub(crate) fn cast() -> Value {
    json!({"do":"play","card":"s"})
}

pub(crate) fn resume() -> Value {
    json!({"do":"resolve-choice","choice":"execute"})
}
