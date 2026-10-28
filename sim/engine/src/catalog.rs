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

pub(crate) mod current;
mod rule_bindings;
mod selection;
mod semantics;

pub use rule_bindings::{EngineIdentityInput, FaceIdentity};
pub use selection::SelectedEffects;

/// Card facts from the sole versioned card database snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    /// Exact official printed card number.
    pub number: String,
    /// Every face, in snapshot order.
    pub faces: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TokenDefinition {
    pub(crate) name: String,
    pub(crate) name_override: bool,
    pub(crate) template: bool,
    pub(crate) complete: bool,
}

/// Shared immutable catalogue, serializable into a self-contained server save.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Catalog {
    pub(crate) cards: BTreeMap<String, Card>,
    pub(crate) programs: BTreeMap<String, Value>,
    pub(crate) keywords: BTreeMap<String, Value>,
    pub(crate) token_definitions: BTreeMap<String, TokenDefinition>,
    pub(crate) rule_bindings: rule_bindings::RuleBindings,
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
        Self::load_with_identity(snapshot, authored, &EngineIdentityInput::LegacyJp)
    }

    /// Loads explicit identity input; the default loader is restricted to legacy JP.
    ///
    /// # Errors
    /// Rejects inconsistent input, missing bindings and the ordinary catalogue errors.
    pub fn load_with_identity(
        snapshot: &Path,
        authored: &Path,
        identity: &EngineIdentityInput,
    ) -> Result<Self> {
        for area in [
            "rules/effects-index.yaml",
            "rules/author-effects",
            "rules/effect-bodies",
        ] {
            if authored.join(area).try_exists().map_err(invalid)? {
                return Err(EngineFailure::Unsupported(
                    "current author inventory requires Catalog::from_current_selection".into(),
                ));
            }
        }
        let mut paths: Vec<_> = read_dir(authored.join("rules/effects"))
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
        Self::from_documents_with_rules(
            &read_to_string(snapshot).map_err(invalid)?,
            &read_to_string(authored.join("rules/keywords.yaml")).map_err(invalid)?,
            &documents,
            &read_to_string(authored.join("rules/engine/index.yaml")).map_err(invalid)?,
            identity,
        )
    }

    /// Consume `CurrentAuthorSources.selection_input()` from the trusted Python reader.
    ///
    /// # Errors
    /// Rejects the entire selection on any rejected card, overlap or D0 failure;
    /// callers never receive a partial selection or an implicit prototype fallback.
    pub fn from_current_selection(
        snapshot: &str,
        keywords: &str,
        input: &Value,
    ) -> Result<SelectedEffects> {
        selection::select(snapshot, keywords, input, None)
    }

    /// Load current effects together with the snapshot's existing title and resource rules.
    ///
    /// # Errors
    /// Rejects invalid selections and unknown or inconsistent rule identities.
    pub fn from_current_selection_with_rules(
        snapshot: &str,
        keywords: &str,
        input: &Value,
        rules: &str,
        identity: &EngineIdentityInput,
    ) -> Result<SelectedEffects> {
        selection::select(snapshot, keywords, input, Some((rules, identity)))
    }

    /// Resolves settings against caller-supplied bytes without filesystem access.
    ///
    /// # Errors
    /// Rejects unknown, ambiguous, missing or inconsistent identities and templates.
    pub fn from_documents_with_rules(
        snapshot: &str,
        keywords: &str,
        documents: &[(String, String)],
        rules: &str,
        identity: &EngineIdentityInput,
    ) -> Result<Self> {
        let mut catalog = Self::from_documents(snapshot, keywords, documents)?;
        catalog.bind_rules(snapshot, keywords, documents, rules, identity)?;
        Ok(catalog)
    }

    fn bind_rules(
        &mut self,
        snapshot: &str,
        keywords: &str,
        documents: &[(String, String)],
        rules: &str,
        identity: &EngineIdentityInput,
    ) -> Result<()> {
        let authored = serde_json::to_vec(&json!({"keywords":keywords,"documents":documents.iter().map(|(_, text)| text).collect::<Vec<_>>()})).map_err(invalid)?;
        self.rule_bindings = rule_bindings::RuleBindings::resolve(
            self,
            snapshot,
            &rule_bindings::digest(&authored),
            rules,
            identity,
        )?;
        Ok(())
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
        let catalog = Self::read_documents(snapshot, keywords, documents)?;
        catalog.validate_token_templates()?;
        Ok(catalog)
    }

    fn read_documents(
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
        if !registry["format"].is_u64() {
            return Err(invalid("keyword registry format must be an integer"));
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
            if !document["format"].is_u64() {
                return Err(invalid("effect document format must be an integer"));
            }
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
                    .validate(&json!({"format":1_u8,"kind":"effect_set","cards":{number:program}}))
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
        for (number, program) in &catalog.programs {
            let face = catalog.face(number, 0)?;
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
                    .ok_or_else(|| invalid("token requires a rules name"))?
                    .to_owned();
                catalog.token_definitions.insert(
                    number.clone(),
                    TokenDefinition {
                        name,
                        name_override: program["rules_name"].is_string(),
                        template: program["token_template"] == true,
                        complete: program["status"] == "complete",
                    },
                );
            }
        }
        Ok(catalog)
    }

    pub(super) fn rules_label(&self, card: &str, ordinal: usize) -> Result<String> {
        let face = self.face(card, ordinal)?;
        self.token_definitions
            .get(card)
            .filter(|definition| definition.name_override)
            .map(|definition| definition.name.as_str())
            .or_else(|| {
                let program = self.programs.get(card)?;
                program["rules_name"].as_str()
            })
            .or_else(|| face["name"].as_str())
            .map(str::to_owned)
            .ok_or_else(|| invalid("referenced face has no rules name"))
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
        let mut names: BTreeMap<&str, Vec<&TokenDefinition>> = BTreeMap::new();
        for definition in self.token_definitions.values() {
            names.entry(&definition.name).or_default().push(definition);
        }
        for (name, definitions) in names {
            if definitions.len() > 1
                && definitions
                    .iter()
                    .filter(|definition| definition.template)
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
        for (number, definition) in &self.token_definitions {
            if definition.name == name || self.face(number, 0)?["name"] == name {
                candidates.push((number, definition));
            }
        }
        let selected = if candidates.len() == 1 {
            candidates.first()
        } else {
            candidates
                .iter()
                .find(|(_, definition)| definition.template)
        }
        .ok_or_else(|| EngineFailure::Unsupported(format!("no authored token template: {name}")))?;
        if !selected.1.complete {
            return Err(EngineFailure::Unsupported(format!(
                "partial card program: {}",
                selected.0
            )));
        }
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
