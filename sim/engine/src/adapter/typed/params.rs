//! Exam decision parameters, consumed in printed order as serial questions arrive.

use serde_json::Value;

pub(super) fn labels(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Numbered parameter lists (`{1: [...], 2: [...]}`) in key order.
fn numbered(value: &Value) -> Vec<Vec<String>> {
    let Some(map) = value.as_object() else {
        return Vec::new();
    };
    let mut entries: Vec<(u64, Vec<String>)> = map
        .iter()
        .map(|(key, items)| (key.parse().unwrap_or(u64::MAX), labels(items)))
        .collect();
    entries.sort_by_key(|(key, _)| *key);
    entries.into_iter().map(|(_, items)| items).collect()
}

/// `additional` first, then `mode_<n>` in mode order.
fn optional_order(key: &str) -> (u64, String) {
    key.strip_prefix("mode_")
        .and_then(|mode| mode.parse().ok())
        .map_or_else(|| (0, key.to_owned()), |mode| (mode, String::new()))
}

/// Parameters of one exam decision, consumed in printed order.
#[derive(Debug, Default)]
pub(super) struct Params {
    pub(super) targets: Vec<Vec<String>>,
    pub(super) costs: Vec<Vec<String>>,
    pub(super) distribute: Vec<Value>,
    pub(super) optional: Vec<bool>,
    pub(super) options: Option<Vec<u64>>,
    pub(super) x: Option<u64>,
    pub(super) pay: Value,
    pub(super) select: Option<Vec<String>>,
    pub(super) order: Option<Vec<String>>,
    pub(super) choice: Option<bool>,
    pub(super) proceed: Option<bool>,
    pub(super) position: Option<String>,
}

impl Params {
    pub(super) fn of(decision: &Value) -> Self {
        let mut distribute: Vec<(u64, Value)> = decision["distribute"]
            .as_object()
            .map(|map| {
                map.iter()
                    .map(|(key, value)| (key.parse().unwrap_or(u64::MAX), value.clone()))
                    .collect()
            })
            .unwrap_or_default();
        distribute.sort_by_key(|(key, _)| *key);
        let mut costs = if decision["costs"] == "decline" {
            Vec::new()
        } else {
            numbered(&decision["costs"])
        };
        // Each optional payment is its own answer, asked in printed order.
        let mut entries: Vec<(&String, &Value)> = decision["optional_costs"]
            .as_object()
            .map(|map| map.iter().collect())
            .unwrap_or_default();
        entries.sort_by_key(|(key, _)| optional_order(key));
        let mut optional = Vec::new();
        for (_, answer) in entries {
            optional.push(answer != "decline");
            for value in answer
                .as_object()
                .into_iter()
                .flat_map(|content| content.values())
            {
                let items = value
                    .as_str()
                    .map_or_else(|| labels(value), |label| vec![label.to_owned()]);
                if !items.is_empty() {
                    costs.push(items);
                }
            }
        }
        Self {
            targets: numbered(&decision["targets"]),
            costs,
            distribute: distribute.into_iter().map(|(_, value)| value).collect(),
            optional,
            options: decision["options"]
                .as_array()
                .map(|items| items.iter().filter_map(Value::as_u64).collect()),
            x: decision["x"]
                .as_u64()
                .or_else(|| decision["declare"].as_u64()),
            pay: decision["pay"].clone(),
            select: decision.get("select").map(labels),
            order: decision.get("order").and_then(|order| {
                order
                    .as_array()
                    .is_some_and(|items| items.iter().all(Value::is_string))
                    .then(|| labels(order))
            }),
            choice: decision["choice"]
                .as_str()
                .map(|choice| choice == "execute")
                .or_else(|| (decision["costs"] == "decline").then_some(false)),
            proceed: decision["continue"].as_bool(),
            position: decision["position"].as_str().map(str::to_owned),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.targets.is_empty()
            && self.costs.is_empty()
            && self.distribute.is_empty()
            && self.optional.is_empty()
            && self.options.is_none()
            && self.x.is_none()
            && self.select.is_none()
            && self.order.is_none()
            && self.choice.is_none()
            && self.proceed.is_none()
            && self.pay.is_null()
            && self.position.is_none()
    }
}
