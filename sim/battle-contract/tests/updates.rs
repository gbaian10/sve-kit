//! Updates carry every event, presentation step and input state the contract defines.

#![expect(
    clippy::unwrap_used,
    reason = "A missing or malformed fixture must fail the test."
)]

extern crate alloc;

#[path = "support/fixtures.rs"]
mod support;

use alloc::collections::BTreeSet;
use serde_json::Value;
use sve_battle_contract::event::EventDelivery;
use sve_battle_contract::presentation::PresentationOp;
use sve_battle_contract::{HostError, HostMessage, decode, schema};

fn updates() -> Vec<Value> {
    support::fixtures("host")
        .into_iter()
        .map(|(_, text)| serde_json::from_str::<Value>(&text).unwrap())
        .filter(|message| message.get("kind").unwrap() == "update")
        .collect()
}

/// Every variant of one definition in the generated schema, by its tag.
fn tags(definition: &str, tag: &str) -> BTreeSet<String> {
    let document = serde_json::to_value(schema()).unwrap();
    let pointer = format!("/properties/{tag}/const");
    document
        .pointer(&format!("/$defs/{definition}/oneOf"))
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|variant| {
            variant
                .pointer(&pointer)
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect()
}

/// The strings found at one pointer inside every element of the arrays at `lists`.
fn found(documents: &[Value], lists: &[&str], pointer: &str) -> BTreeSet<String> {
    let mut items: Vec<&Value> = documents.iter().collect();
    for list in lists {
        items = items
            .into_iter()
            .filter_map(|item| item.pointer(list))
            .flat_map(|array| array.as_array().unwrap())
            .collect();
    }
    items
        .into_iter()
        .map(|item| item.pointer(pointer).unwrap().as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn every_event_kind_has_a_committed_fixture() {
    let committed = found(&updates(), &["/events/batches", "/events"], "/body/kind");
    assert_eq!(committed, tags("EventBody", "kind"));
}

#[test]
fn every_input_delivery_and_presentation_state_has_a_fixture() {
    let updates = updates();
    assert_eq!(
        found(&updates, &[], "/input/kind"),
        tags("BattleInput", "kind")
    );
    assert_eq!(
        found(&updates, &[], "/events/status"),
        tags("EventDelivery", "status")
    );
    assert_eq!(
        found(&updates, &["/presentation"], "/kind"),
        tags("PresentationOp", "kind")
    );
}

#[test]
fn the_first_update_is_a_silent_baseline_with_an_empty_advice_list() {
    let (_, text) = support::fixtures("host")
        .into_iter()
        .find(|(name, _)| name.ends_with("update-baseline.json"))
        .unwrap();
    let HostMessage::Update(update) = decode::<HostMessage>(&text).unwrap() else {
        panic!("not an update");
    };
    assert_eq!(update.update_seq.get(), 0);
    assert_eq!(
        update.events,
        EventDelivery::Available {
            batches: Vec::new(),
            animate: Vec::new(),
        }
    );
    assert_eq!(update.presentation, Vec::<PresentationOp>::new());
    assert_eq!(update.notices, Vec::new());

    let advised = text.replacen("\"advice\": []", "\"advice\": [null]", 1);
    assert_ne!(advised, text);
    assert_eq!(
        decode::<HostMessage>(&advised).err(),
        Some(HostError::FORMAT)
    );
}

#[test]
fn a_promoted_batch_links_back_by_presentation_id_and_is_not_animated_again() {
    let (_, text) = support::fixtures("host")
        .into_iter()
        .find(|(name, _)| name.ends_with("update-events.json"))
        .unwrap();
    let HostMessage::Update(update) = decode::<HostMessage>(&text).unwrap() else {
        panic!("not an update");
    };
    let EventDelivery::Available { batches, animate } = &update.events else {
        panic!("events unavailable");
    };
    let promoted: Vec<_> = batches
        .iter()
        .filter(|batch| {
            batch
                .events
                .iter()
                .any(|event| event.presentation_id.is_some())
        })
        .collect();
    assert_ne!(promoted, Vec::new());
    for batch in promoted {
        // A scope promotes as a whole: every member carries its link.
        assert!(
            batch
                .events
                .iter()
                .all(|event| event.presentation_id.is_some())
        );
        assert!(!animate.contains(&batch.id));
    }
    for id in animate {
        assert!(batches.iter().any(|batch| &batch.id == id));
    }
}

/// Bodies of one kind in the events fixture, in order.
fn bodies(kind: &str) -> Vec<Value> {
    updates()
        .iter()
        .filter_map(|update| update.pointer("/events/batches"))
        .flat_map(|batches| batches.as_array().unwrap())
        .flat_map(|batch| batch.get("events").unwrap().as_array().unwrap())
        .map(|event| event.get("body").unwrap())
        .filter(|body| body.get("kind").unwrap() == kind)
        .cloned()
        .collect()
}

/// Whether two bodies are equal except for the one field at `pointer`.
fn differ_only_at(first: &Value, second: &Value, pointer: &str) -> bool {
    let mut aligned = first.clone();
    *aligned.pointer_mut(pointer).unwrap() = second.pointer(pointer).unwrap().clone();
    first != second && &aligned == second
}

/// Exactly `N` bodies of one kind, so each case is named instead of indexed.
fn exactly<const N: usize>(kind: &str) -> [Value; N] {
    bodies(kind).try_into().unwrap()
}

#[test]
fn events_keep_the_public_facts_that_tell_similar_occurrences_apart() {
    let [_activated, triggered, unknown] = exactly("ability_played");
    assert!(differ_only_at(&triggered, &unknown, "/ability"));

    let [one_material, two_materials] = exactly("fusion");
    assert!(differ_only_at(&one_material, &two_materials, "/materials"));

    let [first_race, second_race] = exactly("race");
    assert!(differ_only_at(&first_race, &second_race, "/ordinal"));

    let [one_card, other_card, hidden_card] = exactly("drive_trigger");
    assert!(differ_only_at(&one_card, &other_card, "/card"));
    assert_eq!(hidden_card.get("card"), Some(&Value::Null));

    let [one_check, other_check, hidden_check] = exactly("drive_checked");
    assert!(differ_only_at(&one_check, &other_check, "/card"));
    assert_eq!(hidden_check.get("card"), Some(&Value::Null));

    let [general, one_stack, two_stacks] = exactly("payment");
    assert_eq!(general.pointer("/component/kind").unwrap(), "general");
    assert!(differ_only_at(&one_stack, &two_stacks, "/component/stacks"));
}
