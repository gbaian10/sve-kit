#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use alloc::collections::BTreeSet;
use serde_json::{Value, json};

use super::{Game, Pending, list, string};
use crate::{EngineFailure, Result};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Keyword availability and provenance share the authoritative game state."
)]
impl Game {
    fn keyword_grants(&self, id: &str) -> Result<Vec<(String, Value)>> {
        let mut grants = Vec::new();
        for source in self.field_ids() {
            for ability in self.abilities(&source)? {
                let body = &ability["body"];
                if ability["kind"] != "static" || body["op"] != "aura" {
                    continue;
                }
                let frame = self.frame_for(&source)?;
                if let Some(condition) = body.get("condition")
                    && !self.truth(condition, &frame)?
                {
                    continue;
                }
                if self.matches(id, &body["subjects"], &frame)? {
                    grants.push((source.clone(), ability));
                }
            }
        }
        Ok(grants)
    }

    pub(super) fn keywords(&self, id: &str) -> Result<BTreeSet<String>> {
        let object = self.object(id)?;
        let mut result: BTreeSet<String> = list(&object.state["keywords"])
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        if !list(&object.state["links"]["出走"]).is_empty() {
            result.insert("rush".into());
        }
        for ability in self.abilities(id)? {
            if ability["body"]["op"] == "drive" {
                result.insert(string(&ability["keyword"]).into());
            }
            if ability["kind"] == "static" && ability["body"]["op"] == "keyword" {
                result.insert(string(&ability["body"]["name"]).into());
            }
        }
        for (_, ability) in self.keyword_grants(id)? {
            for keyword in list(&ability["body"]["keywords"]) {
                result.insert(string(&keyword).into());
            }
        }
        Ok(result)
    }

    pub(super) fn granted_reference(&self, id: &str, reference: &Value) -> Result<Value> {
        if reference["line"].is_null() {
            return Err(EngineFailure::Unsupported(
                "granted keyword needs its authored source reference".into(),
            ));
        }
        let mut result = reference.clone();
        if reference["source"] != id && reference["card"].is_null() {
            result["card"] = json!(self.object(string(&reference["source"]))?.card);
        }
        result["source"] = json!(id);
        Ok(result)
    }

    fn keyword_reference(&self, id: &str, keyword: &str) -> Result<Value> {
        for ability in self.abilities(id)? {
            if ability["kind"] == "static"
                && ability["body"]["op"] == "keyword"
                && ability["body"]["name"] == keyword
            {
                return Ok(self.reference(id, &ability));
            }
        }
        for (source, ability) in self.keyword_grants(id)? {
            if list(&ability["body"]["keywords"]).contains(&json!(keyword)) {
                return self.granted_reference(id, &self.reference(&source, &ability));
            }
        }
        for entry in self.state.continuous.iter().rev() {
            if self.continuous_applies(entry, id)
                && list(&entry["effect"]["keywords"]).contains(&json!(keyword))
                && !entry["reference"]["line"].is_null()
            {
                return Ok(entry["reference"].clone());
            }
        }
        Err(EngineFailure::Unsupported(format!(
            "keyword source reference unavailable: {keyword}"
        )))
    }

    pub(super) fn drain_triggers(&self, hits: &[Value], cause: &Value) -> Result<Vec<Pending>> {
        let mut pending = Vec::new();
        for hit in hits {
            let source = string(&hit["source"]);
            if hit["attack"] != true || !self.keywords(source)?.contains("drain") {
                continue;
            }
            let mut reference = self.keyword_reference(source, "drain")?;
            reference["keyword"] = json!(self.catalog.keyword_name("drain"));
            reference["rule"] = json!("12.13.2");
            let mut context = self.start_frame(source, reference.clone(), &Value::Null)?;
            context.event = json!({"amount":hit["amount"]});
            pending.push(Pending {
                controller: context.controller.clone(),
                code: json!({"kind":"trigger","line":reference["line"],"body":{"op":"modify","subjects":"self.leader","hp":{"read":"event.amount"}}}),
                reference,
                event: json!({"damaged":hit["target"]}),
                source: source.into(),
                cause: cause.clone(),
                retained: false,
                id: None,
                context: Some(context),
                alternatives: Vec::new(),
            });
        }
        Ok(pending)
    }
}
