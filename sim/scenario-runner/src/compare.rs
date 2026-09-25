//! Comparison rules of contract sections 2.2, 2.3 and 6.
//!
//! Where the contract is silent, the choices made here are:
//! - lists outside zones are compared as multisets of partially matching items;
//! - numbers compare by value (`1` equals `1.0`);
//! - events match when every written field matches (fields the engine adds are ignored);
//! - an event `group` must be one contiguous block in the engine's event stream;
//! - a zone placeholder must be exactly `{filler: n}`: an unidentified card that carries
//!   an id or any other field is not a placeholder;
//! - `awaiting.choices` must equal the complete option set exactly, field for field.

use core::iter;
use serde_json::{Map, Value};

use crate::engine::View;

/// Zones compared in order (contract 2.2); every other zone is a multiset.
const ORDERED_ZONES: &[&str] = &["deck"];

/// Zone keys of contract 2.2.
pub const ZONES: &[&str] = &[
    "deck",
    "evolve_deck",
    "hand",
    "field",
    "ex",
    "cemetery",
    "banish",
    "resolution",
    "evolution",
    "race",
    "drive",
    "trigger",
    "equipment",
];

/// `expected` matches `actual` when every key written in `expected` matches.
#[must_use]
pub fn subset(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) => e
            .iter()
            .all(|(k, v)| a.get(k).is_some_and(|av| subset(v, av))),
        (Value::Array(e), Value::Array(a)) => multiset(e, a),
        (Value::Number(e), Value::Number(a)) => e.as_f64() == a.as_f64(),
        _ => expected == actual,
    }
}

/// Every expected item pairs with a distinct actual item and none is left over.
fn multiset(expected: &[Value], actual: &[Value]) -> bool {
    expected.len() == actual.len()
        && pair_up(expected, actual, &mut vec![false; actual.len()], subset)
}

fn set_used(used: &mut [bool], at: usize, value: bool) {
    if let Some(slot) = used.get_mut(at) {
        *slot = value;
    }
}

/// Compares the value at an `assert` path.
#[must_use]
pub fn assert_value(path: &str, expected: &Value, actual: &Value) -> bool {
    if path == "knowledge" {
        return knowledge(expected, actual);
    }
    let segments: Vec<&str> = path.split('.').collect();
    match (segments.as_slice(), expected, actual) {
        ([_, zone], Value::Array(e), Value::Array(a)) if ZONES.contains(zone) => {
            zone_list(ORDERED_ZONES.contains(zone), e, a)
        }
        _ => subset(expected, actual),
    }
}

/// Zone contents: `{filler: n}` stands for `n` unidentified cards at that position.
fn zone_list(ordered: bool, expected: &[Value], actual: &[Value]) -> bool {
    let e = expand_fillers(expected);
    let a = expand_fillers(actual);
    if ordered {
        e.len() == a.len() && e.iter().zip(&a).all(|(x, y)| zone_item(x, y))
    } else {
        e.len() == a.len() && pair_up(&e, &a, &mut vec![false; a.len()], zone_item)
    }
}

/// A placeholder matches only a bare placeholder; anything else matches by `subset`.
fn zone_item(expected: &Value, actual: &Value) -> bool {
    if filler_count(expected).is_some() {
        filler_count(actual) == Some(1)
    } else {
        subset(expected, actual)
    }
}

/// `n` for a bare `{filler: n}`; `None` for anything with more fields.
fn filler_count(item: &Value) -> Option<u64> {
    let m = item.as_object()?;
    (m.len() == 1).then(|| m.get("filler").and_then(Value::as_u64))?
}

fn expand_fillers(items: &[Value]) -> Vec<Value> {
    let mut out = Vec::new();
    for item in items {
        match filler_count(item) {
            Some(n) => out.extend(iter::repeat_n(
                filler_one(),
                usize::try_from(n).unwrap_or(usize::MAX),
            )),
            None => out.push(item.clone()),
        }
    }
    out
}

fn pair_up(
    expected: &[Value],
    actual: &[Value],
    used: &mut [bool],
    same: fn(&Value, &Value) -> bool,
) -> bool {
    let Some((first, rest)) = expected.split_first() else {
        return true;
    };
    for (i, item) in actual.iter().enumerate() {
        if used.get(i) == Some(&false) && same(first, item) {
            set_used(used, i, true);
            if pair_up(rest, actual, used, same) {
                return true;
            }
            set_used(used, i, false);
        }
    }
    false
}

/// Structural equality with numbers compared by value.
#[must_use]
pub fn exact(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) => {
            e.len() == a.len()
                && e.iter()
                    .all(|(k, v)| a.get(k).is_some_and(|av| exact(v, av)))
        }
        (Value::Array(e), Value::Array(a)) => {
            e.len() == a.len() && e.iter().zip(a).all(|(x, y)| exact(x, y))
        }
        (Value::Number(e), Value::Number(a)) => e.as_f64() == a.as_f64(),
        _ => expected == actual,
    }
}

fn filler_one() -> Value {
    let mut m = Map::new();
    m.insert("filler".into(), Value::from(1_u64));
    Value::Object(m)
}

/// `{knows, does_not_know}` against the engine's `{identifiable: [...]}`.
fn knowledge(expected: &Value, actual: &Value) -> bool {
    let ids = |v: Option<&Value>| -> Vec<Value> {
        v.and_then(Value::as_array).cloned().unwrap_or_default()
    };
    let identifiable = ids(actual.get("identifiable"));
    ids(expected.get("knows"))
        .iter()
        .all(|id| identifiable.contains(id))
        && ids(expected.get("does_not_know"))
            .iter()
            .all(|id| !identifiable.contains(id))
}

/// One expected unit in an event sequence: a single event, or a set of events
/// that must all fall in one actual `group`, in any order.
enum Unit<'ev> {
    One(&'ev Value),
    Group(Vec<&'ev Value>),
}

fn units(expected: &[Value]) -> Vec<Unit<'_>> {
    let mut out: Vec<Unit<'_>> = Vec::new();
    let mut labels: Vec<(&Value, usize)> = Vec::new();
    for event in expected {
        match event.get("group") {
            Some(label) => {
                if let Some(&(_, at)) = labels.iter().find(|(l, _)| *l == label) {
                    if let Some(Unit::Group(members)) = out.get_mut(at) {
                        members.push(event);
                    }
                } else {
                    labels.push((label, out.len()));
                    out.push(Unit::Group(vec![event]));
                }
            }
            None => out.push(Unit::One(event)),
        }
    }
    out
}

fn without_group(event: &Value) -> Value {
    let mut e = event.clone();
    if let Value::Object(m) = &mut e {
        m.remove("group");
    }
    e
}

/// The expected events appear in order in `actual` (contract 6.4 subsequence).
#[must_use]
pub fn events_subsequence(expected: &[Value], actual: &[Value]) -> bool {
    place(&units(expected), actual, 0)
}

fn place(units: &[Unit<'_>], actual: &[Value], from: usize) -> bool {
    let Some((first, rest)) = units.split_first() else {
        return true;
    };
    match first {
        Unit::One(e) => {
            let pattern = without_group(e);
            actual
                .iter()
                .enumerate()
                .skip(from)
                .any(|(i, a)| subset(&pattern, a) && place(rest, actual, i.saturating_add(1)))
        }
        Unit::Group(members) => {
            // Try each actual group label that occurs at or after `from`.
            let mut tried: Vec<&Value> = Vec::new();
            for event in actual.iter().skip(from) {
                let Some(label) = event.get("group") else {
                    continue;
                };
                if tried.contains(&label) {
                    continue;
                }
                tried.push(label);
                if !contiguous(actual, label) {
                    continue;
                }
                let slots = positions(actual, label, from);
                let patterns: Vec<Value> = members.iter().map(|m| without_group(m)).collect();
                let mut used = vec![false; slots.len()];
                if let Some(end) = cover(&patterns, actual, &slots, &mut used)
                    && place(rest, actual, end.saturating_add(1))
                {
                    return true;
                }
            }
            false
        }
    }
}

/// The first `group` label whose events are not one unbroken run, if any.
#[must_use]
pub fn broken_group(events: &[Value]) -> Option<Value> {
    events
        .iter()
        .filter_map(|e| e.get("group"))
        .find(|label| !contiguous(events, label))
        .cloned()
}

/// Whether every event carrying `label` sits in one unbroken run.
fn contiguous(actual: &[Value], label: &Value) -> bool {
    let at = positions(actual, label, 0);
    at.iter()
        .zip(at.iter().skip(1))
        .all(|(prev, next)| prev.checked_add(1) == Some(*next))
}

/// Indices at or after `from` of the events carrying `label`.
fn positions(actual: &[Value], label: &Value, from: usize) -> Vec<usize> {
    actual
        .iter()
        .enumerate()
        .skip(from)
        .filter(|(_, e)| e.get("group") == Some(label))
        .map(|(i, _)| i)
        .collect()
}

/// Matches every pattern to a distinct slot; returns the highest slot used.
fn cover(
    patterns: &[Value],
    actual: &[Value],
    slots: &[usize],
    used: &mut [bool],
) -> Option<usize> {
    let Some((first, rest)) = patterns.split_first() else {
        return Some(0);
    };
    for (k, &i) in slots.iter().enumerate() {
        if used.get(k) == Some(&false) && actual.get(i).is_some_and(|a| subset(first, a)) {
            set_used(used, k, true);
            if let Some(end) = cover(rest, actual, slots, used) {
                return Some(end.max(i));
            }
            set_used(used, k, false);
        }
    }
    None
}

/// `events_exact`: restricted to these kinds, actual events equal the listed ones.
#[must_use]
pub fn events_exact(kinds: &[String], expected: &[Value], actual: &[Value]) -> bool {
    let keep = |e: &&Value| {
        e.get("kind")
            .and_then(Value::as_str)
            .is_some_and(|k| kinds.iter().any(|x| x == k))
    };
    let e: Vec<Value> = expected.iter().filter(keep).cloned().collect();
    let a: Vec<Value> = actual.iter().filter(keep).cloned().collect();
    e.len() == a.len() && events_subsequence(&e, &a)
}

/// The first forbidden pattern that some actual event matches, if any.
#[must_use]
pub fn forbidden_hit<'pat>(forbidden: &'pat [Value], actual: &[Value]) -> Option<&'pat Value> {
    forbidden
        .iter()
        .find(|f| actual.iter().any(|a| subset(&without_group(f), a)))
}

/// `awaiting`: same player, and `choices` (when written) is the complete option set.
///
/// From the other player's view only `by` is compared: the choices may be private.
#[must_use]
pub fn awaiting(view: View, expected: &Value, actual: Option<&Value>) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    if expected.get("by") != actual.get("by") {
        return false;
    }
    let other_player = match view {
        View::Omniscient => false,
        View::P1 => actual.get("by") != Some(&Value::from("P1")),
        View::P2 => actual.get("by") != Some(&Value::from("P2")),
    };
    if other_player {
        // The options may be private to the deciding player: none may be sent.
        return actual.get("choices").is_none();
    }
    match (expected.get("choices"), actual.get("choices")) {
        (None, _) => true,
        (Some(Value::Array(e)), Some(Value::Array(a))) => {
            e.len() == a.len() && pair_up(e, a, &mut vec![false; a.len()], exact)
        }
        _ => false,
    }
}

/// Whether `needle` occurs anywhere in `haystack`, as a value or as a map key.
#[must_use]
pub fn mentions(haystack: &Value, needle: &Value) -> bool {
    match haystack {
        Value::Object(m) => m
            .iter()
            .any(|(k, v)| Some(k.as_str()) == needle.as_str() || mentions(v, needle)),
        Value::Array(a) => a.iter().any(|v| mentions(v, needle)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => haystack == needle,
    }
}

/// Whether `text` occurs inside any string or map key in `haystack`.
///
/// Used for card numbers, which are long enough that a substring hit is meaningful
/// (object ids such as `d` are not, so they go through [`mentions`]).
#[must_use]
pub fn contains_text(haystack: &Value, text: &str) -> bool {
    match haystack {
        Value::Object(m) => m
            .iter()
            .any(|(k, v)| k.contains(text) || contains_text(v, text)),
        Value::Array(a) => a.iter().any(|v| contains_text(v, text)),
        Value::String(s) => s.contains(text),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}
