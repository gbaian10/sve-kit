//! Hand-written fixtures decode, re-encode to the same JSON, and lose their meaning
//! as soon as one defined field is missing or one unknown field appears.

#![expect(
    clippy::unwrap_used,
    reason = "A missing or malformed fixture must fail the test."
)]

#[path = "support/fixtures.rs"]
mod support;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use support::{damaged, fixtures};
use sve_battle_contract::observation::BattleObservation;
use sve_battle_contract::{HostError, HostMessage, decode, encode};

fn check<T>(group: &str)
where
    T: DeserializeOwned + Serialize,
{
    for (name, text) in fixtures(group) {
        let message: T = decode(&text)
            .map_err(|error| format!("{name}: {error}"))
            .unwrap();
        let original: Value = serde_json::from_str(&text).unwrap();
        let encoded: Value = serde_json::from_str(&encode(&message).unwrap()).unwrap();
        assert_eq!(encoded, original, "{name}");
        for (damage, copy) in damaged(&original) {
            assert_eq!(
                decode::<T>(&copy.to_string()).err(),
                Some(HostError::FORMAT),
                "{name}: {damage}"
            );
        }
    }
}

#[test]
fn host_messages_round_trip() {
    check::<HostMessage>("host");
}

#[test]
fn observations_round_trip() {
    check::<BattleObservation>("observation");
}

#[test]
fn another_wire_format_is_a_format_error() {
    for format in ["0", "2", "\"1\"", "null"] {
        let text = format!(r#"{{"kind":"host_error","format":{format},"reason":"range"}}"#);
        assert_eq!(
            decode::<HostMessage>(&text),
            Err(HostError::FORMAT),
            "{format}"
        );
    }
}

#[test]
fn unknown_tags_and_values_are_a_format_error() {
    for text in [
        r#"{"kind":"resync","format":1}"#,
        r#"{"kind":"host_error","format":1,"reason":"unknown"}"#,
        r#"{"format":1,"reason":"range"}"#,
        "[]",
    ] {
        assert_eq!(
            decode::<HostMessage>(text),
            Err(HostError::FORMAT),
            "{text}"
        );
    }
}

#[test]
fn a_number_outside_the_safe_range_inside_a_message_is_a_range_error() {
    let (_, baseline) = fixtures("observation").swap_remove(0);
    let unsafe_count = baseline.replacen("\"count\": 50", "\"count\": 9007199254740992", 1);
    assert_ne!(unsafe_count, baseline);
    assert_eq!(
        decode::<BattleObservation>(&unsafe_count).err(),
        Some(HostError::RANGE)
    );
}

#[test]
fn a_repeated_key_is_refused_wherever_it_hides_a_value() {
    // Each text would read as a valid message if only the last value of a key were kept.
    for text in [
        r#"{"kind":"host_error","format":9007199254740991.4,"format":1,"reason":"range"}"#,
        r#"{"kind":"host_error","format":1,"format":1,"reason":"range"}"#,
        r#"{"kind":"resync","kind":"host_error","format":1,"reason":"range"}"#,
        r#"{"kind":"host_error","format":1,"reason":"range","reason":"range"}"#,
    ] {
        assert_eq!(
            decode::<HostMessage>(text),
            Err(HostError::FORMAT),
            "{text}"
        );
    }

    let (_, baseline) = fixtures("observation").swap_remove(0);
    for (once, twice) in [
        // Nested object: the first, refused count would be overwritten.
        (
            "\"count\": 50,",
            "\"count\": 9007199254740992, \"count\": 50,",
        ),
        // Object table: a second entry under the same handle would replace the first.
        (
            "\"objects\": {}",
            "\"objects\": {\"o1\": null, \"o1\": null}",
        ),
        (
            "\"viewer\": \"p1\",",
            "\"viewer\": \"p2\", \"viewer\": \"p1\",",
        ),
    ] {
        let repeated = baseline.replacen(once, twice, 1);
        assert_ne!(repeated, baseline, "{once}");
        assert_eq!(
            decode::<BattleObservation>(&repeated).err(),
            Some(HostError::FORMAT),
            "{twice}"
        );
    }
}

#[test]
fn text_after_the_message_is_a_format_error() {
    let text = r#"{"kind":"host_error","format":1,"reason":"range"} []"#;
    assert_eq!(decode::<HostMessage>(text), Err(HostError::FORMAT));
}
