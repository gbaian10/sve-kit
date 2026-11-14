//! Board face identity and disclosure come from authority readers, never UI guesses.

use serde_json::{Value, json};
extern crate alloc;

use alloc::sync::Arc;
use std::sync::OnceLock;
use sve_engine::catalog::Catalog;
use sve_engine::game::Game;
use sve_engine::game::player::Viewer;
use sve_engine::game::player::observation::{CardObservation, Observation, ObservedZone};

fn catalog() -> Arc<Catalog> {
    static CATALOG: OnceLock<Arc<Catalog>> = OnceLock::new();
    Arc::clone(CATALOG.get_or_init(|| {
        let snapshot = ["base", "evolved", "secret-a", "secret-b"]
            .into_iter()
            .map(|number| {
                let face = json!({
                    "name":number,"card_class":"ニュートラル","card_type":"フォロワー",
                    "cost":"1","power":"2","hp":"3","traits":[],"text":null,"sections":[]
                });
                let faces = if number == "base" {
                    vec![face.clone(), face]
                } else {
                    vec![face]
                };
                json!({"number":number,"faces":faces}).to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
        let cards = ["base", "evolved", "secret-a", "secret-b"]
            .into_iter()
            .map(|number| {
                (
                    number.to_owned(),
                    json!({"status":"complete","review":"synthetic","abilities":[]}),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        let document = json!({"format":1_u8,"kind":"effect_set","cards":cards});
        Arc::new(
            Catalog::from_documents(
                &snapshot,
                "{\"format\":1,\"kind\":\"keyword_registry\",\"keywords\":{}}",
                &[("synthetic.yaml".into(), document.to_string())],
            )
            .unwrap(),
        )
    }))
}

fn position(secret: &str) -> Value {
    json!({
        "turn":{"active":"P1","first_player":"P1","elapsed_turns":{"P1":3,"P2":2},"phase":"main"},
        "players":{
            "P1":{"leader":{"class":"ニュートラル","life":20},"pp":{"current":2,"max":2},"ep":0,"sep":0,"zones":{
                "field":[{"id":"physical-base","card":"base","state":{"evolved_with":"physical-evolved"}}],
                "evolution":[{"id":"physical-evolved","card":"evolved"}],
                "hand":[{"id":"own-hand","card":"base"}]
            }},
            "P2":{"leader":{"class":"ニュートラル","life":20},"pp":{"current":2,"max":2},"ep":0,"sep":0,"zones":{
                "hand":[{"id":"opponent-secret","card":secret}],
                "evolve_deck":[{"id":"face-down-evolve","card":"evolved"}]
            }}
        }
    })
}

fn game(position: &Value) -> Game {
    Game::new(
        catalog(),
        position,
        &Value::Null,
        &Value::Null,
        "board-faces",
    )
    .unwrap()
}

fn card(observation: &Observation, player: Viewer, zone: ObservedZone) -> &CardObservation {
    &observation
        .players
        .iter()
        .find(|view| view.player == player)
        .unwrap()
        .zones
        .iter()
        .find(|view| view.kind == zone)
        .unwrap()
        .cards
        .first()
        .unwrap()
        .card
}

#[test]
fn physical_printing_stays_separate_from_evolved_characteristics() {
    let game = game(&position("secret-a"));
    for viewer in [Viewer::P1, Viewer::P2] {
        let observed = game.observe(viewer).unwrap();
        let base = card(&observed, Viewer::P1, ObservedZone::Field);
        assert_eq!(base.physical.card, "base");
        assert_eq!(base.physical.face, 0);
        assert_eq!(base.properties.card, "evolved");
        assert_eq!(base.properties.face, 0);
        assert!(base.public);
        assert!(base.face_up);
        let evolution = card(&observed, Viewer::P1, ObservedZone::Evolution);
        assert_eq!(evolution.physical.card, "evolved");
        assert_ne!(base.handle, evolution.handle);
    }
}

#[test]
fn private_faces_are_known_only_to_their_viewer_and_queries_stay_read_only() {
    let first = game(&position("secret-a"));
    let second = game(&position("secret-b"));
    let before = first.digest().unwrap();
    let first_observation = first.observe(Viewer::P1).unwrap();
    assert_eq!(first_observation, second.observe(Viewer::P1).unwrap());
    assert_eq!(first_observation, first.observe(Viewer::P1).unwrap());
    assert_eq!(first.digest().unwrap(), before);
    let hand = card(&first_observation, Viewer::P1, ObservedZone::Hand);
    assert!(!hand.public);
    assert!(!hand.face_up);
    let own = first.observe(Viewer::P2).unwrap();
    let other_own = second.observe(Viewer::P2).unwrap();
    let hand = card(&own, Viewer::P2, ObservedZone::Hand);
    assert_eq!(hand.physical.card, "secret-a");
    assert!(!hand.public);
    assert_ne!(
        hand.physical,
        card(&other_own, Viewer::P2, ObservedZone::Hand).physical
    );
    let evolve = card(&own, Viewer::P2, ObservedZone::EvolveDeck);
    assert!(!evolve.public);
    assert!(!evolve.face_up);
}

#[test]
fn damaged_required_face_or_facing_stops_observation() {
    let mut broken_face = position("secret-a");
    broken_face["players"]["P1"]["zones"]["evolution"][0]["state"]["face"] = json!("unknown");
    assert!(
        Game::new(
            catalog(),
            &broken_face,
            &Value::Null,
            &Value::Null,
            "unknown-face"
        )
        .and_then(|game| game.observe(Viewer::P1))
        .is_err()
    );
    let mut broken_facing = position("secret-a");
    broken_facing["players"]["P1"]["zones"]["field"][0]["face_up"] = json!("unknown");
    let broken = game(&broken_facing);
    assert!(broken.observe(Viewer::P1).is_err());
}

#[test]
fn another_seats_private_knowledge_does_not_change_disclosure_metadata() {
    let original = game(&position("secret-a"));
    let mut saved = serde_json::to_value(&original).unwrap();
    let own_handle = saved["state"]["knowledge"]["P1"]["handles"]["own-hand"].clone();
    saved["state"]["knowledge"]["P2"]["located"]
        .as_array_mut()
        .unwrap()
        .push(json!("own-hand"));
    saved["state"]["knowledge"]["P2"]["handles"]["own-hand"] = own_handle;
    let changed: Game = serde_json::from_value(saved).unwrap();
    assert!(
        original.observe(Viewer::P1).unwrap() == changed.observe(Viewer::P1).unwrap(),
        "opponent knowledge must not change authorized metadata"
    );
}

#[test]
fn face_memory_keeps_observed_ordinals_and_deduplicates_physical_copies() {
    let mut initial = position("secret-a");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"copy-a","card":"base","state":{"face":1}},
        {"id":"copy-b","card":"base","state":{"face":1}}
    ]);
    let observed = game(&initial).observe(Viewer::P1).unwrap();
    let faces = observed
        .remembered
        .iter()
        .filter(|memory| memory.card == "base" && memory.owner == Viewer::P1)
        .map(|memory| memory.face)
        .collect::<Vec<_>>();
    assert_eq!(faces, vec![0, 1]);
    assert_eq!(
        card(&observed, Viewer::P1, ObservedZone::Hand)
            .physical
            .face,
        1
    );
}

#[test]
fn an_evolved_face_index_does_not_relabel_the_physical_base() {
    let mut initial = position("secret-a");
    initial["players"]["P1"]["zones"]["field"][0]["state"]["face"] = json!(1);
    initial["players"]["P1"]["zones"]["evolution"] = json!([
        {"id":"physical-evolved","card":"base","state":{"face":1}}
    ]);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    let game = game(&initial);
    for viewer in [Viewer::P1, Viewer::P2] {
        let observed = game.observe(viewer).unwrap();
        let base = card(&observed, Viewer::P1, ObservedZone::Field);
        assert_eq!(base.physical.face, 0);
        assert_eq!(base.properties.face, 1);
        let form = card(&observed, Viewer::P1, ObservedZone::Evolution);
        assert_eq!(form.physical.face, 1);
        assert_ne!(base.handle, form.handle);
        let remembered = observed
            .remembered
            .iter()
            .filter(|memory| memory.card == "base" && memory.owner == Viewer::P1)
            .map(|memory| memory.face)
            .collect::<Vec<_>>();
        assert_eq!(remembered, vec![0, 1]);
    }
}

#[test]
fn evolve_deck_missing_facing_defaults_to_private_and_face_down() {
    let original = game(&position("secret-a"));
    let mut saved = serde_json::to_value(&original).unwrap();
    saved["state"]["objects"]["face-down-evolve"]["state"]
        .as_object_mut()
        .unwrap()
        .remove("face_up");
    let restored: Game = serde_json::from_value(saved).unwrap();
    let observation = restored.observe(Viewer::P2).unwrap();
    let form = card(&observation, Viewer::P2, ObservedZone::EvolveDeck);
    assert!(!form.face_up);
    assert!(!form.public);
}
