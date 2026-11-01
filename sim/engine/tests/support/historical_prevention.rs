//! Derive a closed damage-replacement subset for historical scenarios from current data.
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
            if only_keys(effect, &["damage_cap"]) {
                return damage_cap(effect.get("damage_cap")?, line);
            }
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

fn damage_recipient(value: &Value) -> Option<Value> {
    if let Some(subject) = leader(value) {
        return Some(subject);
    }
    if value == &json!({"who":"opp", "zone":"field", "kind":"follower"}) {
        return Some(json!({"zone":"field", "side":"opponent", "type":"follower"}));
    }
    if !only_keys(value, &["any_of"]) {
        return None;
    }
    let sets = value.get("any_of")?.as_array()?;
    if sets.is_empty() {
        return None;
    }
    let subjects: Option<Vec<Value>> = sets.iter().map(damage_recipient).collect();
    Some(json!({"union":subjects?}))
}

fn damage_body(steps: &Value) -> Option<Value> {
    let steps: Option<Vec<Value>> = steps.as_array()?.iter().map(|step| {
        if !only_keys(step, &["damage"]) { return None; }
        let damage = step.get("damage")?;
        if !only_keys(damage, &["to", "n"]) { return None; }
        Some(json!({"op":"damage", "subjects":damage_recipient(damage.get("to")?)?, "amount":damage.get("n")?.as_u64()?}))
    }).collect();
    match steps?.as_slice() {
        [] => None,
        [step] => Some(step.clone()),
        steps => Some(json!({"op":"seq", "steps":steps})),
    }
}

fn fanfare(head: &Value, line: u64) -> Option<Value> {
    if !only_keys(head, &["do"]) {
        return None;
    }
    Some(
        json!({"kind":"trigger", "line":line, "event":"enter", "subject":"self", "body":damage_body(head.get("do")?)?}),
    )
}

fn damage_cap(shape: &Value, line: u64) -> Option<Value> {
    if !only_keys(shape, &["on", "threshold", "set"])
        || shape.get("on").is_some_and(|on| on != "this")
    {
        return None;
    }
    let threshold = shape.get("threshold")?.as_u64()?;
    let amount = shape.get("set")?.as_u64()?;
    if threshold <= amount || threshold > i64::MAX.unsigned_abs() {
        return None;
    }
    Some(json!({"kind":"static","line":line,"body":{
        "op":"replace_damage","subjects":"self","kind":"any","set":amount,
        "condition":{"fn":"ge","args":[{"read":"damage.amount"},threshold]}
    }}))
}

fn keywords(head: &Value, line: u64) -> Option<Vec<Value>> {
    head.as_array()?
        .iter()
        .map(|keyword| {
            let name = match keyword.as_str()? {
                "ward" => "guard",
                name @ ("rush" | "storm" | "aura" | "bane" | "assail" | "intimidate") => name,
                _ => return None,
            };
            Some(json!({"kind":"static","line":line,"body":{"op":"macro","name":name}}))
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
    let body = damage_body(head.get("do")?)?;
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
    let mut has_replacement = false;
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
            has_replacement = true;
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
        } else if only_keys(
            entry,
            &[
                "line",
                "fanfare",
                "sources",
                "section",
                "ports",
                "generated_by",
            ],
        ) {
            abilities.push(fanfare(entry.get("fanfare")?, line)?);
        } else if only_keys(
            entry,
            &[
                "line",
                "keywords",
                "sources",
                "section",
                "ports",
                "generated_by",
            ],
        ) {
            abilities.extend(keywords(entry.get("keywords")?, line)?);
        } else {
            return None;
        }
    }
    has_replacement.then(|| json!({"status":"complete", "abilities":abilities}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cap_card() -> Value {
        json!({"faces":{"base":{"abilities":[
            {"line":1_u8,"keywords":["rush","ward"]},
            {"line":2_u8,"static":{"effects":[{"damage_cap":{"threshold":4_u8,"set":3_u8}}]}}
        ]}}})
    }

    #[test]
    fn a_complete_cap_preserves_keywords_and_each_event_threshold() {
        let output = program(&cap_card()).unwrap();
        assert_eq!(output["abilities"][0]["body"]["name"], "rush");
        assert_eq!(output["abilities"][1]["body"]["name"], "guard");
        assert_eq!(
            output["abilities"][2]["body"],
            json!({
                "op":"replace_damage","subjects":"self","kind":"any","set":3_u8,
                "condition":{"fn":"ge","args":[{"read":"damage.amount"},4_u8]}
            })
        );
    }

    #[test]
    fn unknown_cap_fields_or_keywords_reject_the_whole_card() {
        let mut input = cap_card();
        input["faces"]["base"]["abilities"][1]["static"]["effects"][0]["damage_cap"]["next_only"] =
            json!(true);
        assert!(program(&input).is_none(), "a limit cannot be lost");
        let mut keyword_input = cap_card();
        keyword_input["faces"]["base"]["abilities"][0]["keywords"] = json!(["drain"]);
        assert!(
            program(&keyword_input).is_none(),
            "a triggered keyword cannot be lost"
        );
    }

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
    #[test]
    fn ward_and_union_damage_fanfare_are_preserved_as_complete_programs() {
        let card = json!({"faces":{"base":{"abilities":[
            {"line":1_u8,"keywords":["ward"]},
            {"line":2_u8,"static":{"effects":[{"prevent":{"kind":"ability"}}]}},
            {"line":3_u8,"fanfare":{"do":[{"damage":{"to":{"any_of":[
                {"leaders":"opponents"},{"who":"opp","zone":"field","kind":"follower"}
            ]},"n":5_u8}}]}}
        ]}}});
        let result = program(&card).unwrap();
        let abilities = result.get("abilities").unwrap().as_array().unwrap();
        assert_eq!(abilities.len(), 3);
        assert_eq!(
            abilities.first().unwrap().pointer("/body/name"),
            Some(&json!("guard"))
        );
        assert_eq!(
            abilities.last().unwrap(),
            &json!({"kind":"trigger","line":3_u8,
            "event":"enter","subject":"self","body":{"op":"damage","amount":5_u8,
            "subjects":{"union":["opponent.leader",{"zone":"field","side":"opponent","type":"follower"}]}}})
        );
    }

    #[test]
    fn a_fanfare_cost_cannot_be_dropped_from_a_historical_projection() {
        let card = json!({"faces":{"base":{"abilities":[
            {"line":1_u8,"static":{"effects":[{"prevent":{"kind":"ability"}}]}},
            {"line":2_u8,"fanfare":{"cost":{"pp":2_u8},"do":[{"damage":{"to":{"leaders":"opponents"},"n":5_u8}}]}}
        ]}}});
        assert!(program(&card).is_none());
    }
}
