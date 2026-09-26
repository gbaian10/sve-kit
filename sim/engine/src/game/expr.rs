#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use alloc::collections::BTreeSet;

use serde_json::{Value, json};

use super::{Frame, Game, int, list, other, scalar, string};
use crate::{Result, invalid};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Rule domains share one private state and are split into focused modules."
)]
impl Game {
    pub(super) fn select(&self, selector: &Value, frame: &Frame) -> Result<Vec<String>> {
        if let Some(reference) = selector.as_str() {
            return self.reference_set(reference, frame);
        }
        if let Some(parts) = selector["union"].as_array() {
            let mut ids = Vec::new();
            for part in parts {
                for id in self.select(part, frame)? {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
            return Ok(ids);
        }
        if let Some(parts) = selector["difference"].as_array() {
            let mut ids = parts
                .first()
                .map_or_else(|| Ok(Vec::new()), |part| self.select(part, frame))?;
            for part in parts.iter().skip(1) {
                let remove = self.select(part, frame)?;
                ids.retain(|id| !remove.contains(id));
            }
            return Ok(ids);
        }
        let mut ids = Vec::new();
        let seats = self.seats(string(&selector["side"]), frame);
        for seat in seats {
            let zone = string(&selector["zone"]);
            let candidates = match zone {
                "leader" => vec![format!("{seat}.leader")],
                "any" => self
                    .state
                    .objects
                    .values()
                    .filter(|o| o.controller == seat)
                    .map(|o| o.id.clone())
                    .collect(),
                _ => self.zone_ids(&seat, zone),
            };
            let top = selector
                .get("top")
                .map(|v| self.number(v, frame))
                .transpose()?
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(usize::MAX);
            for id in candidates.into_iter().take(top) {
                if self.matches(&id, selector, frame)? {
                    ids.push(id);
                }
            }
        }
        Ok(ids)
    }

    fn reference_set(&self, reference: &str, frame: &Frame) -> Result<Vec<String>> {
        if reference == "self" {
            return Ok(vec![frame.source.clone()]);
        }
        if reference == "both.leaders" {
            return Ok(vec!["P1.leader".into(), "P2.leader".into()]);
        }
        if reference == "self.leader" {
            return Ok(vec![format!("{}.leader", frame.controller)]);
        }
        if reference == "opponent.leader" {
            return Ok(vec![format!("{}.leader", other(&frame.controller))]);
        }
        if let Some(base) = reference.strip_suffix(".leader") {
            return self
                .reference_set(base, frame)?
                .iter()
                .map(|id| self.object(id).map(|o| format!("{}.leader", o.controller)))
                .collect();
        }
        if let Some(index) = reference.strip_prefix("target.") {
            return Ok(list(&frame.decision["targets"][index])
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect());
        }
        if let Some(index) = reference.strip_prefix("cost.") {
            return Ok(list(&frame.decision["costs"][index])
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect());
        }
        Ok(frame.bindings.get(reference).cloned().unwrap_or_else(|| {
            if self.state.objects.contains_key(reference) {
                vec![reference.into()]
            } else {
                Vec::new()
            }
        }))
    }

    pub(super) fn matches(&self, id: &str, selector: &Value, frame: &Frame) -> Result<bool> {
        if selector.is_string()
            || selector.get("union").is_some()
            || selector.get("difference").is_some()
        {
            return Ok(self
                .select(selector, frame)?
                .iter()
                .any(|candidate| candidate == id));
        }
        let Some(object) = self.state.objects.get(id) else {
            return Ok(id.ends_with(".leader") && selector["zone"] == "leader");
        };
        if !self
            .seats(string(&selector["side"]), frame)
            .contains(&object.controller)
        {
            return Ok(false);
        }
        if selector["other"] == true && id == frame.source {
            return Ok(false);
        }
        let face = self.face(id)?;
        let card_type = string(&face["card_type"]);
        let typ = match string(&selector["type"]) {
            "follower" => "フォロワー",
            "amulet" => "アミュレット",
            "spell" => "スペル",
            "crest" => "クレスト",
            _ => "",
        };
        if !card_type.contains(typ) {
            return Ok(false);
        }
        if let Some(name) = selector["name"].as_str()
            && string(&face["name"]) != name
        {
            return Ok(false);
        }
        if let Some(part) = selector["name_contains"].as_str()
            && !string(&face["name"]).contains(part)
        {
            return Ok(false);
        }
        if let Some(name) = selector["not_name"].as_str() {
            let name = if name == "self" {
                string(&self.face(&frame.source)?["name"])
            } else {
                name
            };
            if string(&face["name"]) == name {
                return Ok(false);
            }
        }
        if let Some(trait_name) = selector["trait"].as_str()
            && !list(&face["traits"])
                .iter()
                .any(|v| string(v) == trait_name)
        {
            return Ok(false);
        }
        if let Some(keyword) = selector["keyword"].as_str()
            && !self.keywords(id)?.contains(keyword)
        {
            return Ok(false);
        }
        if let Some(condition) = selector.get("where") {
            let mut context = frame.clone();
            context.bindings.insert("item".into(), vec![id.into()]);
            if !self.truth(condition, &context)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn seats(&self, side: &str, frame: &Frame) -> Vec<String> {
        match side {
            "both" => vec!["P1".into(), "P2".into()],
            "opponent" => vec![other(&frame.controller).into()],
            "active" => vec![self.active().into()],
            "P1" | "P2" => vec![side.into()],
            _ => vec![frame.controller.clone()],
        }
    }

    pub(super) fn number(&self, expr: &Value, frame: &Frame) -> Result<i64> {
        self.eval(expr, frame).map(|value| int(&value))
    }
    pub(super) fn truth(&self, expr: &Value, frame: &Frame) -> Result<bool> {
        self.eval(expr, frame)
            .map(|value| value.as_bool().unwrap_or_else(|| int(&value) > 0))
    }

    pub(super) fn eval(&self, expr: &Value, frame: &Frame) -> Result<Value> {
        if let Some(path) = expr["read"].as_str() {
            return self.read(path, frame);
        }
        if let Some(selector) = expr.get("count") {
            if selector.is_object()
                && selector
                    .as_object()
                    .is_some_and(|m| m.keys().all(|key| matches!(key.as_str(), "zone" | "side")))
            {
                let count = self
                    .seats(string(&selector["side"]), frame)
                    .iter()
                    .map(|seat| self.zone_count(seat, string(&selector["zone"])))
                    .sum::<i64>();
                return Ok(json!(count));
            }
            return Ok(json!(self.select(selector, frame)?.len()));
        }
        if let Some(condition) = expr.get("if") {
            return self.eval(
                if self.truth(condition, frame)? {
                    &expr["then"]
                } else {
                    &expr["else"]
                },
                frame,
            );
        }
        if let Some(function) = expr["fn"].as_str() {
            let args = list(&expr["args"])
                .iter()
                .map(|arg| self.eval(arg, frame))
                .collect::<Result<Vec<_>>>()?;
            let a = args.first().cloned().unwrap_or(Value::Null);
            let b = args.get(1).cloned().unwrap_or(Value::Null);
            let n = int(&a);
            let m = int(&b);
            return Ok(match function {
                "add" => json!(n.saturating_add(m)),
                "sub" => json!(n.saturating_sub(m)),
                "mul" => json!(n.saturating_mul(m)),
                "div" => json!(n.checked_div(m).unwrap_or_default()),
                "min" => json!(n.min(m)),
                "max" => json!(n.max(m)),
                "eq" => json!(a == b),
                "ne" => json!(a != b),
                "lt" => json!(n < m),
                "le" => json!(n <= m),
                "gt" => json!(n > m),
                "ge" => json!(n >= m),
                "and" => json!(args.iter().all(|v| v.as_bool() == Some(true))),
                "or" => json!(args.iter().any(|v| v.as_bool() == Some(true))),
                "not" => json!(a.as_bool() != Some(true)),
                "in" => json!(list(&b).contains(&a)),
                "distinct" => json!(
                    list(&a)
                        .iter()
                        .map(Value::to_string)
                        .collect::<BTreeSet<_>>()
                        .len()
                ),
                "half_up" => json!(n.saturating_add(1).checked_div(2).unwrap_or_default()),
                _ => return Err(invalid(format!("unknown expression: {function}"))),
            });
        }
        Ok(expr.clone())
    }

    fn read(&self, path: &str, frame: &Frame) -> Result<Value> {
        if path == "x" {
            return Ok(frame.decision["x"].clone());
        }
        if let Some(rest) = path
            .strip_prefix("self.")
            .or_else(|| path.strip_prefix("opponent."))
        {
            let seat = if path.starts_with("opponent.") {
                other(&frame.controller)
            } else {
                &frame.controller
            };
            if let Some(counter) = rest.strip_prefix("turn.") {
                return Ok(json!(
                    self.state
                        .counters
                        .get(&format!("{seat}.{counter}"))
                        .copied()
                        .unwrap_or_default()
                ));
            }
            if let Some(property) = rest.strip_prefix("pp.") {
                return Ok(self.player(seat)?.pp[property].clone());
            }
            if rest == "leader.life" {
                return Ok(self.player(seat)?.leader["life"].clone());
            }
            if rest == "cemetery.cost" {
                return Ok(json!(
                    self.zone_ids(seat, "cemetery")
                        .iter()
                        .map(|id| self.face(id).map(|face| scalar(&face["cost"])))
                        .collect::<Result<Vec<_>>>()?
                ));
            }
        }
        let Some((reference, field)) = path.rsplit_once('.') else {
            return Ok(frame.decision[path].clone());
        };
        let ids = self.reference_set(reference, frame)?;
        let Some(id) = ids.first() else {
            return Ok(Value::Null);
        };
        if field == "original_hp" {
            return Ok(frame
                .captured
                .get(id)
                .map_or(Value::Null, |attrs| attrs["hp"].clone()));
        }
        if reference.starts_with("cost.")
            && let Some(attrs) = frame.captured.get(id)
            && let Some(value) = attrs.get(field)
        {
            return Ok(value.clone());
        }
        let object = self.object(id)?;
        if field == "zone" {
            return Ok(json!(object.zone));
        }
        if field == "class" {
            return Ok(self.face(id)?["card_class"].clone());
        }
        if field == "cost" {
            return Ok(json!(scalar(&self.face(id)?["cost"])));
        }
        Ok(object.state.get(field).cloned().unwrap_or_else(|| {
            self.face(id)
                .ok()
                .and_then(|face| face.get(field))
                .cloned()
                .unwrap_or(Value::Null)
        }))
    }

    pub(super) fn keywords(&self, id: &str) -> Result<BTreeSet<String>> {
        let object = self.object(id)?;
        let mut result: BTreeSet<String> = list(&object.state["keywords"])
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        for ability in self.abilities(id)? {
            if ability["body"]["op"] == "keyword" {
                result.insert(string(&ability["body"]["name"]).into());
            }
        }
        for source in self.field_ids() {
            for ability in self.abilities(&source)? {
                let body = &ability["body"];
                if body["op"] != "aura" {
                    continue;
                }
                let frame = self.frame_for(&source)?;
                if body
                    .get("condition")
                    .is_some_and(|c| !self.truth(c, &frame).unwrap_or(false))
                {
                    continue;
                }
                if self.matches(id, &body["subjects"], &frame)? {
                    for keyword in list(&body["keywords"]) {
                        result.insert(string(&keyword).into());
                    }
                }
            }
        }
        Ok(result)
    }
    pub(super) fn field_ids(&self) -> Vec<String> {
        ["P1", "P2"]
            .iter()
            .flat_map(|seat| self.zone_ids(seat, "field"))
            .collect()
    }
}
