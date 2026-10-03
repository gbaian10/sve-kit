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
        if let Some(source) = selector.get("from") {
            let top = selector
                .get("top")
                .map(|value| self.number(value, frame))
                .transpose()?
                .map_or(usize::MAX, |n| {
                    usize::try_from(n.max(0)).unwrap_or(usize::MAX)
                });
            let mut ids = Vec::new();
            for id in self.select(source, frame)?.into_iter().take(top) {
                if self.matches(&id, selector, frame)? {
                    ids.push(id);
                }
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
        if reference == "event.target" || reference == "event.subject" {
            return Ok(frame.event[if reference == "event.target" {
                "target"
            } else {
                "subject_id"
            }]
            .as_str()
            .map(str::to_owned)
            .into_iter()
            .collect());
        }
        if reference == "linked" {
            return Ok(self.object(&frame.source)?.state["equipped_to"]
                .as_str()
                .map(str::to_owned)
                .into_iter()
                .collect());
        }
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
                .chosen_references(base, frame)?
                .iter()
                .map(|id| {
                    frame
                        .captured
                        .get(id)
                        .and_then(|attrs| attrs["controller"].as_str())
                        .map_or_else(
                            || self.object(id).map(|o| format!("{}.leader", o.controller)),
                            |controller| Ok(format!("{controller}.leader")),
                        )
                })
                .collect();
        }
        if let Some(index) = reference.strip_prefix("target.") {
            return Ok(list(&frame.decision["targets"][index])
                .iter()
                .filter_map(Value::as_str)
                .filter(|id| {
                    frame.captured.get(*id).is_none_or(|attrs| {
                        self.state.objects.get(*id).is_some_and(|object| {
                            Some(object.generation) == attrs["generation"].as_u64()
                        })
                    })
                })
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

    pub(super) fn chosen_references(&self, reference: &str, frame: &Frame) -> Result<Vec<String>> {
        if let Some(key) = reference.strip_prefix("target.") {
            return Ok(list(&frame.decision["targets"][key])
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect());
        }
        self.reference_set(reference, frame)
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
            return Ok(id.strip_suffix(".leader").is_some_and(|seat| {
                selector["zone"] == "leader"
                    && self
                        .seats(string(&selector["side"]), frame)
                        .iter()
                        .any(|candidate| candidate == seat)
            }));
        };
        if let Some(zone) = selector["zone"].as_str()
            && zone != "any"
            && zone != object.zone
        {
            return Ok(false);
        }
        if (selector.get("from").is_none() || selector.get("side").is_some())
            && !self
                .seats(string(&selector["side"]), frame)
                .contains(&object.controller)
        {
            return Ok(false);
        }
        if selector["other"] == true && id == frame.source {
            return Ok(false);
        }
        let face = self.face(id)?;
        let card_type = self.object_type(id)?;
        let typ = match string(&selector["type"]) {
            "follower" | "evolved_follower" => "フォロワー",
            "amulet" => "アミュレット",
            "spell" => "スペル",
            "crest" => "クレスト",
            _ => "",
        };
        if selector["type"] == "evolved_follower" && !card_type.contains("エボルヴ") {
            return Ok(false);
        }
        if !card_type.contains(typ) {
            return Ok(false);
        }
        if !self.matches_names(id, selector, frame)? {
            return Ok(false);
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
        if let Some(event) = selector["ability_event"].as_str()
            && !self
                .abilities(id)?
                .iter()
                .any(|ability| ability["kind"] == event || ability["event"] == event)
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

    fn matches_names(&self, id: &str, selector: &Value, frame: &Frame) -> Result<bool> {
        let names = self.card_names(id)?;
        if let Some(role) = selector["resource_role"].as_str()
            && !self.catalog.rule_bindings.matches_resource(role, &names)?
        {
            return Ok(false);
        }
        if let Some(name) = selector["name"].as_str()
            && !names.iter().any(|own| own == name)
        {
            return Ok(false);
        }
        if let Some(part) = selector["name_contains"].as_str()
            && !names.iter().any(|own| own.contains(part))
        {
            return Ok(false);
        }
        if let Some(name) = selector["not_name"].as_str() {
            let name = if name == "self" {
                string(&self.face(&frame.source)?["name"])
            } else {
                name
            };
            if names.iter().any(|own| own == name) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn single_rules_name_matches_role(&self, id: &str, role: &str) -> Result<bool> {
        self.catalog
            .rule_bindings
            .matches_resource(role, &[self.card_name(id)?])
    }

    /// The rules name plus every alias (Q1145: an alias is an additional card name).
    fn card_names(&self, id: &str) -> Result<Vec<String>> {
        let mut names = vec![self.card_name(id)?];
        names.extend(self.name_aliases(id)?);
        Ok(names)
    }

    /// "これは『X』でもある" (static `name_alias`): extra names while in the given zone.
    fn name_aliases(&self, id: &str) -> Result<Vec<String>> {
        let zone = self.object(id)?.zone.clone();
        // A card without an executable program has no alias to offer; its other
        // abilities still fail closed when something tries to use them.
        // Only the card's own printed abilities: an alias is never granted, and
        // `abilities()` evaluates auras whose conditions may compare names again.
        let abilities = match self.printed_abilities(id) {
            Ok(abilities) => abilities,
            Err(crate::EngineFailure::Unsupported(_)) => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        Ok(abilities
            .iter()
            .map(|ability| &ability["body"])
            .filter(|body| {
                body["op"] == "name_alias"
                    && body["subjects"] == "self"
                    && (body["while_zone"] == "any" || body["while_zone"] == zone.as_str())
            })
            .filter_map(|body| body["name"].as_str().map(str::to_owned))
            .collect())
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
        if expr.get("at").is_some() {
            return frame.frozen.get(&expr.to_string()).map_or_else(
                || self.eval(&expr["value"], frame),
                |value| Ok(value.clone()),
            );
        }
        if let Some(selector) = expr.get("values") {
            let mut values = Vec::new();
            for id in self.select(selector, frame)? {
                let mut context = frame.clone();
                context.bindings.insert("item".into(), vec![id]);
                let value = self.read(&format!("item.{}", string(&expr["field"])), &context)?;
                if !value.is_null() {
                    values.push(value);
                }
            }
            return Ok(json!(values));
        }
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
                "min" if a.is_array() => json!(list(&a).iter().map(int).min().unwrap_or_default()),
                "max" if a.is_array() => json!(list(&a).iter().map(int).max().unwrap_or_default()),
                "min" => json!(n.min(m)),
                "max" => json!(n.max(m)),
                "sum" => json!(list(&a).iter().map(int).fold(0_i64, i64::saturating_add)),
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

    pub(super) fn freeze(&self, code: &Value, phase: &str, frame: &mut Frame) -> Result<()> {
        if code["at"] == phase {
            let value = self.eval(&code["value"], frame)?;
            frame.frozen.insert(code.to_string(), value);
            return Ok(());
        }
        match code {
            Value::Array(values) => {
                for value in values {
                    self.freeze(value, phase, frame)?;
                }
            }
            Value::Object(fields) => {
                for (key, value) in fields {
                    if key != "abilities" && !(code["op"] == "delay" && key == "body") {
                        self.freeze(value, phase, frame)?;
                    }
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        }
        Ok(())
    }

    fn read(&self, path: &str, frame: &Frame) -> Result<Value> {
        if let Some(value) = frame.values.get(path) {
            return Ok(value.clone());
        }
        if let Some(value) = self.related_player_count(path, frame)? {
            return Ok(value);
        }
        if let Some(path) = path.strip_prefix("event.") {
            return Ok(path
                .split('.')
                .fold(&frame.event, |value, key| &value[key])
                .clone());
        }
        if let Some(counter) = path.strip_prefix("self.counters.") {
            return Ok(self.object(&frame.source)?.state["counters"][counter].clone());
        }
        if path == "x" {
            return Ok(frame.decision["x"].clone());
        }
        if path == "turn.phase" {
            return Ok(self.state.turn["phase"].clone());
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
            if rest == "is_active" {
                return Ok(json!(seat == self.active()));
            }
            if rest == "combo" {
                let played = self
                    .state
                    .counters
                    .get(&format!("{seat}.cards_played"))
                    .copied()
                    .unwrap_or_default();
                let starting = frame.decision["do"] == "play"
                    && self
                        .object(&frame.source)
                        .is_ok_and(|object| matches!(object.zone.as_str(), "hand" | "ex"));
                return Ok(json!(played.saturating_add(i64::from(starting))));
            }
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
        self.read_object_attribute(reference, field, frame)
    }

    fn related_player_count(&self, path: &str, frame: &Frame) -> Result<Option<Value>> {
        let relation = ["controller", "owner"].iter().find_map(|role| {
            path.split_once(&format!(".{role}."))
                .map(|(reference, property)| (reference, *role, property))
        });
        let Some((reference, role, property)) = relation else {
            return Ok(None);
        };
        let zone = property.strip_suffix("_count").filter(|zone| {
            matches!(
                *zone,
                "field" | "hand" | "deck" | "ex" | "cemetery" | "banish" | "evolve_deck"
            )
        });
        let zone = zone.ok_or_else(|| {
            crate::EngineFailure::Unsupported(format!("related player property: {property}"))
        })?;
        let ids = self.chosen_references(reference, frame)?;
        let Some(id) = ids.first() else {
            return Ok(Some(Value::Null));
        };
        let seat = if let Some(seat) = id.strip_suffix(".leader") {
            seat
        } else {
            let object = self.object(id)?;
            if role == "owner" {
                &object.owner
            } else {
                if frame
                    .captured
                    .get(id)
                    .is_some_and(|attrs| attrs["generation"].as_u64() != Some(object.generation))
                {
                    return Err(crate::EngineFailure::Unsupported(
                        "departed object controller needs a retained last-known relation".into(),
                    ));
                }
                &object.controller
            }
        };
        Ok(Some(json!(self.zone_count(seat, zone))))
    }

    fn read_object_attribute(&self, reference: &str, field: &str, frame: &Frame) -> Result<Value> {
        let ids = if field == "original_hp" {
            self.chosen_references(reference, frame)?
        } else {
            self.reference_set(reference, frame)?
        };
        let Some(id) = ids.first() else {
            return Ok(Value::Null);
        };
        if reference == "self"
            && field != "zone"
            && !frame.event.is_null()
            && let Some(attrs) = frame.captured.get(id)
            && Some(self.object(id)?.generation) != attrs["generation"].as_u64()
            && let Some(value) = attrs.get(field)
        {
            return Ok(value.clone());
        }
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
        if field == "entered_by_play" || field == "entered_by_effect" {
            return Ok(json!(
                object.state["entered_by"]
                    == if field == "entered_by_play" {
                        "play"
                    } else {
                        "effect"
                    }
            ));
        }
        if field == "zone" {
            return Ok(json!(object.zone));
        }
        if field == "class" {
            return Ok(self.face(id)?["card_class"].clone());
        }
        if field == "cost" {
            return Ok(json!(scalar(&self.face(id)?["cost"])));
        }
        if field == "current_cost" {
            return Ok(json!(object.state.get("cost").map_or_else(
                || self.face(id).map_or(0, |face| scalar(&face["cost"])),
                int
            )));
        }
        if field == "id" {
            return Ok(json!(id));
        }
        if field == "token" {
            return Ok(json!(
                string(&self.face(id)?["card_type"]).contains("トークン")
            ));
        }
        Ok(object.state.get(field).cloned().unwrap_or_else(|| {
            self.face(id)
                .ok()
                .and_then(|face| face.get(field))
                .cloned()
                .unwrap_or(Value::Null)
        }))
    }

    pub(super) fn field_ids(&self) -> Vec<String> {
        ["P1", "P2"]
            .iter()
            .flat_map(|seat| self.zone_ids(seat, "field"))
            .collect()
    }
}
