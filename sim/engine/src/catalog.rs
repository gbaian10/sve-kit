//! Immutable card snapshot and schema-validated authored effects.

#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]

use alloc::collections::{BTreeMap, BTreeSet};
use core::iter::once;
use jsonschema::validator_for;
use std::fs::{read_dir, read_to_string};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{EngineFailure, Result, invalid};

mod semantics;

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
    /// Schema-valid programs that failed the load-time semantic checks, with every
    /// finding as `file:line: message`. They are never executed.
    #[serde(default)]
    pub(crate) rejected: BTreeMap<String, Vec<String>>,
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
        catalog.validate_keyword_names()?;
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
                if catalog.programs.contains_key(number) || catalog.rejected.contains_key(number) {
                    return Err(invalid(format!("duplicate program: {number}")));
                }
                let card_type = catalog
                    .face(number, 0)
                    .ok()
                    .and_then(|face| face["card_type"].as_str())
                    .unwrap_or_default()
                    .to_owned();
                let findings = semantics::check_program(&program, &catalog.keywords, &card_type);
                if findings.is_empty() {
                    catalog.programs.insert(number.clone(), program);
                } else {
                    let located = findings
                        .iter()
                        .map(|finding| {
                            format!(
                                "{name}:{}: {}",
                                locate(text, number, &finding.needle),
                                finding.message
                            )
                        })
                        .collect();
                    catalog.rejected.insert(number.clone(), located);
                }
            }
        }
        catalog.validate_token_templates()?;
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

    /// Cards whose authored program was rejected at load, with located findings.
    #[must_use]
    pub const fn rejections(&self) -> &BTreeMap<String, Vec<String>> {
        &self.rejected
    }

    pub(crate) fn program(&self, number: &str) -> Result<&Value> {
        if let Some(findings) = self.rejected.get(number) {
            return Err(EngineFailure::Unsupported(format!(
                "card program rejected at load: {number}: {}",
                findings.join("; ")
            )));
        }
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

    fn validate_token_templates(&self) -> Result<()> {
        let mut names: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
        for (number, program) in &self.programs {
            let face = self.face(number, 0)?;
            let token = face["card_type"]
                .as_str()
                .is_some_and(|kind| kind.contains("トークン"));
            if program["token_template"] == true && (!token || program["status"] != "complete") {
                return Err(invalid("token template requires a complete token program"));
            }
            if token {
                let name = program["rules_name"]
                    .as_str()
                    .or_else(|| face["name"].as_str())
                    .ok_or_else(|| invalid("token requires a rules name"))?;
                names.entry(name.into()).or_default().push(program);
            }
        }
        for (name, programs) in names {
            if programs.len() > 1
                && programs
                    .iter()
                    .filter(|program| program["token_template"] == true)
                    .count()
                    != 1
            {
                return Err(invalid(format!(
                    "ambiguous token requires one explicit template: {name}"
                )));
            }
        }
        Ok(())
    }

    pub(crate) fn token_print(&self, name: &str) -> Result<String> {
        let mut candidates = Vec::new();
        for (number, program) in &self.programs {
            let face = self.face(number, 0)?;
            if face["card_type"]
                .as_str()
                .is_some_and(|kind| kind.contains("トークン"))
                && (face["name"] == name || program["rules_name"] == name)
            {
                candidates.push((number, program));
            }
        }
        let selected = if candidates.len() == 1 {
            candidates.first()
        } else {
            candidates
                .iter()
                .find(|(_, program)| program["token_template"] == true)
        }
        .ok_or_else(|| EngineFailure::Unsupported(format!("no authored token template: {name}")))?;
        self.program(selected.0)?;
        Ok(selected.0.clone())
    }

    fn validate_keyword_names(&self) -> Result<()> {
        let mut owners = BTreeMap::new();
        for (id, entry) in &self.keywords {
            let aliases = entry["aliases"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str);
            for name in once(id.as_str()).chain(entry["ja"].as_str()).chain(aliases) {
                if owners.insert(name, id).is_some_and(|other| other != id) {
                    return Err(invalid(format!("ambiguous keyword name: {name}")));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn import_attributes(&self, attrs: &mut Value) -> Result<()> {
        if let Some(keys) = attrs["keywords"].as_array() {
            attrs["keywords"] = json!(
                keys.iter()
                    .map(|key| self.keyword_id(key.as_str().unwrap_or_default()))
                    .collect::<Vec<_>>()
            );
        }
        if let Some(counters) = attrs["counters"].as_object() {
            let mut normalized = BTreeMap::new();
            for (key, value) in counters {
                if normalized
                    .insert(self.keyword_id(key), value.clone())
                    .is_some()
                {
                    return Err(invalid("multiple names supplied for the same counter"));
                }
            }
            attrs["counters"] = json!(normalized);
        }
        Ok(())
    }

    pub(crate) fn export_counters(&self, attrs: &mut Value) {
        if let Some(counters) = attrs["counters"].as_object() {
            attrs["counters"] = json!(
                counters
                    .iter()
                    .map(|(key, value)| {
                        let translated = self.keyword_name(key);
                        (
                            if translated.is_empty() {
                                key.as_str()
                            } else {
                                translated
                            },
                            value.clone(),
                        )
                    })
                    .collect::<BTreeMap<_, _>>()
            );
        }
    }

    fn keyword_id(&self, name: &str) -> String {
        self.keywords
            .iter()
            .find(|(_, value)| {
                value["ja"] == name
                    || value["aliases"]
                        .as_array()
                        .is_some_and(|aliases| aliases.iter().any(|alias| alias == name))
            })
            .map_or_else(|| name.to_owned(), |(key, _)| key.clone())
    }
}

/// 1-based line of `needle` inside the card's block, or of the card key itself.
fn locate(text: &str, number: &str, needle: &str) -> usize {
    let key = format!("  {number}:");
    let lines = text.lines().collect::<Vec<_>>();
    let Some(start) = lines.iter().position(|line| *line == key) else {
        return 0;
    };
    let block = lines
        .iter()
        .enumerate()
        .skip(start.saturating_add(1))
        .take_while(|(_, line)| !line.starts_with("  ") || line.starts_with("   "));
    block
        .filter(|(_, line)| line.contains(needle))
        .map(|(index, _)| index)
        .next()
        .unwrap_or(start)
        .saturating_add(1)
}

pub(crate) fn yaml<'de, T>(text: &'de str) -> Result<T>
where
    T: Deserialize<'de>,
{
    serde_saphyr::from_str_with_options(text, serde_saphyr::options! { strict_booleans: true })
        .map_err(invalid)
}

fn validate_choice_labels(value: &Value, registry: &BTreeMap<String, Value>) -> Result<()> {
    let Some(labels) = value["labels"].as_array() else {
        return Ok(());
    };
    if value["modes"].as_array().map(Vec::len) != Some(labels.len()) {
        return Err(invalid("choice labels must match the number of modes"));
    }
    let mut unique = BTreeSet::new();
    for label in labels {
        let mut localized = label.clone();
        if let Some(keyword) = label["keyword"].as_str() {
            let name = registry
                .get(keyword)
                .and_then(|entry| entry["ja"].as_str())
                .filter(|name| !name.is_empty())
                .ok_or_else(|| invalid("choice label needs a registered keyword"))?;
            localized["keyword"] = json!(name);
        }
        if !unique.insert(localized.to_string()) {
            return Err(invalid(
                "choice labels must remain distinct after localization",
            ));
        }
    }
    Ok(())
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
    if value["op"] == "choice" {
        validate_choice_labels(value, registry)?;
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
