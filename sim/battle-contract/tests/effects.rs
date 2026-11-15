//! Frozen damage replacements keep both operands across codec and schema boundaries.

use serde_json::{Value, json};
use sve_battle_contract::observation::PreventionChange;
use sve_battle_contract::{HostError, MAX_SAFE_INTEGER, Nat, decode, encode, schema};

#[test]
fn prevention_cap_round_trips_as_a_distinct_operation() {
    for (threshold, replacement) in [
        (Nat::from(6_u32), Nat::from(5_u32)),
        (Nat::ZERO, Nat::MAX),
        (Nat::MAX, Nat::ZERO),
    ] {
        let cap = PreventionChange::Cap {
            threshold,
            replacement,
        };
        let encoded = encode(&cap).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&encoded).unwrap(),
            json!({
                "kind":"cap", "threshold":threshold.get(), "replacement":replacement.get(),
            })
        );
        assert_eq!(decode::<PreventionChange>(&encoded), Ok(cap));
    }
}

#[test]
fn prevention_cap_codec_refuses_inexact_missing_and_unknown_operands() {
    for text in [
        r#"{"kind":"cap","threshold":6}"#,
        r#"{"kind":"cap","replacement":5}"#,
        r#"{"kind":"cap","threshold":6,"replacement":5,"amount":-1}"#,
        r#"{"kind":"cap","threshold":6,"threshold":7,"replacement":5}"#,
        r#"{"kind":"cap","threshold":6,"replacement":5,"replacement":4}"#,
        r#"{"kind":"cap","threshold":-1,"replacement":5}"#,
        r#"{"kind":"cap","threshold":6,"replacement":-1}"#,
        r#"{"kind":"cap","threshold":null,"replacement":5}"#,
        r#"{"kind":"cap","threshold":6,"replacement":"5"}"#,
    ] {
        assert_eq!(
            decode::<PreventionChange>(text),
            Err(HostError::FORMAT),
            "{text}"
        );
    }
    for token in [
        "9007199254740992",
        "18446744073709551616",
        "0.5",
        "1.0",
        "1e0",
    ] {
        for text in [
            format!(r#"{{"kind":"cap","threshold":{token},"replacement":5}}"#),
            format!(r#"{{"kind":"cap","threshold":6,"replacement":{token}}}"#),
        ] {
            assert_eq!(
                decode::<PreventionChange>(&text),
                Err(HostError::RANGE),
                "{text}"
            );
        }
    }
}

#[test]
fn prevention_cap_schema_requires_both_closed_safe_integer_operands() {
    let document = serde_json::to_value(schema()).unwrap();
    let rooted = json!({
        "$schema":document.get("$schema").unwrap(),
        "$ref":"#/$defs/PreventionChange",
        "$defs":document.get("$defs").unwrap(),
    });
    let validator = jsonschema::validator_for(&rooted).unwrap();
    for valid in [
        json!({"kind":"cap","threshold":6_u8,"replacement":5_u8}),
        json!({"kind":"cap","threshold":0_u8,"replacement":MAX_SAFE_INTEGER}),
        json!({"kind":"cap","threshold":MAX_SAFE_INTEGER,"replacement":0_u8}),
        json!({"kind":"prevent"}),
        json!({"kind":"add","amount":-1_i8}),
    ] {
        assert!(validator.is_valid(&valid), "{valid}");
    }
    let valid = json!({"kind":"cap","threshold":6_u8,"replacement":5_u8});
    for field in ["threshold", "replacement"] {
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(!validator.is_valid(&missing));
        for bad in [
            json!(-1_i8),
            json!(0.5_f64),
            json!(MAX_SAFE_INTEGER + 1),
            json!("5"),
            Value::Null,
        ] {
            let mut changed = valid.clone();
            changed.as_object_mut().unwrap().insert(field.into(), bad);
            assert!(!validator.is_valid(&changed), "{changed}");
        }
    }
    let mut extended = valid;
    extended
        .as_object_mut()
        .unwrap()
        .insert("amount".into(), json!(-1_i8));
    assert!(!validator.is_valid(&extended));
}
