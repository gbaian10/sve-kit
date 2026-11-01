//! Derive the small prevention subset needed by legacy scenarios from accepted current data.
//! This creates no baseline and never qualifies a current program for production execution.

use serde_json::{Value, json};

fn only_keys(value: &Value, allowed: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|object| object.keys().all(|key| allowed.contains(&key.as_str())))
}

fn leader(value: &Value) -> Option<Value> {
    if value == &json!({"leaders":"opponents"}) || value == &json!({"leader":"opp"}) {
        Some(json!("opponent.leader"))
    } else if value == &json!({"leader":"self"}) {
        Some(json!("self.leader"))
    } else {
        None
    }
}

fn prevention(head: &Value, line: u64) -> Option<Vec<Value>> {
    if !only_keys(head, &["effects", "zone"])
        || head.get("zone").is_some_and(|zone| zone != "field")
    {
        return None;
    }
    head.get("effects")?
        .as_array()?
        .iter()
        .map(|effect| {
            if !only_keys(effect, &["prevent"]) {
                return None;
            }
            let effect = effect.get("prevent")?;
            if !only_keys(effect, &["on", "kind", "next_only"])
                || effect.get("next_only").is_some_and(|next| next != false)
            {
                return None;
            }
            let kind = match effect.get("kind")?.as_str()? {
                "ability" => "effect",
                "combat" => "battle",
                _ => return None,
            };
            let subjects = match effect.get("on") {
                None => json!("self"),
                Some(target) => leader(target)?,
            };
            Some(json!({"kind":"static", "line":line, "body":{
                "op":"replace_damage", "kind":kind, "prevent":true, "subjects":subjects
            }}))
        })
        .collect()
}

fn activated(head: &Value, line: u64) -> Option<Value> {
    if !only_keys(head, &["cost", "quick", "do"]) {
        return None;
    }
    let cost = head.get("cost")?;
    if !only_keys(cost, &["pp", "act_self"]) {
        return None;
    }
    let mut costs = Vec::new();
    if let Some(pp) = cost.get("pp") {
        costs.push(json!({"op":"pp", "amount":pp.as_u64()?}));
    }
    if let Some(act) = cost.get("act_self")
        && act.as_bool()?
    {
        costs.push(json!({"op":"act", "subjects":"self"}));
    }
    let steps: Option<Vec<Value>> = head.get("do")?.as_array()?.iter().map(|step| {
        if !only_keys(step, &["damage"]) { return None; }
        let damage = step.get("damage")?;
        if !only_keys(damage, &["to", "n"]) { return None; }
        Some(json!({"op":"damage", "subjects":leader(damage.get("to")?)?, "amount":damage.get("n")?.as_u64()?}))
    }).collect();
    let steps = steps?;
    let body = match steps.as_slice() {
        [] => return None,
        [step] => step.clone(),
        _ => json!({"op":"seq", "steps":steps}),
    };
    Some(
        json!({"kind":"activated", "line":line, "quick":head.get("quick").cloned().unwrap_or(json!(false)), "costs":costs, "body":body}),
    )
}

pub(super) fn program(card: &Value) -> Option<Value> {
    let faces = card.get("faces")?.as_object()?;
    if faces.len() != 1 {
        return None;
    }
    let entries = faces.values().next()?.get("abilities")?.as_array()?;
    let mut abilities = Vec::new();
    let mut has_prevention = false;
    for entry in entries {
        let line = entry.get("line")?.as_u64()?;
        if entry
            .get("section")
            .is_some_and(|section| !section.is_null())
            || entry
                .get("ports")
                .is_some_and(|ports| ports != &json!({"imports":{}, "exports":{}}))
        {
            return None;
        }
        if only_keys(
            entry,
            &[
                "line",
                "static",
                "sources",
                "section",
                "ports",
                "generated_by",
            ],
        ) {
            let declarations = prevention(entry.get("static")?, line)?;
            if declarations.is_empty() {
                return None;
            }
            abilities.extend(declarations);
            has_prevention = true;
        } else if only_keys(
            entry,
            &[
                "line",
                "activate",
                "sources",
                "section",
                "ports",
                "generated_by",
            ],
        ) {
            abilities.push(activated(entry.get("activate")?, line)?);
        } else {
            return None;
        }
    }
    has_prevention.then(|| json!({"status":"complete", "abilities":abilities}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_metadata_and_empty_ports_preserve_supported_prevention() {
        let card = json!({"faces":{"base":{"abilities":[
            {"line":1_u8,"sources":[{"binding_id":"synthetic","occurrence":{}}],
             "ports":{"imports":{},"exports":{}},
             "static":{"effects":[{"prevent":{"kind":"ability"}}]}}
        ]}}});
        assert!(program(&card).is_some());
    }

    #[test]
    fn binding_ports_cannot_be_dropped_from_a_historical_projection() {
        let card = json!({"faces":{"base":{"abilities":[
            {"line":1_u8,"ports":{"imports":{"target":"objects"},"exports":{}},
             "static":{"effects":[{"prevent":{"kind":"ability"}}]}}
        ]}}});
        assert!(program(&card).is_none());
    }

    #[test]
    fn unsupported_abilities_cannot_be_dropped_from_a_historical_projection() {
        let card = json!({"faces":{"base":{"abilities":[
            {"line":1_u8,"static":{"effects":[{"prevent":{"kind":"ability"}}]}},
            {"line":2_u8,"lastword":{"do":[{"draw":1_u8}]}}
        ]}}});
        assert!(program(&card).is_none());
    }

    #[test]
    fn a_live_condition_cannot_become_unconditional_historical_prevention() {
        let card = json!({"faces":{"base":{"abilities":[
            {"line":1_u8,"static":{"while":{"awakened":true},"effects":[{"prevent":{"kind":"ability"}}]}}
        ]}}});
        assert!(program(&card).is_none());
    }
}
