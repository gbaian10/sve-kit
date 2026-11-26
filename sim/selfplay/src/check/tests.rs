//! Each invariant fails on the smallest change to a consistent pair of updates.

#![expect(
    clippy::default_numeric_fallback,
    reason = "test fixtures are literal JSON"
)]

use super::Checker;
use serde_json::{Value, json};
use sve_battle_contract::BattleUpdate;

/// The contract's own baseline for the first seat: untouched decks before the opening.
const BASELINE: &str = include_str!("../../../battle-contract/fixtures/host/update-baseline.json");

/// Both seats' views of that stop; the second seat waits.
fn pair() -> [Value; 2] {
    let mut first: Value = serde_json::from_str(BASELINE).unwrap();
    first.as_object_mut().unwrap().remove("kind");
    // A Quick window is a settled stop: nothing is half declared, so the seats must agree.
    first["input"]["question"]["purpose"] = json!("quick_window");
    first["input"]["question"]["use"] = json!("quick");
    first["input"]["question"]["shape"] = json!({"kind":"quick","window":"attack","options":[]});
    first["input"]["question"]["auto_answer"] = json!({"kind":"unknown"});
    let mut second = first.clone();
    second["viewer"] = json!("p2");
    second["observation"]["viewer"] = json!("p2");
    second["input"] =
        json!({"kind":"waiting","actor":"p1","context":{"processing":null,"dice":null}});
    [first, second]
}

fn decks() -> [Vec<String>; 2] {
    let deck = |prefix: &str| (0..60).map(|index| format!("{prefix}{index}")).collect();
    [deck("a"), deck("b")]
}

/// The first broken invariant after both updates are taken in and compared.
fn broken(pair: &[Value; 2]) -> Option<&'static str> {
    let mut checker = Checker::new(&decks());
    let updates = pair
        .each_ref()
        .map(|json| serde_json::from_value::<BattleUpdate>(json.clone()).unwrap());
    for (update, json) in updates.iter().zip(pair) {
        if let Err(violation) = checker.update(update, json) {
            return Some(violation.name);
        }
    }
    let latest = updates.map(Some);
    let result = checker.stop(&latest).err().map(|violation| violation.name);
    checker.settle(&latest);
    result
}

fn object(printing: &str, knowledge: &str) -> Value {
    let card = json!({"printing_id":printing,"face":0});
    json!({"card":card,"effective_face":card,"owner":"p2","controller":"p2",
        "knowledge":knowledge,"facing":"up",
        "characteristics":{"rules_names":["x"],"card_type":"フォロワー","traits":[],
            "classes":null,"cost":null},
        "state":null})
}

/// Move one card of the second seat's deck to its cemetery, identified to the listed views.
fn buried(pair: &mut [Value; 2], printing: &str, views: &[(usize, &str)]) {
    for view in pair.iter_mut() {
        let zones = &mut view["observation"]["players"][1]["zones"];
        zones["deck"]["count"] = json!(49);
        zones["cemetery"]["count"] = json!(1);
    }
    for (view, knowledge) in views {
        let observation = &mut pair[*view]["observation"];
        observation["objects"]["o-buried"] = object(printing, knowledge);
        observation["players"][1]["zones"]["cemetery"]["cards"] =
            json!([{"position":0,"object":"o-buried"}]);
    }
}

#[test]
fn a_consistent_pair_passes_and_a_public_card_may_be_named_to_both() {
    assert_eq!(broken(&pair()), None);
    let mut shown = pair();
    buried(&mut shown, "b7", &[(0, "public"), (1, "public")]);
    assert_eq!(broken(&shown), None);
}

#[test]
fn a_card_of_the_other_deck_must_be_shown_before_it_is_named() {
    let mut leaked = pair();
    leaked[0]["observation"]["remembered"] =
        json!([{"card":{"printing_id":"b7","face":0},"owner":"p2"}]);
    assert_eq!(broken(&leaked), Some("hidden_information"));

    // The owner still holds it privately, so the other seat identifying it is a leak too.
    let mut private = pair();
    buried(&mut private, "b7", &[(0, "private"), (1, "private")]);
    assert_eq!(broken(&private), Some("hidden_information"));

    // A seat may always be told its own cards.
    let mut own = pair();
    own[0]["observation"]["remembered"] =
        json!([{"card":{"printing_id":"a7","face":0},"owner":"p1"}]);
    assert_eq!(broken(&own), None);
}

/// One committed movement of a card of the second seat's deck, as that event reads on the wire.
fn movement(printing: &str, from: &str, to: &str) -> Value {
    let end =
        |zone: &str, card: Value| json!({"player":"p2","zone":zone,"position":null,"card":card});
    let card = json!({"card":{"printing_id":printing,"face":0},"object":null});
    json!({"status":"available","animate":[],"batches":[{"id":"b1","events":[{
        "id":"e1","cause":"k1","parent":null,"about":[],"presentation_id":null,
        "body":{"kind":"movement","method":"procedure","by_effect":false,
            "before":end(from, Value::Null),"after":end(to, card)}}]}]})
}

#[test]
fn an_event_shows_a_card_only_when_it_passes_through_a_public_zone() {
    // A Drive check turns the top card face up in the trigger zone: both seats saw it.
    let mut checked = pair();
    for view in &mut checked {
        view["events"] = movement("b7", "deck", "trigger");
    }
    assert_eq!(broken(&checked), None);

    // A draw names the card to its owner only. Telling the other seat is a leak even though
    // the owner's own event names it too.
    let mut drawn = pair();
    for view in &mut drawn {
        view["events"] = movement("b7", "deck", "hand");
    }
    assert_eq!(broken(&drawn), Some("hidden_information"));
}

#[test]
fn the_seats_must_agree_on_public_numbers_and_public_cards() {
    let mut life = pair();
    life[1]["observation"]["players"][0]["life"] = json!(19);
    assert_eq!(broken(&life), Some("view_agreement"));

    let mut count = pair();
    count[1]["observation"]["players"][1]["zones"]["hand"]["count"] = json!(1);
    count[1]["observation"]["players"][1]["zones"]["deck"]["count"] = json!(49);
    assert_eq!(broken(&count), Some("view_agreement"));

    let mut turn = pair();
    turn[1]["observation"]["turn"]["elapsed_turns"] = json!([1, 0]);
    assert_eq!(broken(&turn), Some("view_agreement"));

    let mut public = pair();
    buried(&mut public, "b7", &[(1, "public")]);
    assert_eq!(broken(&public), Some("view_agreement"));
}

#[test]
fn cards_cannot_appear_from_nothing_but_generated_objects_are_not_counted() {
    let mut extra = pair();
    for view in &mut extra {
        view["observation"]["players"][0]["zones"]["cemetery"]["count"] = json!(1);
    }
    assert_eq!(broken(&extra), Some("card_conservation"));

    let mut token = pair();
    for view in &mut token {
        let observation = &mut view["observation"];
        observation["objects"]["o-token"] = object("token", "public");
        observation["players"][1]["zones"]["field"] =
            json!({"count":1,"capacity":5,"cards":[{"position":0,"object":"o-token"}]});
    }
    assert_eq!(broken(&token), None);
}

#[test]
fn a_seat_still_declaring_or_paying_for_a_play_may_be_ahead_of_the_other() {
    let mut declaring = pair();
    declaring[0]["input"]["question"]["purpose"] = json!("play_parameter");
    declaring[0]["input"]["question"]["use"] = json!("optional_cost");
    declaring[0]["input"]["question"]["shape"] =
        json!({"kind":"confirm","allowed":[true, false],"subject":null,"origin":null});
    declaring[0]["observation"]["players"][0]["pp"] = json!(-1);
    assert_eq!(broken(&declaring), None);

    // The posture of a card entering with Ward is a rule choice asked while its play is paid.
    let ward = |stage: &str| {
        let mut entering = pair();
        let question = &mut entering[0]["input"]["question"];
        question["purpose"] = json!("rule_choice");
        question["use"] = json!("ward_entry");
        question["shape"] =
            json!({"kind":"confirm","allowed":[true, false],"subject":null,"origin":null});
        question["processing"]["stage"] = json!(stage);
        entering[0]["observation"]["players"][0]["pp"] = json!(-1);
        entering
    };
    assert_eq!(broken(&ward("pay")), None);
    // Any other rule choice is asked on a board both seats already share.
    assert_eq!(broken(&ward("resolve")), Some("view_agreement"));
}

#[test]
fn an_update_must_be_well_formed_and_the_seats_must_agree_on_who_acts() {
    let mut missing = pair();
    missing[0]["observation"]["players"][0]["zones"]["field"] =
        json!({"count":1,"capacity":5,"cards":[{"position":0,"object":"absent"}]});
    assert_eq!(broken(&missing), Some("zone_structure"));

    let mut crowded = pair();
    crowded[0]["observation"]["players"][0]["zones"]["field"] =
        json!({"count":6,"capacity":5,"cards":[]});
    assert_eq!(broken(&crowded), Some("zone_capacity"));

    let mut waiting = pair();
    waiting[0]["input"] = waiting[1]["input"].clone();
    assert_eq!(broken(&waiting), Some("routing"));

    let mut incoherent = pair();
    incoherent[0]["input"]["question"]["use"] = json!("may_effect");
    assert_eq!(broken(&incoherent), Some("question_shape"));

    let [first, _] = pair();
    let mut checker = Checker::new(&decks());
    let mut later = first.clone();
    later["update_seq"] = json!(3);
    for (json, expected) in [
        (&later, None),
        (&later, Some("update_sequence")),
        (&first, Some("update_sequence")),
    ] {
        let update: BattleUpdate = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(
            checker
                .update(&update, json)
                .err()
                .map(|violation| violation.name),
            expected
        );
    }
}
