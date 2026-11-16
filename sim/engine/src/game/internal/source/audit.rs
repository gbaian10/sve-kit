//! The accepted catalogue must produce a bound owner before any ability executes.

use super::{Game, SourceInformation};
use crate::battle::tests::rule_fixtures;
use crate::ir::{AbilityId, AbilityInstanceId, PlayerId, RuntimeAbilitySource};
use alloc::collections::BTreeMap;
use serde_json::{Value, json};

#[path = "../../../../tests/support/repository.rs"]
mod repository;

fn position() -> Value {
    json!({"history":"explicit","turn":{"active":"P1","first_player":"P1","phase":"main",
        "elapsed_turns":{"P1":3_u8,"P2":2_u8}},"players":{
        "P1":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":0_u8,"max":0_u8},"ep":0_u8,"sep":0_u8,"zones":{}},
        "P2":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":0_u8,"max":0_u8},"ep":0_u8,"sep":0_u8,"zones":{}}
    }})
}

fn source_position(empty: &Value, number: &str, face: u64, kind: &str) -> Value {
    let mut initial = empty.clone();
    let zone = if kind.starts_with("スペル") {
        "hand"
    } else {
        "field"
    };
    // Source information requires followers on the field; binding does not execute their text.
    initial["players"]["P1"]["zones"][zone] = json!([
        {"id":"audit-source","card":number,"state":{"face":face}}
    ]);
    if kind.contains("エボルヴ") {
        // This synthetic base/form association supplies cost information, not legal evolution.
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"audit-source","card":number,"state":{"face":0_u64,"evolved_with":"audit-form"}}
        ]);
        initial["players"]["P1"]["zones"]["evolution"] = json!([
            {"id":"audit-form","card":number,"state":{"face":face}}
        ]);
    }
    initial
}

#[test]
fn every_registered_printed_source_in_the_current_catalogue_binds_an_owner() {
    let selected = repository::selection_with_rules();
    let empty = position();
    let admitted = Game::from_selected_effects(
        selected.clone(),
        &empty,
        &Value::Null,
        &Value::Null,
        "source-catalogue-audit",
    )
    .unwrap();
    let expected = selected
        .current
        .values()
        .map(|card| card.program.card().abilities.len())
        .sum::<usize>();
    let mut cases: BTreeMap<(String, u64), Vec<AbilityId>> = BTreeMap::new();
    for (number, card) in &admitted.card_runtime.definitions.cards {
        for ability in &card.card().abilities {
            let face = admitted
                .effect_source_face(number, Some(&ability.source.face))
                .unwrap_or_else(|error| panic!("source face for {number}: {error}"))
                .unwrap();
            cases
                .entry((number.clone(), face))
                .or_default()
                .push(ability.node.id);
        }
    }
    assert_ne!(
        expected, 0,
        "the accepted catalogue must supply real printed abilities"
    );
    assert_eq!(cases.values().map(Vec::len).sum::<usize>(), expected);
    let mut count = 0_usize;
    let mut rejected_unregistered = false;
    for ((number, face), definitions) in cases {
        let native = admitted
            .catalog
            .face(&number, usize::try_from(face).unwrap())
            .unwrap();
        let kind = native["card_type"].as_str().unwrap();
        let initial = source_position(&empty, &number, face, kind);
        let mut game = Game::from_selected_effects(
            selected.clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "source-catalogue-audit",
        )
        .unwrap_or_else(|error| panic!("source position for {number} face {face}: {error}"));
        for definition in definitions {
            let source = game
                .internal_card_source("audit-source", definition, PlayerId { seat: 1 })
                .unwrap_or_else(|error| {
                    panic!(
                        "registered source for {number} definition {}: {error}",
                        definition.0
                    )
                });
            SourceInformation::current(&game, source).unwrap_or_else(|error| {
                panic!(
                    "binding {number} face {face} definition {}: {error}",
                    definition.0
                )
            });
            count = count.checked_add(1).unwrap();
            if !rejected_unregistered {
                let missing = RuntimeAbilitySource {
                    instance: AbilityInstanceId(u64::MAX),
                    ..source
                };
                assert!(
                    matches!(
                        SourceInformation::current(&game, missing),
                        Err(crate::EngineFailure::Invariant(_))
                    ),
                    "an absent registered instance must fail before producing pending or events"
                );
                rejected_unregistered = true;
            }
        }
    }
    assert_eq!(
        count, expected,
        "every accepted printing and printed definition is bound once"
    );
    assert!(rejected_unregistered);
}
