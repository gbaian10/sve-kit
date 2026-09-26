//! Immutable card snapshot and schema-validated authored effects.

#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]

use alloc::collections::BTreeMap;
use jsonschema::validator_for;
use std::fs::{read_dir, read_to_string};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{EngineFailure, Result, invalid};

/// Card facts from the sole versioned card database snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    /// Exact official printed card number.
    pub number: String,
    /// Every face, in snapshot order.
    pub faces: Vec<Value>,
}

/// Shared immutable catalogue, serializable into a self-contained server save.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Catalog {
    pub(crate) cards: BTreeMap<String, Card>,
    pub(crate) programs: BTreeMap<String, Value>,
    pub(crate) keywords: BTreeMap<String, Value>,
}

impl Catalog {
    /// Loads snapshot facts, validates YAML against the authoritative schema, then expands macros.
    ///
    /// # Errors
    /// Rejects missing files, duplicate cards, unknown macros and invalid programs.
    pub fn load(snapshot: &Path, authored: &Path) -> Result<Self> {
        let mut paths: Vec<_> = read_dir(authored.join("effects"))
            .map_err(invalid)?
            .map(|entry| entry.map(|entry| entry.path()).map_err(invalid))
            .collect::<Result<_>>()?;
        paths.sort();
        let documents = paths
            .iter()
            .filter(|path| path.extension().is_some_and(|ext| ext == "yaml"))
            .map(|path| {
                Ok((
                    path.display().to_string(),
                    read_to_string(path).map_err(invalid)?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        Self::from_documents(
            &read_to_string(snapshot).map_err(invalid)?,
            &read_to_string(authored.join("keywords.yaml")).map_err(invalid)?,
            &documents,
        )
    }

    /// Portable loader for caller-supplied snapshot and YAML bytes, including browser hosts.
    ///
    /// # Errors
    /// The same validation failures as the filesystem loader.
    pub fn from_documents(
        snapshot: &str,
        keywords: &str,
        documents: &[(String, String)],
    ) -> Result<Self> {
        let mut catalog = Self::default();
        for line in snapshot.lines() {
            let card: Card = serde_json::from_str(line).map_err(invalid)?;
            let key = card.number.clone();
            if catalog.cards.insert(key.clone(), card).is_some() {
                return Err(EngineFailure::Invalid(format!(
                    "duplicate snapshot card: {key}"
                )));
            }
        }
        let registry: Value = yaml(keywords)?;
        if registry["version"] != "astra/1" {
            return Err(invalid("unknown keyword registry version"));
        }
        catalog.keywords = serde_json::from_value(registry["keywords"].clone()).map_err(invalid)?;
        let schema: Value = serde_json::from_str(include_str!("../../../dsl/effects.schema.json"))
            .map_err(invalid)?;
        let registry_schema = json!({"$ref":"#/$defs/keywordRegistry","$defs":schema["$defs"]});
        validator_for(&registry_schema)
            .map_err(invalid)?
            .validate(&registry)
            .map_err(invalid)?;
        let validator = validator_for(&schema).map_err(invalid)?;
        for (name, text) in documents {
            let document: Value = yaml(text)?;
            validator
                .validate(&document)
                .map_err(|error| EngineFailure::Invalid(format!("{name}: {error}")))?;
            let entries = document["cards"]
                .as_object()
                .ok_or_else(|| invalid("cards map"))?;
            for (number, program) in entries {
                if !catalog.cards.contains_key(number) {
                    return Err(invalid(format!("unknown snapshot card: {number}")));
                }
                let program = expand(program, &catalog.keywords, 0)?;
                validator
                    .validate(&json!({"version":"astra/1","cards":{number:program}}))
                    .map_err(invalid)?;
                if catalog.programs.insert(number.clone(), program).is_some() {
                    return Err(invalid(format!("duplicate program: {number}")));
                }
            }
        }
        Ok(catalog)
    }

    /// Number of authored card programs, including explicit partial programs.
    #[must_use]
    pub fn authored_count(&self) -> usize {
        self.programs.len()
    }

    pub(crate) fn face(&self, number: &str, index: usize) -> Result<&Value> {
        self.cards
            .get(number)
            .and_then(|card| card.faces.get(index))
            .ok_or_else(|| invalid(format!("unknown card face: {number}/{index}")))
    }

    pub(crate) fn program(&self, number: &str) -> Result<&Value> {
        let program = self
            .programs
            .get(number)
            .ok_or_else(|| EngineFailure::Unsupported(format!("no authored program: {number}")))?;
        if program["status"] != "complete" {
            return Err(EngineFailure::Unsupported(format!(
                "partial card program: {number}"
            )));
        }
        Ok(program)
    }

    pub(crate) fn keyword_name(&self, id: &str) -> &str {
        self.keywords
            .get(id)
            .and_then(|entry| entry["ja"].as_str())
            .unwrap_or("")
    }
}

pub(crate) fn yaml<'de, T>(text: &'de str) -> Result<T>
where
    T: Deserialize<'de>,
{
    serde_saphyr::from_str_with_options(text, serde_saphyr::options! { strict_booleans: true })
        .map_err(invalid)
}

fn expand(value: &Value, registry: &BTreeMap<String, Value>, depth: usize) -> Result<Value> {
    if depth > 64 {
        return Err(invalid("macro expansion depth exceeds 64"));
    }
    if value["op"] == "macro" {
        let name = value["name"].as_str().unwrap_or_default();
        let entry = registry
            .get(name)
            .ok_or_else(|| invalid(format!("unknown macro: {name}")))?;
        let expansion = entry
            .get("expansion")
            .ok_or_else(|| invalid(format!("ability label has no macro expansion: {name}")))?;
        return expand(expansion, registry, depth.saturating_add(1));
    }
    if value["op"] == "keyword"
        && !registry.contains_key(value["name"].as_str().unwrap_or_default())
    {
        return Err(invalid("unknown keyword id"));
    }
    match value {
        Value::Array(values) => values
            .iter()
            .map(|v| expand(v, registry, depth))
            .collect::<Result<Vec<_>>>()
            .map(Value::Array),
        Value::Object(values) => values
            .iter()
            .map(|(key, member)| Ok((key.clone(), expand(member, registry, depth)?)))
            .collect::<Result<_>>()
            .map(Value::Object),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => Ok(value.clone()),
    }
}
