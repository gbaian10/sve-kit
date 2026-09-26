use alloc::collections::BTreeSet;
use serde_json::{Value, json};

use super::legal::{permutations, subsets};
use super::{Frame, Game, list, string};
use crate::{EngineFailure, Result, invalid};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Play and resolution selections share their set and ordering constraints."
)]
impl Game {
    pub(super) fn selection_constraints(
        &self,
        spec: &Value,
        selected: &[String],
        groups: &Value,
    ) -> Result<bool> {
        if let Some(property) = spec["distinct_by"].as_str() {
            if property != "name" {
                return Err(EngineFailure::Unsupported(format!(
                    "selection distinct property: {property}"
                )));
            }
            let mut names = BTreeSet::new();
            for id in selected {
                if !names.insert(self.card_name(id)?) {
                    return Ok(false);
                }
            }
        }
        for other_group in list(&spec["different_from"]) {
            if list(&groups[string(&other_group)])
                .iter()
                .any(|id| selected.iter().any(|chosen| chosen == string(id)))
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn selection_subsets(
        &self,
        ids: &[String],
        min: usize,
        max: usize,
        spec: &Value,
        groups: &Value,
    ) -> Result<Vec<Vec<String>>> {
        let mut result = Vec::new();
        for chosen in subsets(ids, min, max) {
            if !self.selection_constraints(spec, &chosen, groups)? {
                continue;
            }
            if spec["order"] == true {
                result.extend(
                    permutations(&chosen.iter().map(|id| json!(id)).collect::<Vec<_>>())
                        .into_iter()
                        .map(|order| order.iter().map(|id| string(id).to_owned()).collect()),
                );
            } else {
                result.push(chosen);
            }
        }
        Ok(result)
    }

    pub(super) fn resolution_subsets(
        &self,
        node: &Value,
        frame: &Frame,
    ) -> Result<Vec<Vec<String>>> {
        let mut ids = self.select(&node["select"], frame)?;
        ids.sort();
        let max = usize::try_from(self.number(&node["max"], frame)?.max(0))
            .map_err(invalid)?
            .min(ids.len());
        let min = usize::try_from(self.number(&node["min"], frame)?.max(0)).map_err(invalid)?;
        let mut candidates = Vec::new();
        for selected in self.selection_subsets(&ids, 0, max, node, &Value::Null)? {
            let mut context = frame.clone();
            context
                .bindings
                .insert(string(&node["bind"]).into(), selected.clone());
            if let Some(constraint) = node.get("constraint")
                && !self.truth(constraint, &context)?
            {
                continue;
            }
            candidates.push(selected);
        }
        let possible = candidates.iter().map(Vec::len).max().unwrap_or_default();
        // Required resolution selections perform as much as all constraints permit (1.3.2).
        candidates.retain(|choice| choice.len() >= min.min(possible));
        if candidates.is_empty() {
            return Err(EngineFailure::Unsupported(
                "unsatisfiable resolution selection constraint".into(),
            ));
        }
        Ok(candidates)
    }
}
