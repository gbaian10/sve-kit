//! Q4: audit of an engine-written search log (design section 3).

use alloc::collections::BTreeMap;

use serde_json::Value;

use super::AiReport;

/// Longest parent chain followed before calling it a cycle.
const CHAIN_LIMIT: usize = 10_000;

/// Checks a search log of `{edge, parent, sample, by, at, rule, decision}` entries,
/// written by the engine's decision-applying function, against the AI's report.
///
/// Passes (empty result) when edges are unique, every parent is an earlier edge of the
/// same sample, the edge count is within `budget` and equals `report.edges`, and some
/// path runs "P1 attacks at the root → P2 decides at 8.4.7 → a Quick card is played".
#[must_use]
pub fn check_search_log(log: &[Value], report: &AiReport, budget: u64) -> Vec<String> {
    let mut reasons = Vec::new();
    // edge → (index, entry)
    let mut seen: BTreeMap<String, (usize, &Value)> = BTreeMap::new();
    for (i, entry) in log.iter().enumerate() {
        let Some(edge) = key(entry.get("edge")) else {
            reasons.push(format!("entry {i} has no edge"));
            continue;
        };
        if seen.contains_key(&edge) {
            reasons.push(format!("edge {edge} appears twice"));
            continue;
        }
        if let Some(parent) = key(entry.get("parent")) {
            match seen.get(&parent) {
                None => reasons.push(format!(
                    "edge {edge}: parent {parent} is not an earlier edge"
                )),
                Some((_, p)) if p.get("sample") != entry.get("sample") => {
                    reasons.push(format!("edge {edge}: parent {parent} is in another sample"));
                }
                Some(_) => {}
            }
        }
        seen.insert(edge, (i, entry));
    }
    let count = u64::try_from(log.len()).unwrap_or(u64::MAX);
    if count > budget {
        reasons.push(format!("{count} edges exceed the budget of {budget}"));
    }
    if count != report.edges {
        reasons.push(format!(
            "the log has {count} edges but the report says {}",
            report.edges
        ));
    }
    if !log.iter().any(|entry| quick_after_attack(entry, &seen)) {
        reasons.push("no path: P1 root attack → P2 at 8.4.7 → Quick played".to_owned());
    }
    reasons
}

/// Edge and parent ids may be strings or numbers; `null` means none.
fn key(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Null | Value::Bool(_) | Value::Array(_) | Value::Object(_) => None,
    }
}

fn is(entry: &Value, field: &str, want: &str) -> bool {
    entry.get(field).and_then(Value::as_str) == Some(want)
}

fn decision_do(entry: &Value) -> Option<&str> {
    entry
        .get("decision")
        .and_then(|d| d.get("do"))
        .and_then(Value::as_str)
}

fn quick_after_attack(entry: &Value, seen: &BTreeMap<String, (usize, &Value)>) -> bool {
    let is_quick_play =
        is(entry, "by", "P2") && is(entry, "rule", "8.4.7") && decision_do(entry) == Some("play");
    if !is_quick_play {
        return false;
    }
    let mut current = entry;
    for _ in 0..CHAIN_LIMIT {
        match key(current.get("parent")).and_then(|p| seen.get(&p)) {
            Some((_, parent)) => current = parent,
            None => {
                return key(current.get("parent")).is_none()
                    && is(current, "by", "P1")
                    && decision_do(current) == Some("attack");
            }
        }
    }
    false
}
