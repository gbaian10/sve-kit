//! Board face identity and disclosure come from authority readers, never UI guesses.

use serde_json::{Value, json};
extern crate alloc;

use alloc::sync::Arc;
use std::sync::OnceLock;
use sve_engine::catalog::Catalog;
use sve_engine::game::Game;
use sve_engine::game::player::Viewer;
use sve_engine::game::player::observation::{
    CardObservation, Observation, ObservedZone, Relationship, RelationshipKind,
};

#[expect(
    clippy::unwrap_used,
    reason = "Invalid synthetic setup or a missing fixture card must fail the test."
)]
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

#[expect(
    clippy::unwrap_used,
    reason = "Invalid synthetic setup or a missing fixture card must fail the test."
)]
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

#[expect(
    clippy::unwrap_used,
    reason = "Invalid synthetic setup or a missing fixture card must fail the test."
)]
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
    let first_hand = card(&first_observation, Viewer::P1, ObservedZone::Hand);
    assert!(!first_hand.public);
    assert!(!first_hand.face_up);
    let own = first.observe(Viewer::P2).unwrap();
    let other_own = second.observe(Viewer::P2).unwrap();
    let second_hand = card(&own, Viewer::P2, ObservedZone::Hand);
    assert_eq!(second_hand.physical.card, "secret-a");
    assert!(!second_hand.public);
    assert_ne!(
        second_hand.physical,
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
    Game::new(
        catalog(),
        &broken_face,
        &Value::Null,
        &Value::Null,
        "unknown-face",
    )
    .and_then(|game| game.observe(Viewer::P1))
    .unwrap_err();
    let mut broken_facing = position("secret-a");
    broken_facing["players"]["P1"]["zones"]["field"][0]["face_up"] = json!("unknown");
    let broken = game(&broken_facing);
    broken.observe(Viewer::P1).unwrap_err();
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
        {"id":"copy-a","card":"base","state":{"face":1_u64}},
        {"id":"copy-b","card":"base","state":{"face":1_u64}}
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
    initial["players"]["P1"]["zones"]["field"][0]["state"]["face"] = json!(1_u64);
    initial["players"]["P1"]["zones"]["evolution"] = json!([
        {"id":"physical-evolved","card":"base","state":{"face":1_u64}}
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

#[test]
fn board_capacities_follow_movement_limits_without_inventing_other_limits() {
    let game = game(&position("secret-a"));
    let before = game.digest().unwrap();
    for viewer in [Viewer::P1, Viewer::P2] {
        let observation = game.observe(viewer).unwrap();
        for player in &observation.players {
            for zone in &player.zones {
                let expected = match zone.kind {
                    ObservedZone::Field | ObservedZone::Ex => Some(5),
                    ObservedZone::Deck
                    | ObservedZone::EvolveDeck
                    | ObservedZone::Hand
                    | ObservedZone::Cemetery
                    | ObservedZone::Banish
                    | ObservedZone::Resolution
                    | ObservedZone::Evolution
                    | ObservedZone::Race
                    | ObservedZone::Drive
                    | ObservedZone::Trigger
                    | ObservedZone::Equipment => None,
                };
                assert_eq!(zone.capacity, expected);
            }
        }
        assert!(observation.attack.is_none());
    }
    assert_eq!(game.digest().unwrap(), before);
}

#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "A malformed synthetic save must fail the fixture."
)]
fn attacking_game(target: &str) -> Value {
    let original = game(&position("secret-a"));
    let mut saved = serde_json::to_value(&original).unwrap();
    let generation = saved["state"]["objects"]["physical-base"]["generation"].clone();
    saved["state"]["flow"] = json!({
        "kind":"battle", "attacker":"physical-base", "attacker_generation":generation,
        "target":target, "target_generation":generation
    });
    saved
}

#[test]
fn board_attack_rejects_missing_required_participants_and_generations() {
    for key in ["attacker", "attacker_generation", "target"] {
        let mut saved = attacking_game("P2.leader");
        saved["state"]["flow"].as_object_mut().unwrap().remove(key);
        let damaged: Game = serde_json::from_value(saved).unwrap();
        for viewer in [Viewer::P1, Viewer::P2] {
            assert_eq!(
                damaged.observe(viewer).unwrap_err().to_string(),
                "invalid: player observation unavailable"
            );
        }
    }
    for target in ["missing-physical-id", "unknown.leader"] {
        let damaged: Game = serde_json::from_value(attacking_game(target)).unwrap();
        damaged.observe(Viewer::P1).unwrap_err();
    }
    let mut saved = attacking_game("physical-base");
    saved["state"]["flow"]["target_generation"] = Value::Null;
    let damaged: Game = serde_json::from_value(saved).unwrap();
    damaged.observe(Viewer::P1).unwrap_err();
}

#[test]
fn board_attack_never_relinks_to_a_new_generation_of_either_participant() {
    for key in ["attacker_generation", "target_generation"] {
        let mut saved = attacking_game("physical-base");
        let current = saved["state"]["flow"][key].as_u64().unwrap();
        saved["state"]["flow"][key] = json!(current + 1);
        let stale: Game = serde_json::from_value(saved).unwrap();
        for viewer in [Viewer::P1, Viewer::P2] {
            assert!(stale.observe(viewer).unwrap().attack.is_none());
        }
    }
}

#[test]
fn board_attack_requires_field_zones_even_when_the_frozen_generations_still_match() {
    for attacker_left in [true, false] {
        let mut saved = attacking_game(if attacker_left {
            "P2.leader"
        } else {
            "own-hand"
        });
        let generation = saved["state"]["objects"]["own-hand"]["generation"].clone();
        if attacker_left {
            saved["state"]["flow"]["attacker"] = json!("own-hand");
            saved["state"]["flow"]["attacker_generation"] = generation;
        } else {
            saved["state"]["flow"]["target_generation"] = generation;
        }
        let outside_field: Game = serde_json::from_value(saved).unwrap();
        let before = outside_field.digest().unwrap();
        for viewer in [Viewer::P1, Viewer::P2] {
            assert!(outside_field.observe(viewer).unwrap().attack.is_none());
        }
        assert_eq!(outside_field.digest().unwrap(), before);
    }
}

#[test]
fn board_attack_requires_current_viewer_authority_for_each_card_endpoint() {
    let mut saved = attacking_game("physical-base");
    saved["state"]["knowledge"]["P2"]["located"]
        .as_array_mut()
        .unwrap()
        .retain(|id| id != "physical-base");
    let damaged: Game = serde_json::from_value(saved).unwrap();
    let before = damaged.digest().unwrap();
    assert!(damaged.observe(Viewer::P1).unwrap().attack.is_some());
    damaged.observe(Viewer::P2).unwrap_err();
    assert_eq!(damaged.digest().unwrap(), before);
}

#[test]
fn board_relationships_require_both_current_viewer_endpoints() {
    let original = game(&position("secret-a"));
    for hidden in ["physical-base", "physical-evolved"] {
        let mut saved = serde_json::to_value(&original).unwrap();
        saved["state"]["knowledge"]["P2"]["located"]
            .as_array_mut()
            .unwrap()
            .retain(|id| id != hidden);
        let changed: Game = serde_json::from_value(saved).unwrap();
        assert_eq!(
            original.observe(Viewer::P1).unwrap(),
            changed.observe(Viewer::P1).unwrap()
        );
        assert_eq!(
            changed.observe(Viewer::P2).unwrap().relationships,
            Vec::<Relationship>::new()
        );
    }
    let observed = original.observe(Viewer::P1).unwrap();
    assert_eq!(observed.relationships.len(), 1);
    let relationship = &observed.relationships[0];
    assert_eq!(relationship.kind, RelationshipKind::Evolution);
    assert_eq!(
        relationship.from,
        card(&observed, Viewer::P1, ObservedZone::Evolution).handle
    );
    assert_eq!(
        relationship.to,
        card(&observed, Viewer::P1, ObservedZone::Field).handle
    );
}

#[test]
fn board_relationships_filter_hidden_categories_before_dispatch() {
    let mut initial = position("secret-a");
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({});
    initial["players"]["P1"]["zones"]["evolution"] = json!([]);
    let baseline = game(&initial).observe(Viewer::P1).unwrap();
    for kind in ["hidden-unknown-kind", "different-hidden-kind"] {
        initial["players"]["P1"]["zones"]["field"][0]["state"]["links"] =
            json!({kind:["opponent-secret"]});
        assert_eq!(game(&initial).observe(Viewer::P1).unwrap(), baseline);
    }
    initial["players"]["P1"]["zones"]["field"][0]["state"]["links"] =
        json!({"visible-unknown-kind":["own-hand"]});
    assert_eq!(
        game(&initial).observe(Viewer::P1).unwrap_err().to_string(),
        "invalid: player observation unavailable"
    );
}

#[test]
fn board_relationships_sort_visible_handles_and_preserve_distinct_copies() {
    let mut observations = Vec::new();
    for (base_ids, form_ids) in [
        (["z-base", "a-base"], ["z-form", "a-form"]),
        (["a-base", "z-base"], ["a-form", "z-form"]),
    ] {
        let mut initial = position("secret-a");
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":base_ids[0],"card":"base","state":{"evolved_with":form_ids[0]}},
            {"id":base_ids[1],"card":"base","state":{"evolved_with":form_ids[1]}}
        ]);
        initial["players"]["P1"]["zones"]["evolution"] = json!([
            {"id":form_ids[0],"card":"evolved"}, {"id":form_ids[1],"card":"evolved"}
        ]);
        let game = game(&initial);
        let before = game.digest().unwrap();
        let observed = [
            game.observe(Viewer::P1).unwrap(),
            game.observe(Viewer::P2).unwrap(),
        ];
        for viewer in &observed {
            let relationships = &viewer.relationships;
            assert_eq!(relationships.len(), 2);
            assert!(relationships[0] < relationships[1]);
            assert_ne!(relationships[0].from, relationships[1].from);
            assert_ne!(relationships[0].to, relationships[1].to);
            let text = format!("{relationships:?}");
            for id in ["z-base", "a-base", "z-form", "a-form"] {
                assert!(!text.contains(id));
            }
        }
        assert_eq!(game.digest().unwrap(), before);
        observations.push(observed);
    }
    assert_eq!(observations[0], observations[1]);
}

#[test]
fn board_relationships_require_resource_association_authority_beyond_stored_ids() {
    for (kind, zone) in [("出走", "race"), ("憑依", "drive")] {
        let mut initial = position("secret-a");
        initial["players"]["P1"]["zones"]["field"][0]["state"] =
            json!({"links":{kind:["unproven-resource"]}});
        initial["players"]["P1"]["zones"]["evolution"] = json!([]);
        initial["players"]["P1"]["zones"][zone] =
            json!([{"id":"unproven-resource","card":"evolved"}]);
        let game = game(&initial);
        for viewer in [Viewer::P1, Viewer::P2] {
            let observed = game.observe(viewer).unwrap();
            assert_eq!(observed.relationships, Vec::<Relationship>::new());
            assert_ne!(
                card(&observed, Viewer::P1, ObservedZone::Field)
                    .handle
                    .as_str(),
                ""
            );
            assert_eq!(
                observed.players[0]
                    .zones
                    .iter()
                    .find(|entry| entry.kind
                        == if zone == "race" {
                            ObservedZone::Race
                        } else {
                            ObservedZone::Drive
                        })
                    .unwrap()
                    .cards
                    .len(),
                1
            );
        }
    }
}
