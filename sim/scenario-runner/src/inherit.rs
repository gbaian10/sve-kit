//! `inherit` expansion (contract 2.6).
//!
//! Copies the parent's `setup` and `card_facts`, then deep-merges this scenario's
//! keys: maps merge key by key, lists replace whole. `decisions`, `random` and
//! `expected` are never inherited.

use std::collections::HashMap;

use serde_json::Value;

use crate::Error;
use crate::model::Scenario;

/// A scenario with `inherit` resolved: what an engine is loaded from.
#[derive(Debug, Clone)]
pub struct Fixture {
    /// Question id, for reports and adapter diagnostics.
    pub question: String,
    /// Scenario name.
    pub scenario: String,
    /// Merged `setup`.
    pub setup: Value,
    /// Merged `card_facts`.
    pub card_facts: Value,
    /// This scenario's own `random`.
    pub random: Value,
}

/// Resolves `inherit` for every scenario of a question.
///
/// # Errors
/// On a missing parent or an inheritance cycle.
pub fn expand(question: &str, scenarios: &[Scenario]) -> Result<Vec<Fixture>, Error> {
    let by_name: HashMap<&str, &Scenario> =
        scenarios.iter().map(|s| (s.name.as_str(), s)).collect();
    scenarios
        .iter()
        .map(|s| {
            let (setup, card_facts) = resolve(&by_name, s, &mut Vec::new())?;
            Ok(Fixture {
                question: question.to_owned(),
                scenario: s.name.clone(),
                setup,
                card_facts,
                random: s.random.clone().unwrap_or(Value::Null),
            })
        })
        .collect()
}

fn resolve<'a>(
    by_name: &HashMap<&str, &'a Scenario>,
    scenario: &'a Scenario,
    seen: &mut Vec<&'a str>,
) -> Result<(Value, Value), Error> {
    let own_setup = scenario.setup.clone().unwrap_or_else(empty);
    let own_facts = scenario.card_facts.clone().unwrap_or_else(empty);
    let Some(parent) = scenario.inherit.as_deref() else {
        return Ok((own_setup, own_facts));
    };
    if parent == scenario.name || seen.contains(&parent) {
        return Err(Error::Question(format!("inherit cycle via {parent:?}")));
    }
    let base = by_name
        .get(parent)
        .ok_or_else(|| Error::Question(format!("inherit target {parent:?} not found")))?;
    seen.push(scenario.name.as_str());
    let (mut setup, mut facts) = resolve(by_name, base, seen)?;
    merge(&mut setup, own_setup);
    merge(&mut facts, own_facts);
    Ok((setup, facts))
}

fn empty() -> Value {
    Value::Object(serde_json::Map::new())
}

/// Deep merge: maps key by key, anything else (lists included) replaces.
pub fn merge(base: &mut Value, over: Value) {
    match (base, over) {
        (Value::Object(b), Value::Object(o)) => {
            for (key, value) in o {
                match b.get_mut(&key) {
                    Some(slot) => merge(slot, value),
                    None => {
                        b.insert(key, value);
                    }
                }
            }
        }
        (slot, value) => *slot = value,
    }
}
