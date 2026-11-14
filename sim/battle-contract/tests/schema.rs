//! The generated schema accepts what the contract states and refuses anything less exact.

#![expect(
    clippy::unwrap_used,
    reason = "A schema that cannot be built must fail the test."
)]

#[path = "support/fixtures.rs"]
mod support;

use jsonschema::Validator;
use serde_json::{Value, json};
use sve_battle_contract::{Int, MAX_SAFE_INTEGER, schema};

/// A validator for one named definition of the contract.
fn validator(definition: &str) -> Validator {
    let document = serde_json::to_value(schema()).unwrap();
    let rooted = json!({
        "$schema": document.get("$schema").unwrap(),
        "$ref": format!("#/$defs/{definition}"),
        "$defs": document.get("$defs").unwrap(),
    });
    jsonschema::validator_for(&rooted).unwrap()
}

#[test]
fn the_schema_accepts_each_fixture_and_refuses_each_damaged_copy() {
    for (group, definition) in [
        ("host", "HostMessage"),
        ("observation", "BattleObservation"),
    ] {
        let validator = validator(definition);
        for (name, text) in support::fixtures(group) {
            let original: Value = serde_json::from_str(&text).unwrap();
            assert!(validator.is_valid(&original), "{name}");
            for (damage, copy) in support::damaged(&original) {
                assert!(!validator.is_valid(&copy), "{name}: {damage}");
            }
        }
    }
}

#[test]
fn the_schema_refuses_numbers_outside_the_safe_range_and_another_format() {
    let numbers = validator("Nat");
    assert!(numbers.is_valid(&json!(MAX_SAFE_INTEGER)));
    assert!(!numbers.is_valid(&json!(2_u64.pow(53))));
    assert!(!numbers.is_valid(&json!(-1_i8)));
    assert!(!numbers.is_valid(&json!(0.5_f64)));
    let signed = validator("Int");
    assert!(signed.is_valid(&json!(Int::MIN.get())));
    assert!(signed.is_valid(&json!(Int::MAX.get())));
    assert!(!signed.is_valid(&json!((-2_i64).pow(53))));

    let host = validator("HostMessage");
    assert!(host.is_valid(&json!({"kind": "host_error", "format": 1_u8, "reason": "range"})));
    assert!(!host.is_valid(&json!({"kind": "host_error", "format": 2_u8, "reason": "range"})));
    assert!(!host.is_valid(&json!({"kind": "resync", "format": 1_u8, "reason": "range"})));
}

#[test]
fn generation_is_reproducible() {
    let first = serde_json::to_string(&schema()).unwrap();
    assert_eq!(serde_json::to_string(&schema()).unwrap(), first);
}
