//! Synthetic vectors pin named random continuation and reject unversioned state.

extern crate alloc;

use serde_json::{Value, json};
use sve_engine::game::{Game, View};

#[path = "support/random_fixtures.rs"]
mod random_fixtures;

#[expect(
    clippy::unwrap_used,
    reason = "Invalid synthetic setup must fail the test."
)]
fn game() -> Game {
    Game::new(
        random_fixtures::catalog(),
        &random_fixtures::setup(),
        &Value::Null,
        &Value::Null,
        "new-vector",
    )
    .unwrap()
}

#[test]
fn games_use_named_stream_and_restore_pending_draws() {
    let mut original = game();
    assert_eq!(original.digest().unwrap(), game().digest().unwrap());
    let encoded = serde_json::to_value(&original).unwrap();
    assert_eq!(
        encoded["state"]["rng"]["algorithm"],
        "chacha12-sha256-rand09-v1"
    );
    let step = original.decide(&random_fixtures::cast(), "new").unwrap();
    assert_eq!(step.outcome, "paused");
    let mut restored: Game =
        serde_json::from_str(&serde_json::to_string(&original).unwrap()).unwrap();
    for view in [View::P1, View::P2] {
        let packet = restored.projection(view).unwrap().to_string();
        for secret in ["\"rng\"", "\"seed\"", "word_pos", "chacha12-sha256"] {
            assert!(!packet.contains(secret));
        }
    }
    let first = original
        .decide(&random_fixtures::resume(), "resume")
        .unwrap();
    let second = restored
        .decide(&random_fixtures::resume(), "resume")
        .unwrap();
    assert_eq!(first.events, second.events);
    assert_eq!(original.digest().unwrap(), restored.digest().unwrap());
}

#[test]
fn game_reader_rejects_unversioned_rng() {
    let initial = game();
    let mut rng = serde_json::to_value(&initial).unwrap()["state"]["rng"].clone();
    rng.as_object_mut().unwrap().remove("algorithm");
    for (bad, message) in [
        (rng, "missing field `algorithm`"),
        (
            json!(0_u64),
            "invalid type: integer `0`, expected adjacently tagged enum RandomWire",
        ),
    ] {
        let mut encoded = serde_json::to_value(&initial).unwrap();
        encoded["state"]["rng"] = bad;
        assert_eq!(
            serde_json::from_value::<Game>(encoded)
                .unwrap_err()
                .to_string(),
            message
        );
    }
}

#[test]
fn scripts_override_without_consuming_rng() {
    let script = json!({"dice":[2_i64,5_i64],"random_selections":[{"n":1_i64,"from":"P1.deck","result":["d1","d4"]}],"shuffles":[{"player":"P1","result":["d7","d6","d5","d4","d3","d2","d1","d0"]}]});
    let mut initial = Game::new(
        random_fixtures::catalog(),
        &random_fixtures::setup(),
        &Value::Null,
        &script,
        "scripted",
    )
    .unwrap();
    let before = serde_json::to_value(&initial).unwrap()["state"]["rng"].clone();
    assert_eq!(
        initial
            .decide(&random_fixtures::cast(), "cast")
            .unwrap()
            .outcome,
        "paused"
    );
    let mut restored: Game =
        serde_json::from_value(serde_json::to_value(initial).unwrap()).unwrap();
    assert_eq!(
        restored
            .decide(&random_fixtures::resume(), "resume")
            .unwrap()
            .outcome,
        "resolved"
    );
    assert_eq!(
        serde_json::to_value(&restored).unwrap()["state"]["rng"],
        before
    );
    assert_eq!(
        restored.query(View::Referee, "P2.leader.life").unwrap(),
        Some(json!(13_i64))
    );
}

#[test]
fn failed_resolution_rolls_back_consumed_randomness() {
    let mut initial = Game::new(
        random_fixtures::catalog(),
        &random_fixtures::setup(),
        &Value::Null,
        &json!({"dice":[]}),
        "rollback",
    )
    .unwrap();
    let before = initial.digest().unwrap();
    let bytes = serde_json::to_value(&initial).unwrap();
    assert!(
        initial
            .decide(&random_fixtures::cast(), "failed")
            .unwrap_err()
            .to_string()
            .contains("missing or exhausted dice script")
    );
    assert_eq!(initial.digest().unwrap(), before);
    assert_eq!(serde_json::to_value(initial).unwrap(), bytes);
}
