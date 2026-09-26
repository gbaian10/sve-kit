#![expect(
    clippy::indexing_slicing,
    reason = "Authored ability nodes and saved references use total JSON indexing."
)]
use serde_json::Value;

use super::{Game, list, string};
use crate::{EngineFailure, Result};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Granted abilities share the same authoritative object and effect history."
)]
impl Game {
    pub(super) fn check_granted_abilities(node: &Value) -> Result<()> {
        let Some(abilities) = node["abilities"].as_array() else {
            if node.get("during").is_some() {
                return Err(EngineFailure::Unsupported(
                    "future modification period requires a pure ability grant".into(),
                ));
            }
            return Ok(());
        };
        if node.get("until").is_some_and(|until| {
            !matches!(
                string(until),
                "game" | "permanent" | "end-of-turn" | "next-controller-end"
            )
        }) || ((node.get("until").is_some() || node.get("during").is_some())
            && [
                "power",
                "hp",
                "set_power",
                "set_hp",
                "keywords",
                "remove_abilities",
            ]
            .iter()
            .any(|key| node.get(*key).is_some()))
        {
            return Err(EngineFailure::Unsupported(
                "mixed or unknown temporary ability grant".into(),
            ));
        }
        for ability in abilities {
            if !matches!(string(&ability["kind"]), "static" | "trigger" | "activated")
                || ability
                    .get("active_zones")
                    .is_some_and(|zones| list(zones).iter().any(|zone| zone != "field"))
                || ability.get("limit").is_some()
            {
                return Err(EngineFailure::Unsupported(
                    "ability grant needs unsupported timing or copy limits".into(),
                ));
            }
        }
        Ok(())
    }

    pub(super) fn local_abilities(&self, id: &str) -> Result<Vec<Value>> {
        let mut granted = Vec::new();
        for entry in &self.state.continuous {
            if !self.continuous_applies(entry, id) || !self.continuous_window_active(entry)? {
                continue;
            }
            if entry["effect"]["remove_abilities"] == true {
                granted.clear();
            }
            for mut ability in list(&entry["effect"]["abilities"]) {
                let mut reference = entry["reference"].clone();
                if !reference.is_object() || reference["line"].is_null() {
                    return Err(EngineFailure::Unsupported(
                        "ability grant is missing the saved provider reference".into(),
                    ));
                }
                reference["line"] = ability["line"].clone();
                for key in ["section", "face", "keyword"] {
                    if let Some(value) = ability.get(key) {
                        reference[key] = value.clone();
                    }
                }
                ability["granted_reference"] = reference;
                granted.push(ability);
            }
        }
        let mut result = self.printed_abilities(id)?;
        result.extend(granted);
        Ok(result)
    }

    pub(super) fn current_activated_ability(
        &self,
        id: &str,
        reference: &Value,
    ) -> Result<Option<Value>> {
        let mut selected = None;
        for code in self.abilities(id)? {
            if !matches!(string(&code["kind"]), "activated" | "meal" | "ride")
                || !super::legal::reference_matches(reference, &self.reference(id, &code))
            {
                continue;
            }
            if selected.as_ref().is_some_and(|previous| *previous != code) {
                return Err(EngineFailure::Unsupported(
                    "ambiguous activated ability reference".into(),
                ));
            }
            selected = Some(code);
        }
        Ok(selected)
    }
}
