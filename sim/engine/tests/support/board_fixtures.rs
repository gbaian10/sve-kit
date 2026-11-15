//! Synthetic board facts shared by private unit and authority integration tests.
//! Lib unit tests also include this file via `#[path]`; use APIs visible to both test contexts.

use alloc::sync::Arc;
use serde_json::{Value, json};
use sve_engine::Result;
use sve_engine::catalog::Catalog;
use sve_engine::game::Game;
use sve_engine::game::player::{ObjectHandle, Viewer, observation as native};

pub(crate) fn setup(seat: u32, deck: Value) -> Value {
    let own = Value::Object(
        [
            ("deck".into(), deck),
            ("hand".into(), json!([{"id":"s","card":"unit-spell"}])),
        ]
        .into_iter()
        .collect(),
    );
    let other = json!({"deck":[],"field":[{"id":"f","card":"unit-follower"}]});
    let [first, second] = if seat == 1 {
        [own, other]
    } else {
        [other, own]
    };
    json!({"history":"explicit","turn":{"active":"P1","first_player":"P1","phase":"main","elapsed_turns":{"P1":3_i32,"P2":2_i32}},"players":{
        "P1":{"leader":{"class":"ニュートラル","life":20_i32},"pp":{"current":2_i32,"max":2_i32},"ep":0_i32,"sep":0_i32,"construction":"class","zones":first},
        "P2":{"leader":{"class":"ニュートラル","life":20_i32},"pp":{"current":2_i32,"max":2_i32},"ep":0_i32,"sep":0_i32,"construction":"class","zones":second}
    }})
}
pub(crate) fn catalog_documents(cost: &Value) -> (String, String, String) {
    let snapshot = [("unit-spell", "スペル"), ("unit-follower", "フォロワー"), ("unit-amulet", "アミュレット")].iter().map(|(number, kind)| {
        json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,"cost":cost,"power":if *kind=="アミュレット" {"-"}else{"2"},"hp":if *kind=="アミュレット" {"-"}else{"3"},"traits":[],"text":null,"sections":[]}]}).to_string()
    }).collect::<Vec<_>>().join("\n");
    let registry = json!({"format":1_i32,"kind":"keyword_registry","keywords":{}}).to_string();
    let document = json!({"format":1_i32,"kind":"effect_set","cards":{
        "unit-spell":{"status":"complete","review":"synthetic","abilities":[]},
        "unit-follower":{"status":"complete","review":"synthetic","abilities":[]},
        "unit-amulet":{"status":"complete","review":"synthetic","abilities":[]}
    }})
    .to_string();
    (snapshot, registry, document)
}

pub(crate) fn catalog() -> Result<Arc<Catalog>> {
    catalog_with_cost(&json!("1"))
}

pub(crate) fn catalog_with_cost(cost: &Value) -> Result<Arc<Catalog>> {
    let (snapshot, registry, document) = catalog_documents(cost);
    Catalog::from_documents(&snapshot, &registry, &[("unit.yaml".into(), document)]).map(Arc::new)
}

pub(crate) fn exact_authority(game: &Game) -> String {
    format!("{game:?}")
}

pub(crate) fn object(
    observation: &native::Observation,
    seat: Viewer,
    zone: native::ObservedZone,
) -> Option<ObjectHandle> {
    let player = observation
        .players
        .iter()
        .find(|player| player.player == seat)?;
    let zone = player.zones.iter().find(|entry| entry.kind == zone)?;
    Some(zone.cards.first()?.card.handle.clone())
}
