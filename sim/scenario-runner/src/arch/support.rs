//! Comparison helpers shared by the replay and assist checks.

use alloc::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use super::{ArchOptions, Fixed, Hidden, NodeId};
use crate::compare;
use crate::engine::{EngineError, View};

/// Engine-issued ids in the order the third party first received them.
///
/// Labels depend on that order only, so two runs that issue ids in the same order
/// compare equal even when the raw ids differ.
#[derive(Debug, Clone, Default)]
pub(super) struct Ids {
    nodes: Vec<String>,
    events: Vec<String>,
}

impl Ids {
    pub(super) fn node(&mut self, id: &NodeId) {
        if !self.nodes.contains(&id.0) {
            self.nodes.push(id.0.clone());
        }
    }

    pub(super) fn events(&mut self, events: &[Value]) {
        for id in events
            .iter()
            .filter_map(|e| e.get("id").and_then(Value::as_str))
        {
            if !self.events.iter().any(|known| known == id) {
                self.events.push(id.to_owned());
            }
        }
    }

    pub(super) fn knows_node(&self, id: &str) -> bool {
        self.nodes.iter().any(|n| n == id)
    }

    /// An id the third party never received stays as it is: folding unknown ids into
    /// one label would hide differences the paired comparisons must see.
    fn label(list: &[String], prefix: &str, id: &str) -> String {
        list.iter().position(|known| known == id).map_or_else(
            || id.to_owned(),
            |i| format!("#{prefix}{}", i.saturating_add(1)),
        )
    }

    fn node_label(&self, id: &str) -> String {
        Self::label(&self.nodes, "N", id)
    }

    fn event_label(&self, id: &str) -> String {
        Self::label(&self.events, "E", id)
    }
}

/// Replaces engine ids by order labels, only where ids are known to be: event `id`,
/// `cause.event`, `cause.decision` and the approved node-id paths. Skipped paths go.
pub(super) fn normalize(value: &Value, ids: &Ids, options: &ArchOptions) -> Value {
    let mut out = value.clone();
    for path in &options.skip_paths {
        remove_path(&mut out, &segments(path));
    }
    for path in &options.node_id_paths {
        map_path(&mut out, &segments(path), &mut |v| {
            if let Some(s) = v.as_str() {
                *v = Value::from(ids.node_label(s));
            }
        });
    }
    relabel_events(&mut out, ids);
    out
}

fn segments(path: &str) -> Vec<&str> {
    path.split('.').collect()
}

fn relabel_events(value: &mut Value, ids: &Ids) {
    match value {
        Value::Object(m) => {
            let is_event = m.contains_key("kind");
            if is_event && let Some(Value::String(id)) = m.get_mut("id") {
                *id = ids.event_label(id);
            }
            if let Some(Value::Object(cause)) = m.get_mut("cause") {
                if let Some(Value::String(e)) = cause.get_mut("event") {
                    *e = ids.event_label(e);
                }
                if let Some(Value::String(n)) = cause.get_mut("decision") {
                    *n = ids.node_label(n);
                }
            }
            m.values_mut().for_each(|v| relabel_events(v, ids));
        }
        Value::Array(a) => a.iter_mut().for_each(|v| relabel_events(v, ids)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn map_path(value: &mut Value, path: &[&str], f: &mut dyn FnMut(&mut Value)) {
    let Some((head, rest)) = path.split_first() else {
        f(value);
        return;
    };
    match value {
        Value::Object(m) => {
            if *head == "*" {
                for v in m.values_mut() {
                    map_path(v, rest, f);
                }
            } else if let Some(v) = m.get_mut(*head) {
                map_path(v, rest, f);
            } else {
                // Absent key: nothing to map.
            }
        }
        Value::Array(a) => {
            if *head == "*" {
                for v in a.iter_mut() {
                    map_path(v, rest, f);
                }
            } else if let Some(v) = head.parse::<usize>().ok().and_then(|i| a.get_mut(i)) {
                map_path(v, rest, f);
            } else {
                // Absent index: nothing to map.
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn remove_path(value: &mut Value, path: &[&str]) {
    let Some((last, parent)) = path.split_last() else {
        return;
    };
    map_path(value, parent, &mut |v| match v {
        Value::Object(m) => {
            if *last == "*" {
                m.clear();
            } else {
                m.remove(*last);
            }
        }
        Value::Array(a) => {
            if *last == "*" {
                a.clear();
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    });
}

/// Identities `view` may not know at node `node`, minus those it legitimately carries.
pub(super) fn hidden_at(
    rules: &[Hidden],
    view: View,
    node: usize,
    carried: &[String],
) -> Vec<String> {
    let name = view_name(view);
    let mut out: Vec<String> = Vec::new();
    for rule in rules
        .iter()
        .filter(|r| r.view == name && r.from <= node && node <= r.to)
    {
        for object in &rule.objects {
            if !carried.contains(object) && !out.contains(object) {
                out.push(object.clone());
            }
        }
    }
    out
}

pub(super) const fn view_name(view: View) -> &'static str {
    match view {
        View::Omniscient => "omniscient",
        View::P1 => "P1",
        View::P2 => "P2",
    }
}

/// What the raw outputs give away: a hidden object's id, its card number (unless an
/// object the viewer may know shares that number) or the seed.
pub(super) fn leaks(
    outputs: &Value,
    hidden: &[String],
    cards: &BTreeMap<String, String>,
    seed: &str,
) -> Vec<String> {
    let mut found = Vec::new();
    let public_numbers: BTreeSet<&str> = cards
        .iter()
        .filter(|(id, _)| !hidden.contains(id))
        .map(|(_, card)| card.as_str())
        .collect();
    for id in hidden {
        if compare::mentions(outputs, &Value::from(id.as_str())) {
            found.push(format!("hidden object {id}"));
        } else if let Some(card) = cards.get(id)
            && !public_numbers.contains(card.as_str())
            && compare::contains_text(outputs, card)
        {
            found.push(format!("card number {card} of hidden object {id}"));
        } else {
            // Neither the id nor an unmasked card number appears.
        }
    }
    if compare::contains_text(outputs, seed)
        || compare::contains_text(outputs, &seed.to_uppercase())
    {
        found.push("the seed".to_owned());
    }
    found
}

/// `knowledge.carried` as `(object, from)`; `from` is `None` when absent.
pub(super) fn carried_from(knowledge: &Value) -> Vec<(String, Option<String>)> {
    knowledge
        .get("carried")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    let object = x.get("object").and_then(Value::as_str)?;
                    let from = x.get("from").and_then(Value::as_str).map(str::to_owned);
                    Some((object.to_owned(), from))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Relabels every `cause.decision` by the order its value first appears in `seen` (one
/// table per source), or `#invalid` unless it is a non-empty string. Assist layers have
/// no node ids, so A3 compares which decision a cause names, not the raw reference.
pub(super) fn label_decisions(value: &mut Value, seen: &mut Vec<String>) {
    match value {
        Value::Object(m) => {
            if let Some(Value::Object(cause)) = m.get_mut("cause")
                && let Some(d) = cause.get_mut("decision")
            {
                let label = d.as_str().filter(|s| !s.is_empty()).map_or_else(
                    || "#invalid".to_owned(),
                    |raw| {
                        let at = seen.iter().position(|x| x == raw).unwrap_or_else(|| {
                            seen.push(raw.to_owned());
                            seen.len().saturating_sub(1)
                        });
                        format!("#D{}", at.saturating_add(1))
                    },
                );
                *d = Value::from(label);
            }
            m.values_mut().for_each(|v| label_decisions(v, seen));
        }
        Value::Array(a) => a.iter_mut().for_each(|v| label_decisions(v, seen)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

/// Objects in `knowledge` (`identifiable` and `carried`).
pub(super) fn known(knowledge: &Value) -> (Vec<String>, Vec<String>) {
    let strings = |v: Option<&Value>| -> Vec<String> {
        v.and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| {
                        x.as_str()
                            .or_else(|| x.get("object").and_then(Value::as_str))
                            .map(str::to_owned)
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    (
        strings(knowledge.get("identifiable")),
        strings(knowledge.get("carried")),
    )
}

/// Answers the omniscient questions of a fixed assertion; one line per mismatch.
pub(super) struct Probe<'probe> {
    pub(super) query: &'probe dyn Fn(&str) -> Result<Option<Value>, EngineError>,
    pub(super) awaiting: &'probe dyn Fn() -> Result<Option<Value>, EngineError>,
}

/// Checks one fixed assertion. `deck` holds `D5` once recorded.
pub(super) fn check_fixed(
    probe: &Probe<'_>,
    fixed: &Fixed,
    deck: &mut Option<Vec<String>>,
    label: &str,
) -> Result<Vec<String>, EngineError> {
    let mut fails = Vec::new();
    for (path, want) in &fixed.assert {
        let got = (probe.query)(path)?;
        if !got
            .as_ref()
            .is_some_and(|g| compare::assert_value(path, want, g))
        {
            fails.push(format!(
                "{label} N{}: {path} is {got:?}, expected {want}",
                fixed.node
            ));
        }
    }
    if let Some(want) = &fixed.awaiting_exact {
        let got = (probe.awaiting)()?;
        if !compare::awaiting(View::Omniscient, want, got.as_ref()) {
            fails.push(format!(
                "{label} N{}: awaiting {got:?}, expected {want}",
                fixed.node
            ));
        }
    }
    if let Some(want) = &fixed.awaiting_do {
        let got = (probe.awaiting)()?;
        if !awaits(got.as_ref(), &want.by, &want.what) {
            fails.push(format!(
                "{label} N{}: awaiting {got:?}, expected {} to {}",
                fixed.node, want.by, want.what
            ));
        }
    }
    for (path, want) in &fixed.contains {
        let got = (probe.query)(path)?;
        if !list_has(got.as_ref(), want) {
            fails.push(format!(
                "{label} N{}: {path} is {got:?}, expected to contain {want}",
                fixed.node
            ));
        }
    }
    if let Some(record) = &fixed.record_deck {
        let got = (probe.query)(&record.path)?;
        match ids_of(got.as_ref()) {
            Some(order) if is_ordering(&order, &record.of) => *deck = Some(order),
            _ => fails.push(format!(
                "{label} N{}: {} is {got:?}, expected an ordering of {:?}",
                fixed.node, record.path, record.of
            )),
        }
    }
    if let Some(drew) = &fixed.drew_recorded {
        fails.extend(check_drew(
            probe,
            drew,
            deck.as_deref(),
            &format!("{label} N{}", fixed.node),
        )?);
    }
    Ok(fails)
}

fn check_drew(
    probe: &Probe<'_>,
    drew: &super::DrewRecorded,
    deck: Option<&[String]>,
    label: &str,
) -> Result<Vec<String>, EngineError> {
    let Some((top, rest)) = deck.and_then(<[String]>::split_first) else {
        return Ok(vec![format!("{label}: no deck order was recorded")]);
    };
    let mut fails = Vec::new();
    let hand = (probe.query)(&drew.hand)?;
    if !list_has(hand.as_ref(), &Value::from(top.as_str())) {
        fails.push(format!(
            "{label}: {} is {hand:?}, expected to contain {top}",
            drew.hand
        ));
    }
    let left = (probe.query)(&drew.deck)?;
    if ids_of(left.as_ref()).as_deref() != Some(rest) {
        fails.push(format!(
            "{label}: {} is {left:?}, expected {rest:?}",
            drew.deck
        ));
    }
    Ok(fails)
}

pub(super) fn awaits(awaiting: Option<&Value>, by: &str, what: &str) -> bool {
    awaiting.is_some_and(|a| {
        a.get("by").and_then(Value::as_str) == Some(by)
            && a.get("choices").and_then(Value::as_array).is_some_and(|c| {
                c.iter()
                    .any(|o| o.get("do").and_then(Value::as_str) == Some(what))
            })
    })
}

fn list_has(list: Option<&Value>, want: &Value) -> bool {
    list.and_then(Value::as_array).is_some_and(|items| {
        items
            .iter()
            .any(|item| item == want || item.get("id") == Some(want))
    })
}

fn ids_of(list: Option<&Value>) -> Option<Vec<String>> {
    list.and_then(Value::as_array)?
        .iter()
        .map(|v| {
            v.as_str()
                .or_else(|| v.get("id").and_then(Value::as_str))
                .map(str::to_owned)
        })
        .collect()
}

fn is_ordering(order: &[String], of: &[String]) -> bool {
    let mut a = order.to_vec();
    let mut b = of.to_vec();
    a.sort();
    b.sort();
    a == b
}

/// Whether `haystack` contains an object that `pattern` matches (written keys only).
pub(super) fn has_match(haystack: &Value, pattern: &Value) -> bool {
    compare::subset(pattern, haystack)
        || match haystack {
            Value::Object(m) => m.values().any(|v| has_match(v, pattern)),
            Value::Array(a) => a.iter().any(|v| has_match(v, pattern)),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
        }
}

/// A JSON object of the three parts of an observation, for whole comparisons.
pub(super) fn observation_value(o: &super::Observation) -> Value {
    let mut m = Map::new();
    m.insert("projection".into(), o.projection.clone());
    m.insert("awaiting".into(), o.awaiting.clone().unwrap_or(Value::Null));
    m.insert("knowledge".into(), o.knowledge.clone());
    Value::Object(m)
}

#[cfg(test)]
mod tests;
