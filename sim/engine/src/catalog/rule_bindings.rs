//! Resolve public references against the same private input before running rules.

#![expect(
    clippy::indexing_slicing,
    reason = "References are schema-validated before resolution."
)]

use alloc::collections::{BTreeMap, BTreeSet};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use super::{Catalog, yaml};
use crate::{EngineFailure, Result, invalid};

/// Identity-only projection of a face in the caller's exact snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceIdentity {
    /// Exact snapshot card number, without cross-region normalization.
    pub card_no: String,
    /// Index in that card's face list.
    pub face_ordinal: usize,
    /// Existing permanent face identifier.
    pub face_id: String,
    /// Existing effective rules-name identifier.
    pub rules_name_id: String,
    /// Adopted title code, when the face has a title.
    pub title_code: Option<String>,
}

/// Explicit input boundary; a regionless JSONL never implies an EN mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineIdentityInput {
    /// Resolve reviewed JP locators from this exact legacy JSONL and authored programs.
    LegacyJp,
    /// Resolve existing identifiers supplied by the same snapshot's projection.
    Resolved {
        /// Region of every projected member.
        region: String,
        /// Exact snapshot bytes, not a digest of display names.
        snapshot_sha256: String,
        /// Identity facts only; names are read from the snapshot.
        faces: Vec<FaceIdentity>,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
enum Background {
    #[default]
    Unbound,
    Bound(Value),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
enum ResolvedRulesName {
    Legacy(String),
    Permanent(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TemplateRef {
    pub(crate) card_no: String,
    pub(crate) face_ordinal: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Resource {
    names: BTreeSet<ResolvedRulesName>,
    template: Option<TemplateRef>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct RuleBindings {
    background: Background,
    titles: BTreeMap<String, BTreeSet<String>>,
    title_labels: BTreeMap<String, String>,
    resources: BTreeMap<String, Resource>,
    name_labels: BTreeMap<String, ResolvedRulesName>,
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut result = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        result.push(char::from(HEX[usize::from(byte >> 4_u8)]));
        result.push(char::from(HEX[usize::from(byte & 15)]));
    }
    result
}

fn text<'node>(node: &'node Value, key: &str) -> Result<&'node str> {
    node[key]
        .as_str()
        .ok_or_else(|| invalid("invalid identity field"))
}

fn array(node: &Value) -> Result<&Vec<Value>> {
    node.as_array()
        .ok_or_else(|| invalid("invalid identity array"))
}

fn unique_insert<K, V>(map: &mut BTreeMap<K, V>, key: K, value: V) -> Result<()>
where
    K: Ord,
{
    if map.insert(key, value).is_some() {
        return Err(invalid("duplicate engine binding key"));
    }
    Ok(())
}

struct Projection<'input> {
    catalog: &'input Catalog,
    region: &'input str,
    identity: &'input EngineIdentityInput,
    ids: BTreeMap<String, &'input FaceIdentity>,
    names: BTreeMap<String, String>,
}

impl<'input> Projection<'input> {
    fn new(
        catalog: &'input Catalog,
        rules: &'input Value,
        identity: &'input EngineIdentityInput,
        snapshot_hash: &str,
    ) -> Result<Self> {
        let region = text(&rules["scope"], "region")?;
        let mut ids = BTreeMap::new();
        let mut names = BTreeMap::new();
        match identity {
            EngineIdentityInput::LegacyJp => {
                if region != "jp" || rules["input"]["kind"] != "legacy-jp" {
                    return Err(invalid("legacy JP identity boundary mismatch"));
                }
            }
            EngineIdentityInput::Resolved {
                region: projected,
                snapshot_sha256,
                faces,
            } => {
                if projected != region
                    || snapshot_sha256 != snapshot_hash
                    || rules["input"]["kind"] != "resolved"
                {
                    return Err(invalid("resolved identity background mismatch"));
                }
                let mut locators = BTreeSet::new();
                let mut label_ids = BTreeMap::new();
                for face in faces {
                    let printed = catalog.face(&face.card_no, face.face_ordinal)?;
                    if face.face_id.is_empty()
                        || face.rules_name_id.is_empty()
                        || !locators.insert((&face.card_no, face.face_ordinal))
                    {
                        return Err(invalid("invalid or repeated projected identity"));
                    }
                    unique_insert(&mut ids, face.face_id.clone(), face)?;
                    let label = catalog.rules_label(&face.card_no, face.face_ordinal)?;
                    if names
                        .insert(face.rules_name_id.clone(), label.clone())
                        .is_some_and(|old| old != label)
                    {
                        return Err(invalid("ambiguous projected rules-name identity"));
                    }
                    if label_ids
                        .insert(label, &face.rules_name_id)
                        .is_some_and(|previous| previous != &face.rules_name_id)
                    {
                        return Err(invalid("ambiguous projected rules-name label"));
                    }
                    if face.title_code.as_ref().is_some_and(String::is_empty)
                        || (face.title_code.is_some() && printed["title"].as_str().is_none())
                    {
                        return Err(invalid("invalid projected title identity"));
                    }
                }
            }
        }
        Ok(Self {
            catalog,
            region,
            identity,
            ids,
            names,
        })
    }

    fn face(&self, reference: &Value) -> Result<TemplateRef> {
        if text(reference, "region")? != self.region {
            return Err(invalid("cross-region engine face reference"));
        }
        let locator = match text(reference, "kind")? {
            "legacy-face" => TemplateRef {
                card_no: text(reference, "card_no")?.into(),
                face_ordinal: reference["face_ordinal"]
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(|| invalid("invalid face ordinal"))?,
            },
            "face-id" => {
                let projected = self
                    .ids
                    .get(text(reference, "face_id")?)
                    .ok_or_else(|| invalid("missing projected face identity"))?;
                TemplateRef {
                    card_no: projected.card_no.clone(),
                    face_ordinal: projected.face_ordinal,
                }
            }
            _ => return Err(invalid("unknown face reference")),
        };
        self.catalog.face(&locator.card_no, locator.face_ordinal)?;
        Ok(locator)
    }
}

impl RuleBindings {
    pub(crate) fn resolve(
        catalog: &Catalog,
        snapshot: &str,
        authored_digest: &str,
        document: &str,
        identity: &EngineIdentityInput,
    ) -> Result<Self> {
        let rules: Value =
            yaml(document).map_err(|_private_error| invalid("invalid engine rules YAML"))?;
        if !rules["format"].is_u64() {
            return Err(invalid("engine rules format must be an integer"));
        }
        let schema: Value =
            serde_json::from_str(include_str!("../../../../dsl/engine-rules.schema.json"))
                .map_err(invalid)?;
        jsonschema::options()
            .should_validate_formats(true)
            .build(&schema)
            .map_err(invalid)?
            .validate(&rules)
            .map_err(|_private_error| invalid("invalid engine rules schema"))?;
        let snapshot_hash = digest(snapshot.as_bytes());
        if rules["input"]["snapshot_sha256"] != snapshot_hash {
            return Err(invalid("engine rules snapshot hash mismatch"));
        }
        let region = text(&rules["scope"], "region")?;
        let projection = Projection::new(catalog, &rules, identity, &snapshot_hash)?;
        let mut result = Self::default();
        let mut used_evidence = BTreeSet::new();
        let mut evidence = BTreeSet::new();
        for entry in array(&rules["evidence"])? {
            if !evidence.insert(text(entry, "id")?.to_owned()) {
                return Err(invalid("duplicate engine evidence key"));
            }
        }
        result.resolve_titles(&rules, &projection, &mut used_evidence)?;
        result.resolve_resources(&rules, &projection, &mut used_evidence)?;
        if used_evidence != evidence {
            return Err(invalid("engine evidence closure mismatch"));
        }
        for capabilities in result.titles.values() {
            if capabilities.contains("opening_ex_resource")
                && !result.resources.contains_key("lesson_item")
            {
                return Err(invalid("opening capability requires lesson resource"));
            }
            if capabilities.contains("opening_ex_resource") {
                result.template("lesson_item")?;
            }
        }
        result.validate_projected_titles(&projection)?;
        for program in catalog.programs.values() {
            result.validate_dependencies(program)?;
        }
        result.background = Background::Bound(
            json!({"region":region,"kind":rules["input"]["kind"],"snapshot_sha256":snapshot_hash,
            "settings_sha256":digest(document.as_bytes()),"authored_sha256":authored_digest,
            "identity_sha256":digest(&serde_json::to_vec(identity).map_err(invalid)?),
            "schema_versions":{
                "effects":{"kind":"effect_set","format":1_u8,"schema_id":"urn:sve-kit:effects:astra:1"},
                "keywords":{"kind":"keyword_registry","format":1_u8,"schema_id":"urn:sve-kit:effects:astra:1"},
                "engine_rules":{"kind":"engine_rules","format":1_u8,"schema_id":schema["$id"]}},
            "schemas_sha256":digest(&serde_json::to_vec(&json!({"effects":include_str!("../../../../dsl/effects.schema.json"),"engine_rules":include_str!("../../../../dsl/engine-rules.schema.json")})).map_err(invalid)?),
            "scope":rules["scope"]}),
        );
        Ok(result)
    }

    fn validate_projected_titles(&self, projection: &Projection<'_>) -> Result<()> {
        if let EngineIdentityInput::Resolved { faces, .. } = projection.identity {
            for face in faces {
                let label = projection.catalog.face(&face.card_no, face.face_ordinal)?["title"]
                    .as_str()
                    .unwrap_or_default();
                if let Some(code) = &face.title_code
                    && self.title_labels.get(label) != Some(code)
                {
                    return Err(invalid("projected title disagrees with resolved binding"));
                }
            }
        }
        Ok(())
    }

    fn validate_dependencies(&self, node: &Value) -> Result<()> {
        match node {
            Value::Object(fields) => {
                let role = node["resource_role"]
                    .as_str()
                    .or_else(|| match node["op"].as_str() {
                        Some("lesson") => Some("lesson_item"),
                        Some("eat") => Some("meal_item"),
                        Some("stack") => Some("stack_base"),
                        _ if node["kind"] == "ride" => Some("drive_point"),
                        _ => None,
                    });
                if let Some(role) = role {
                    if !self.resources.contains_key(role) {
                        return Err(invalid("program requires an unbound resource role"));
                    }
                    if node["op"] == "stack" {
                        self.template(role)?;
                    }
                }
                for child in fields.values() {
                    self.validate_dependencies(child)?;
                }
            }
            Value::Array(values) => {
                for child in values {
                    self.validate_dependencies(child)?;
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        }
        Ok(())
    }

    fn resolve_titles(
        &mut self,
        rules: &Value,
        projection: &Projection<'_>,
        used_evidence: &mut BTreeSet<String>,
    ) -> Result<()> {
        for entry in array(&rules["titles"])? {
            let code = text(entry, "title_code")?;
            let anchor = projection.face(&entry["anchor"])?;
            let label = projection
                .catalog
                .face(&anchor.card_no, anchor.face_ordinal)?["title"]
                .as_str()
                .filter(|label| !label.is_empty())
                .ok_or_else(|| invalid("title anchor has no title"))?
                .to_owned();
            if let EngineIdentityInput::Resolved { faces, .. } = projection.identity {
                let projected = faces.iter().find(|face| {
                    face.card_no == anchor.card_no && face.face_ordinal == anchor.face_ordinal
                });
                if projected.and_then(|face| face.title_code.as_deref()) != Some(code) {
                    return Err(invalid("title code disagrees with projected anchor"));
                }
            }
            let mut capabilities = BTreeSet::new();
            for capability in array(&entry["capabilities"])? {
                if !capabilities.insert(text(capability, "kind")?.to_owned()) {
                    return Err(invalid("duplicate engine capability"));
                }
            }
            unique_insert(&mut self.titles, code.to_owned(), capabilities)?;
            unique_insert(&mut self.title_labels, label, code.to_owned())?;
            used_evidence.extend(
                array(&entry["evidence"])?
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned),
            );
        }
        Ok(())
    }

    fn resolve_resources(
        &mut self,
        rules: &Value,
        projection: &Projection<'_>,
        used_evidence: &mut BTreeSet<String>,
    ) -> Result<()> {
        for entry in array(&rules["resources"])? {
            let role = text(entry, "role")?;
            let mut keys = BTreeSet::new();
            let mut labels = BTreeSet::new();
            for reference in array(&entry["names"])? {
                let (key, label) = match text(reference, "kind")? {
                    "legacy-name" => {
                        let face = projection.face(&reference["face"])?;
                        let label = if reference["name_source"] == "rules" {
                            projection
                                .catalog
                                .rules_label(&face.card_no, face.face_ordinal)?
                        } else {
                            text(
                                projection.catalog.face(&face.card_no, face.face_ordinal)?,
                                "name",
                            )?
                            .to_owned()
                        };
                        (ResolvedRulesName::Legacy(reference.to_string()), label)
                    }
                    "rules-name-id" => {
                        if text(reference, "region")? != projection.region {
                            return Err(invalid("cross-region rules-name reference"));
                        }
                        let id = text(reference, "rules_name_id")?;
                        let label = projection
                            .names
                            .get(id)
                            .ok_or_else(|| invalid("missing projected rules-name identity"))?
                            .clone();
                        (ResolvedRulesName::Permanent(id.into()), label)
                    }
                    _ => return Err(invalid("unknown name reference")),
                };
                if label.is_empty() || !keys.insert(key.clone()) || !labels.insert(label.clone()) {
                    return Err(invalid("empty or duplicate resolved role name"));
                }
                unique_insert(&mut self.name_labels, label, key)?;
            }
            let template = entry
                .get("template")
                .map(|reference| projection.face(reference))
                .transpose()?;
            if let Some(face) = &template {
                let printed = projection.catalog.face(&face.card_no, face.face_ordinal)?;
                if !printed["card_type"]
                    .as_str()
                    .is_some_and(|kind| kind.contains("トークン"))
                {
                    return Err(invalid("resource template is not a token"));
                }
                projection
                    .catalog
                    .program(&face.card_no)
                    .map_err(|_private_error| {
                        invalid("resource template program is not executable")
                    })?;
                if !labels.contains(
                    &projection
                        .catalog
                        .rules_label(&face.card_no, face.face_ordinal)?,
                ) {
                    return Err(invalid("resource template name differs from role"));
                }
            }
            unique_insert(
                &mut self.resources,
                role.to_owned(),
                Resource {
                    names: keys,
                    template,
                },
            )?;
            used_evidence.extend(
                array(&entry["evidence"])?
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned),
            );
        }
        Ok(())
    }

    pub(crate) fn title_code(&self, code: &str, label: &str) -> Result<Option<String>> {
        if code.is_empty() && label.is_empty() {
            return Ok(None);
        }
        let from_label = if label.is_empty() {
            None
        } else {
            Some(
                self.title_labels
                    .get(label)
                    .ok_or_else(|| EngineFailure::Unsupported("unregistered title label".into()))?
                    .as_str(),
            )
        };
        let selected = if code.is_empty() {
            from_label
        } else {
            Some(code)
        };
        if selected.is_none_or(|selected_code| !self.titles.contains_key(selected_code)) {
            return Err(EngineFailure::Unsupported("unregistered title code".into()));
        }
        if !code.is_empty() && from_label.is_some_and(|resolved| resolved != code) {
            return Err(invalid("title code and label disagree"));
        }
        Ok(selected.map(str::to_owned))
    }

    pub(crate) fn capability(&self, code: &str, capability: &str) -> bool {
        self.titles
            .get(code)
            .is_some_and(|set| set.contains(capability))
    }

    pub(crate) fn matches_resource(&self, role: &str, labels: &[String]) -> Result<bool> {
        let resource = self
            .resources
            .get(role)
            .ok_or_else(|| EngineFailure::Unsupported(format!("unbound resource role: {role}")))?;
        Ok(labels
            .iter()
            .filter_map(|label| self.name_labels.get(label))
            .any(|key| resource.names.contains(key)))
    }

    pub(crate) fn legacy_resource(&self, label: &str) -> Option<&str> {
        let key = self.name_labels.get(label)?;
        self.resources
            .iter()
            .find(|(_, resource)| resource.names.contains(key))
            .map(|(role, _)| role.as_str())
    }

    pub(crate) fn template(&self, role: &str) -> Result<TemplateRef> {
        self.resources
            .get(role)
            .and_then(|r| r.template.clone())
            .ok_or_else(|| EngineFailure::Unsupported(format!("unbound resource template: {role}")))
    }
}
