//! Synthetic vectors pin legacy replays and versioned random continuation.

extern crate alloc;

use alloc::sync::Arc;

use serde_json::{Value, json};
use sve_engine::game::{Game, View};
use sve_engine::random::RandomAlgorithm;
use sve_engine::replay::Replay;

#[path = "support/random_fixtures.rs"]
mod random_fixtures;

#[expect(
    clippy::unwrap_used,
    reason = "Invalid synthetic setup must fail the test."
)]
fn game(algorithm: RandomAlgorithm) -> Game {
    Game::new_with_random_algorithm(
        random_fixtures::catalog(),
        &random_fixtures::setup(),
        &Value::Null,
        &Value::Null,
        "legacy-vector",
        algorithm,
    )
    .unwrap()
}

#[test]
fn original_numeric_replay_restores_and_continues_exactly() {
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/random/legacy-expected.json")).unwrap();
    let initial = game(RandomAlgorithm::Legacy);
    assert_eq!(initial.digest().unwrap(), expected["initial_digest"]);
    let (mut fresh, root) = Replay::start(initial, "legacy");
    let (pending, first) = fresh.decide(&root, &random_fixtures::cast()).unwrap();
    assert_eq!(serde_json::to_value(first).unwrap(), expected["first"]);
    let (mut restored, selected) =
        Replay::restore(include_bytes!("fixtures/random/legacy-save.json")).unwrap();
    assert_eq!(pending, selected);
    assert_eq!(
        fresh.game(&pending).unwrap().digest().unwrap(),
        restored.game(&selected).unwrap().digest().unwrap()
    );
    assert_eq!(
        fresh.save(&pending).unwrap(),
        include_str!("fixtures/random/legacy-save.json")
            .trim_end()
            .as_bytes()
    );
    let (done, second) = restored
        .decide(&selected, &random_fixtures::resume())
        .unwrap();
    assert_eq!(serde_json::to_value(second).unwrap(), expected["second"]);
    assert_eq!(
        restored.game(&done).unwrap().digest().unwrap(),
        expected["final_digest"]
    );
    assert_eq!(
        restored
            .game(&done)
            .unwrap()
            .projection(View::Referee)
            .unwrap(),
        expected["final_state"]
    );
}

#[test]
fn default_games_use_named_stream_and_restore_pending_draws() {
    let initial = Game::new(
        random_fixtures::catalog(),
        &random_fixtures::setup(),
        &Value::Null,
        &Value::Null,
        "legacy-vector",
    )
    .unwrap();
    assert_eq!(
        initial.digest().unwrap(),
        game(RandomAlgorithm::ChaCha12V1).digest().unwrap()
    );
    assert_ne!(
        initial.digest().unwrap(),
        game(RandomAlgorithm::Legacy).digest().unwrap()
    );
    let encoded = serde_json::to_value(&initial).unwrap();
    assert_eq!(
        encoded["state"]["rng"]["algorithm"],
        "chacha12-sha256-rand09-v1"
    );
    let (mut original, root) = Replay::start(initial, "new");
    let (pending, step) = original.decide(&root, &random_fixtures::cast()).unwrap();
    assert_eq!(step.outcome, "paused");
    let (mut restored, selected) = Replay::restore(&original.save(&pending).unwrap()).unwrap();
    for view in [View::P1, View::P2] {
        let packet = restored
            .game(&selected)
            .unwrap()
            .projection(view)
            .unwrap()
            .to_string();
        for secret in ["\"rng\"", "\"seed\"", "word_pos", "chacha12-sha256"] {
            assert!(!packet.contains(secret));
        }
    }
    let (a, first) = original
        .decide(&pending, &random_fixtures::resume())
        .unwrap();
    let (b, second) = restored
        .decide(&selected, &random_fixtures::resume())
        .unwrap();
    assert_eq!(first.events, second.events);
    assert_eq!(
        original.game(&a).unwrap().digest().unwrap(),
        restored.game(&b).unwrap().digest().unwrap()
    );
    let branch = restored.branch(&root, false).unwrap();
    let (branched, repeated) = restored.decide(&branch, &random_fixtures::cast()).unwrap();
    assert_eq!(repeated.outcome, "paused");
    assert_eq!(
        serde_json::to_value(restored.game(&branched).unwrap()).unwrap()["state"]["rng"],
        serde_json::to_value(original.game(&pending).unwrap()).unwrap()["state"]["rng"]
    );
}

#[test]
fn scripts_override_each_algorithm_without_consuming_rng() {
    for algorithm in [RandomAlgorithm::Legacy, RandomAlgorithm::ChaCha12V1] {
        let script = json!({"dice":[2_i64,5_i64],"random_selections":[{"n":1_i64,"from":"P1.deck","result":["d1","d4"]}],"shuffles":[{"player":"P1","result":["d7","d6","d5","d4","d3","d2","d1","d0"]}]});
        let mut initial = Game::new_with_random_algorithm(
            random_fixtures::catalog(),
            &random_fixtures::setup(),
            &Value::Null,
            &script,
            "scripted",
            algorithm,
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
}

#[test]
fn failed_resolution_rolls_back_consumed_randomness() {
    for algorithm in [RandomAlgorithm::Legacy, RandomAlgorithm::ChaCha12V1] {
        let mut initial = Game::new_with_random_algorithm(
            random_fixtures::catalog(),
            &random_fixtures::setup(),
            &Value::Null,
            &json!({"dice":[]}),
            "rollback",
            algorithm,
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
}

#[test]
fn observation_algorithm_selection_remains_independent_of_server_rng() {
    let initial = game(RandomAlgorithm::Legacy);
    let mut packet = initial.projection(View::P1).unwrap();
    packet["P1"]["deck_list"] =
        json!([{"card":"unit","count":8_i64},{"card":"spell","count":1_i64}]);
    for algorithm in [RandomAlgorithm::Legacy, RandomAlgorithm::ChaCha12V1] {
        let catalog = random_fixtures::catalog();
        let first = Game::from_observation_with_random_algorithm(
            Arc::clone(&catalog),
            &packet,
            "P1",
            "world",
            algorithm,
        )
        .unwrap();
        let second = Game::from_observation_with_random_algorithm(
            catalog, &packet, "P1", "world", algorithm,
        )
        .unwrap();
        assert_eq!(first.digest().unwrap(), second.digest().unwrap());
        assert_eq!(
            serde_json::to_value(&first).unwrap()["state"]["rng"].is_number(),
            algorithm == RandomAlgorithm::Legacy
        );
    }
    let sampled =
        Game::from_observation(random_fixtures::catalog(), &packet, "P1", "world").unwrap();
    assert!(serde_json::to_value(sampled).unwrap()["state"]["rng"].is_object());
}
