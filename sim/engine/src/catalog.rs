//! Immutable snapshot facts and battle rule names.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::sync::Arc;
use core::iter::once;

use serde::{Deserialize, Serialize, de::IgnoredAny};
use serde_json::{Value, json};

use crate::{EngineFailure, Result, invalid};

pub(crate) mod current;
mod rule_bindings;
mod selection;

pub use rule_bindings::{EngineIdentityInput, FaceIdentity};
pub(crate) use selection::RuntimeCards;
pub use selection::{CurrentSelectionLoader, ProgramPrinting, SelectedEffects, SnapshotPrograms};

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
    pub(crate) alias_of: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeywordRegistry {
    format: Value,
    kind: String,
    #[serde(default)]
    records: Vec<RegisteredKeyword>,
    #[serde(default)]
    rule_names: BTreeMap<String, RuleName>,
    #[serde(default, rename = "decision")]
    _decision: Option<IgnoredAny>,
    #[serde(default, rename = "suppressions")]
    _suppressions: Option<IgnoredAny>,
}

#[derive(Deserialize)]
struct RegisteredKeyword {
    code: String,
    rule_name: Option<RuleName>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleName {
    ja: String,
    #[serde(rename = "rule", skip_serializing_if = "Option::is_none")]
    _rule: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<RuleNameRole>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    aliases: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum RuleNameRole {
    Keyword,
    AbilityLabel,
    Counter,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureCards {
    format: Value,
    kind: String,
    cards: BTreeMap<String, FixtureCard>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureCard {
    status: FixtureStatus,
    rules_name: Option<String>,
    #[serde(default)]
    token_template: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FixtureStatus {
    Complete,
    Partial,
    Unsupported,
}

/// Shared immutable catalogue, serializable into a self-contained server save.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Catalog {
    pub(crate) cards: BTreeMap<String, Arc<Card>>,
    fixtures: BTreeMap<String, FixtureCard>,
    keywords: BTreeMap<String, RuleName>,
    pub(crate) token_definitions: BTreeMap<String, TokenDefinition>,
    pub(crate) rule_bindings: rule_bindings::RuleBindings,
    /// Current selection rejections carried for unresolved source rulings.
    #[serde(default)]
    pub(crate) rejected: BTreeMap<String, Vec<String>>,
    /// Current construction names per printing number and face, from the selection.
    /// A collab printing's site name is display text, not the name rules compare.
    #[serde(default)]
    pub(crate) rules_names: BTreeMap<String, Vec<Option<String>>>,
}

impl Catalog {
    /// Consume `CurrentAuthorSources.selection_from_results()` from the trusted Python reader.
    ///
    /// # Errors
    /// Rejects the entire selection on any rejected card, overlap or D0 failure;
    /// callers never receive a partial selection.
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
    pub fn from_fixture_documents_with_rules(
        snapshot: &str,
        keywords: &str,
        documents: &[(String, String)],
        rules: &str,
        identity: &EngineIdentityInput,
    ) -> Result<Self> {
        let mut catalog = Self::from_fixture_documents(snapshot, keywords, documents)?;
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

    /// Load snapshot facts and rule names without binding any card programs.
    ///
    /// # Errors
    /// Rejects malformed or duplicated snapshot facts and rule names.
    pub fn from_snapshot(snapshot: &str, keywords: &str) -> Result<Self> {
        Self::read_snapshot_using(snapshot, keywords, Self::read_snapshot_card)
    }

    /// Load synthetic card metadata for an in-process rules fixture.
    ///
    /// # Errors
    /// Rejects invalid snapshot facts, rule names or fixture metadata.
    pub fn from_fixture_documents(
        snapshot: &str,
        keywords: &str,
        documents: &[(String, String)],
    ) -> Result<Self> {
        let mut catalog = Self::from_snapshot(snapshot, keywords)?;
        catalog.load_fixtures(documents)?;
        catalog.validate_token_templates()?;
        Ok(catalog)
    }

    fn load_fixtures(&mut self, documents: &[(String, String)]) -> Result<()> {
        for (name, text) in documents {
            let document: FixtureCards =
                yaml(text).map_err(|error| invalid(format!("{name}: {error}")))?;
            if document.format.as_u64() != Some(1) || document.kind != "fixture_cards" {
                return Err(invalid("invalid fixture card envelope"));
            }
            for (number, fixture) in document.cards {
                self.add_fixture(number, fixture)?;
            }
        }
        Ok(())
    }

    fn read_snapshot_card(line: &str) -> Result<Arc<Card>> {
        serde_json::from_str(line).map(Arc::new).map_err(invalid)
    }

    fn read_snapshot_using(
        snapshot: &str,
        keywords: &str,
        mut read_card: impl FnMut(&str) -> Result<Arc<Card>>,
    ) -> Result<Self> {
        let mut catalog = Self::default();
        for line in snapshot.lines() {
            let card = read_card(line)?;
            let key = card.number.clone();
            if catalog.cards.insert(key.clone(), card).is_some() {
                return Err(EngineFailure::Invalid(format!(
                    "duplicate snapshot card: {key}"
                )));
            }
        }
        let registry: KeywordRegistry = yaml(keywords)?;
        if registry.format.as_u64() != Some(1) || registry.kind != "keyword_registry" {
            return Err(invalid("invalid keyword registry envelope"));
        }
        catalog.keywords = registry.rule_names;
        let mut codes = BTreeSet::new();
        for record in registry.records {
            if !codes.insert(record.code.clone()) || catalog.keywords.contains_key(&record.code) {
                return Err(invalid("duplicate keyword code"));
            }
            if let Some(name) = record.rule_name {
                // The existing battle state and its fixtures identify Ward as guard.
                let key = if record.code == "ward" {
                    "guard".to_owned()
                } else {
                    record.code
                };
                if catalog.keywords.insert(key, name).is_some() {
                    return Err(invalid("duplicate battle rule name"));
                }
            }
        }
        catalog.validate_keyword_names()?;
        Ok(catalog)
    }

    fn add_fixture(&mut self, number: String, fixture: FixtureCard) -> Result<()> {
        let face = self.face(&number, 0)?;
        let token = face["card_type"]
            .as_str()
            .is_some_and(|kind| kind.contains("トークン"));
        let complete = fixture.status == FixtureStatus::Complete;
        if fixture.token_template && (!token || !complete) {
            return Err(invalid("token template requires a complete token fixture"));
        }
        if token {
            let name = fixture
                .rules_name
                .as_deref()
                .or_else(|| face["name"].as_str())
                .ok_or_else(|| invalid("token requires a rules name"))?
                .to_owned();
            self.token_definitions.insert(
                number.clone(),
                TokenDefinition {
                    name,
                    name_override: fixture.rules_name.is_some(),
                    template: fixture.token_template,
                    alias_of: None,
                    complete,
                },
            );
        }
        if self.fixtures.insert(number, fixture).is_some() {
            return Err(invalid("duplicate fixture card"));
        }
        Ok(())
    }

    pub(super) fn rules_label(&self, card: &str, ordinal: usize) -> Result<String> {
        let face = self.face(card, ordinal)?;
        self.token_definitions
            .get(card)
            .filter(|definition| definition.name_override)
            .map(|definition| definition.name.as_str())
            .or_else(|| self.fixtures.get(card)?.rules_name.as_deref())
            .or_else(|| self.rules_names.get(card)?.get(ordinal)?.as_deref())
            .or_else(|| face["name"].as_str())
            .map(str::to_owned)
            .ok_or_else(|| invalid("referenced face has no rules name"))
    }

    pub(crate) fn declarable_names(&self) -> Result<Vec<String>> {
        let mut names = BTreeSet::new();
        for (number, card) in &self.cards {
            for face in 0..card.faces.len() {
                names.insert(self.rules_label(number, face)?);
            }
        }
        if names.is_empty() {
            return Err(invalid("snapshot has no declarable card names"));
        }
        Ok(names.into_iter().collect())
    }

    pub(crate) fn face(&self, number: &str, index: usize) -> Result<&Value> {
        self.cards
            .get(number)
            .and_then(|card| card.faces.get(index))
            .ok_or_else(|| invalid(format!("unknown card face: {number}/{index}")))
    }

    pub(crate) fn require_fixture_card(&self, number: &str) -> Result<()> {
        if self
            .fixtures
            .get(number)
            .is_some_and(|fixture| fixture.status == FixtureStatus::Complete)
        {
            return Ok(());
        }
        Err(EngineFailure::Unsupported(format!(
            "card lacks complete fixture metadata: {number}"
        )))
    }

    pub(crate) fn keyword_name(&self, id: &str) -> &str {
        self.keywords.get(id).map_or("", |entry| entry.ja.as_str())
    }

    pub(crate) fn registered_counter_name(&self, name: &str) -> Option<&str> {
        self.keywords.iter().find_map(|(code, entry)| {
            (entry.role == Some(RuleNameRole::Counter) && (code == name || entry.ja == name))
                .then_some(entry.ja.as_str())
        })
    }

    /// The name a counter change is recorded under: its registered name, or its identifier
    /// when the registry has none, so that no change is ever recorded under an empty name.
    pub(crate) fn counter_name<'name>(&'name self, id: &'name str) -> &'name str {
        let name = self.keyword_name(id);
        if name.is_empty() { id } else { name }
    }

    fn validate_token_templates(&self) -> Result<()> {
        let mut names: BTreeMap<&str, Vec<&TokenDefinition>> = BTreeMap::new();
        for definition in self
            .token_definitions
            .values()
            .filter(|definition| definition.alias_of.is_none())
        {
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
            if definition.alias_of.is_none()
                && (definition.name == name || self.face(number, 0)?["name"] == name)
            {
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
            for name in once(id.as_str())
                .chain(once(entry.ja.as_str()))
                .chain(entry.aliases.iter().map(String::as_str))
            {
                if name.is_empty() {
                    return Err(invalid("empty keyword name"));
                }
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

    pub(crate) fn keyword_id(&self, name: &str) -> String {
        self.keywords
            .iter()
            .find(|(_, value)| value.ja == name || value.aliases.iter().any(|alias| alias == name))
            .map_or_else(|| name.to_owned(), |(key, _)| key.clone())
    }
}

pub(crate) fn yaml<'de, T>(text: &'de str) -> Result<T>
where
    T: Deserialize<'de>,
{
    serde_saphyr::from_str_with_options(text, serde_saphyr::options! { strict_booleans: true })
        .map_err(invalid)
}
