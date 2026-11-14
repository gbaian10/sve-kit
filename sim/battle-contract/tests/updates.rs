//! Updates carry every event, presentation step and input state the contract defines.

#![expect(
    clippy::unwrap_used,
    reason = "A missing or malformed fixture must fail the test."
)]

extern crate alloc;

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
    let inputs: BTreeSet<_> = updates
        .iter()
        .map(|update| {
            update
                .pointer("/input/kind")
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(inputs, tags("BattleInput", "kind"));

    let deliveries: BTreeSet<_> = updates
        .iter()
        .map(|update| {
            update
                .pointer("/events/status")
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(deliveries, tags("EventDelivery", "status"));

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
    assert!(!promoted.is_empty());
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
