//! Q4: the machine-checkable part of the search-evidence audit (design section 3).
//!
//! Whether the log is really written by the engine's decision-applying function, and
//! whether samples are drawn from P1's view only, stays a manual audit.

use alloc::collections::BTreeMap;

use serde_json::Value;

use super::{AiPosition, AiReport};

/// Longest parent chain followed before calling it a cycle.
const CHAIN_LIMIT: usize = 10_000;

/// Checks a search log of `{edge, parent, sample, by, at, rule, decision}` entries
/// (root entries also carry `sample_hand: [{id, card}]`) for one `think` call on
/// `position` (`q-a` or `q-b`).
///
/// Passes (empty result) when the log is a forest per sample, each sample states the
/// hand P2 is assumed to hold, the edge count is within `budget` and equals
/// `report.edges`, and some sample has the path "P1 attacks at the root → P2 decides
/// at 8.4.7 → P2 plays a Quick card of `quick_cards` from that sample's hand, whose
/// cost fits P2's PP".
#[must_use]
pub fn check_search_log(
    log: &[Value],
    report: &AiReport,
    budget: u64,
    position: &AiPosition,
) -> Vec<String> {
    let mut reasons = Vec::new();
    let mut seen: BTreeMap<String, &Value> = BTreeMap::new();
    let mut hands: BTreeMap<String, &Value> = BTreeMap::new();
    for (i, entry) in log.iter().enumerate() {
        let Some(edge) = key(entry.get("edge")) else {
            reasons.push(format!("entry {i} has no edge"));
            continue;
        };
        let Some(sample) = key(entry.get("sample")) else {
            reasons.push(format!("edge {edge} has no sample"));
            continue;
        };
        if seen.contains_key(&edge) {
            reasons.push(format!("edge {edge} appears twice"));
            continue;
        }
        match key(entry.get("parent")) {
            Some(parent) => match seen.get(&parent) {
                None => reasons.push(format!(
                    "edge {edge}: parent {parent} is not an earlier edge"
                )),
                Some(p) if key(p.get("sample")).as_ref() != Some(&sample) => {
                    reasons.push(format!("edge {edge}: parent {parent} is in another sample"));
                }
                Some(_) => {}
            },
            None => reasons.extend(root_hand(&edge, &sample, entry, &mut hands, position)),
        }
        seen.insert(edge, entry);
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
    let answered = log
        .iter()
        .any(|entry| quick_after_attack(entry, &seen, &hands, position));
    if !answered {
        reasons.push(
            "no path: P1 root attack → P2 at 8.4.7 → a Quick card from the sample's hand"
                .to_owned(),
        );
    }
    reasons
}

/// Records a sample's assumed hand from a root entry and checks it against the position.
fn root_hand<'log>(
    edge: &str,
    sample: &str,
    entry: &'log Value,
    hands: &mut BTreeMap<String, &'log Value>,
    position: &AiPosition,
) -> Vec<String> {
    let Some(hand) = entry.get("sample_hand").filter(|h| h.is_array()) else {
        return vec![format!("root edge {edge} has no sample_hand")];
    };
    if let Some(known) = hands.get(sample) {
        return if *known == hand {
            Vec::new()
        } else {
            vec![format!("sample {sample} states two different hands")]
        };
    }
    hands.insert(sample.to_owned(), hand);
    let mut reasons = Vec::new();
    let p2 = path(&position.setup, &["players", "P2"]);
    let size = p2
        .and_then(|p| path(p, &["zones", "hand"]))
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if hand.as_array().map_or(0, Vec::len) != size {
        reasons.push(format!("sample {sample}: hand size is not {size}"));
    }
    let listed = |card: &str| {
        p2.and_then(|p| p.get("deck_list"))
            .and_then(Value::as_array)
            .is_none_or(|list| list.iter().any(|e| e["card"].as_str() == Some(card)))
    };
    for card in hand.as_array().into_iter().flatten() {
        let number = card["card"].as_str().unwrap_or_default();
        if !listed(number) {
            reasons.push(format!(
                "sample {sample}: {number} is not in P2's deck list"
            ));
        }
    }
    reasons
}

fn path<'val>(value: &'val Value, keys: &[&str]) -> Option<&'val Value> {
    keys.iter().try_fold(value, |node, key| node.get(key))
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

/// Whether `entry` plays a Quick card that its sample's hand holds and P2 can pay for.
fn plays_quick(entry: &Value, hands: &BTreeMap<String, &Value>, position: &AiPosition) -> bool {
    let Some(hand) = key(entry.get("sample")).and_then(|s| hands.get(&s).copied()) else {
        return false;
    };
    let object = path(entry, &["decision", "card"]).and_then(Value::as_str);
    let Some(number) = hand
        .as_array()
        .into_iter()
        .flatten()
        .find(|c| c["id"].as_str().is_some() && c["id"].as_str() == object)
        .and_then(|c| c["card"].as_str())
    else {
        return false;
    };
    let pp = path(&position.setup, &["players", "P2", "pp", "current"])
        .and_then(Value::as_u64)
        .unwrap_or(0);
    position
        .quick_cards
        .iter()
        .any(|q| q.card == number && q.cost <= pp)
}

fn quick_after_attack(
    entry: &Value,
    seen: &BTreeMap<String, &Value>,
    hands: &BTreeMap<String, &Value>,
    position: &AiPosition,
) -> bool {
    let is_quick_play = is(entry, "by", "P2")
        && is(entry, "rule", "8.4.7")
        && decision_do(entry) == Some("play")
        && plays_quick(entry, hands, position);
    if !is_quick_play {
        return false;
    }
    let mut current = entry;
    for _ in 0..CHAIN_LIMIT {
        match key(current.get("parent")).and_then(|p| seen.get(&p)) {
            Some(parent) => current = parent,
            None => {
                return key(current.get("parent")).is_none()
                    && is(current, "by", "P1")
                    && decision_do(current) == Some("attack");
            }
        }
    }
    false
}
