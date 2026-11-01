//! Closed permission projection for historical scenarios, never current admission.
use serde_json::{Value, json};

fn only_keys(value: &Value, allowed: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|object| object.keys().all(|key| allowed.contains(&key.as_str())))
}

fn field(value: &Value) -> Option<Value> {
    if !only_keys(value, &["who", "zone", "kind", "other"])
        || value.get("zone")? != "field"
        || value.get("kind")? != "follower"
    {
        return None;
    }
    let side = match value.get("who")?.as_str()? {
        "self" => "self",
        "opp" => "opponent",
        "all" => "both",
        _ => return None,
    };
    let mut output = json!({"side":side,"zone":"field","type":"follower"});
    if let Some(other) = value.get("other") {
        output
            .as_object_mut()?
            .insert("other".into(), json!(other.as_bool()?));
    }
    Some(output)
}

fn objects(value: &Value) -> Option<Value> {
    if value == "this" {
        Some(json!("self"))
    } else {
        field(value)
    }
}

fn number(value: &Value) -> Option<Value> {
    if let Some(number) = value.as_i64() {
        Some(json!(number))
    } else if only_keys(value, &["count"]) {
        Some(json!({"count":field(value.get("count")?)?}))
    } else {
        None
    }
}

fn condition(value: &Value) -> Option<Value> {
    if !only_keys(value, &["cmp"]) {
        return None;
    }
    let cmp = value.get("cmp")?;
    if !only_keys(cmp, &["left", "op", "right"]) {
        return None;
    }
    let op = cmp.get("op")?.as_str()?;
    if !["eq", "ne", "ge", "gt", "le", "lt"].contains(&op) {
        return None;
    }
    Some(json!({"fn":op,"args":[number(cmp.get("left")?)?,number(cmp.get("right")?)?]}))
}

fn restriction(head: &Value, line: u64) -> Option<Vec<Value>> {
    if !only_keys(head, &["effects", "zone", "while"])
        || head.get("zone").is_some_and(|zone| zone != "field")
    {
        return None;
    }
    let predicate = match head.get("while") {
        None => None,
        Some(value) => Some(condition(value)?),
    };
    head.get("effects")?
        .as_array()?
        .iter()
        .map(|effect| {
            if !only_keys(effect, &["cannot"]) {
                return None;
            }
            let effect = effect.get("cannot")?;
            if !only_keys(effect, &["what", "on", "target"]) || effect.get("what")? != "attack" {
                return None;
            }
            let action = match effect.get("target") {
                None => "attack",
                Some(target)
                    if target == &json!({"leader":"opp"})
                        || target == &json!({"leaders":"opponents"}) =>
                {
                    "attack_leader"
                }
                Some(_) => return None,
            };
            let subjects = effect
                .get("on")
                .map_or_else(|| Some(json!("self")), objects)?;
            let mut body = json!({"op":"restrict","subjects":subjects,"action":action});
            if let Some(predicate) = &predicate {
                body.as_object_mut()?
                    .insert("condition".into(), predicate.clone());
            }
            Some(json!({"line":line,"kind":"static","body":body}))
        })
        .collect()
}

fn body(head: &Value) -> Option<Value> {
    if !only_keys(head, &["do"]) {
        return None;
    }
    let steps: Option<Vec<_>> = head
        .get("do")?
        .as_array()?
        .iter()
        .map(|step| {
            if !only_keys(step, &["destroy"]) {
                return None;
            }
            let destroy = step.get("destroy")?;
            let target = if destroy.is_object() {
                if !only_keys(destroy, &["ref"]) {
                    return None;
                }
                objects(destroy.get("ref")?)?
            } else {
                objects(destroy)?
            };
            Some(json!({"op":"destroy","subjects":target}))
        })
        .collect();
    match steps?.as_slice() {
        [] => None,
        [step] => Some(step.clone()),
        steps => Some(json!({"op":"seq","steps":steps})),
    }
}

fn evolve(head: &Value, line: u64) -> Option<Value> {
    if !only_keys(head, &["cost"]) {
        return None;
    }
    let cost = head.get("cost")?;
    if !only_keys(cost, &["pp"]) {
        return None;
    }
    Some(
        json!({"line":line,"kind":"evolve","costs":[{"op":"pp","amount":cost.get("pp")?.as_u64()?}],"body":{"op":"evolve","subjects":"self"}}),
    )
}

pub(super) fn program(card: &Value) -> Option<Value> {
    let faces = card.get("faces")?.as_object()?;
    if faces.len() != 1 {
        return None;
    }
    let entries = faces.values().next()?.get("abilities")?.as_array()?;
    let mut abilities = Vec::new();
    let mut has_permission = false;
    for entry in entries {
        if !only_keys(
            entry,
            &[
                "line",
                "evolve",
                "keywords",
                "static",
                "fanfare",
                "lastword",
                "attack_time",
                "evolve_time",
                "sources",
                "section",
                "ports",
                "generated_by",
            ],
        ) || entry
            .get("section")
            .is_some_and(|section| !section.is_null())
            || entry
                .get("ports")
                .is_some_and(|ports| ports != &json!({"imports":{},"exports":{}}))
        {
            return None;
        }
        let line = entry.get("line")?.as_u64()?;
        let heads = [
            "evolve",
            "keywords",
            "static",
            "fanfare",
            "lastword",
            "attack_time",
            "evolve_time",
        ]
        .into_iter()
        .filter(|key| entry.get(*key).is_some())
        .collect::<Vec<_>>();
        let [kind] = heads.as_slice() else {
            return None;
        };
        match *kind {
            "evolve" => abilities.push(evolve(entry.get("evolve")?, line)?),
            "keywords" => {
                for keyword in entry.get("keywords")?.as_array()? {
                    let keyword = match keyword.as_str()? {
                        "ward" => "guard",
                        name @ ("rush" | "storm" | "aura" | "bane" | "assail" | "intimidate") => {
                            name
                        }
                        _ => return None,
                    };
                    abilities.push(
                        json!({"line":line,"kind":"static","body":{"op":"macro","name":keyword}}),
                    );
                }
            }
            "static" => {
                let declarations = restriction(entry.get("static")?, line)?;
                if declarations.is_empty() {
                    return None;
                }
                abilities.extend(declarations);
                has_permission = true;
            }
            head @ ("fanfare" | "lastword" | "attack_time" | "evolve_time") => {
                let event = match head {
                    "fanfare" => "enter",
                    "lastword" => "field_to_cemetery",
                    "attack_time" => "attack",
                    "evolve_time" => "evolve",
                    _ => return None,
                };
                abilities.push(json!({"line":line,"kind":"trigger","event":event,"subject":"self","body":body(entry.get(head)?)?}));
            }
            _ => return None,
        }
    }
    has_permission.then(|| json!({"status":"complete","abilities":abilities}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn card() -> Value {
        json!({"faces":{"base":{"abilities":[
            {"line":1_u8,"evolve":{"cost":{"pp":0_u8}}},
            {"line":2_u8,"static":{"while":{"cmp":{"left":{"count":{"who":"opp","zone":"field","kind":"follower"}},"op":"ge","right":2_u8}},"effects":[{"cannot":{"what":"attack","target":{"leader":"opp"}}}]}},
            {"line":3_u8,"fanfare":{"do":[{"destroy":{"ref":{"who":"all","zone":"field","kind":"follower","other":true}}}]}}
        ]}}})
    }
    #[test]
    fn a_complete_permission_card_preserves_its_condition_and_other_followers() {
        let output = program(&card()).unwrap();
        assert_eq!(
            output["abilities"][1]["body"]["condition"]["args"][0]["count"]["side"], "opponent",
            "the live condition keeps its player relation"
        );
        assert_eq!(
            output["abilities"][2]["body"]["subjects"],
            json!({"side":"both","zone":"field","type":"follower","other":true}),
            "the trigger preserves its complete simultaneous recipient set"
        );
    }
    #[test]
    fn unknown_trigger_limits_cannot_be_dropped() {
        let mut input = card();
        input["faces"]["base"]["abilities"][2]["fanfare"]["per_turn"] = json!(1_u8);
        assert!(
            program(&input).is_none(),
            "an unsupported limit rejects the complete projection"
        );
    }
    #[test]
    fn unknown_conditions_and_nonempty_ports_are_rejected() {
        let mut input = card();
        input["faces"]["base"]["abilities"][1]["static"]["while"] = json!({"awakened":true});
        assert!(
            program(&input).is_none(),
            "a live condition cannot disappear"
        );
        let mut port_input = card();
        port_input["faces"]["base"]["abilities"][0]["ports"] =
            json!({"imports":{"x":"objects"},"exports":{}});
        assert!(
            program(&port_input).is_none(),
            "bound data cannot disappear"
        );
    }
    #[test]
    fn every_supported_trigger_keeps_its_legacy_event() {
        for (head, event) in [
            ("fanfare", "enter"),
            ("lastword", "field_to_cemetery"),
            ("attack_time", "attack"),
            ("evolve_time", "evolve"),
        ] {
            let mut input = card();
            let entry = input["faces"]["base"]["abilities"][2]
                .as_object_mut()
                .unwrap();
            let steps = entry.remove("fanfare").unwrap();
            entry.insert(head.to_owned(), steps);
            let output = program(&input).unwrap();
            assert_eq!(
                output["abilities"][2]["event"], event,
                "historical timing follows the declared head"
            );
        }
    }
}
