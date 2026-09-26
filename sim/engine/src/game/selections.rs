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
    pub(super) fn resolution_optional(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        let mut choices = self.combine_selection_choices(
            &node["selection"],
            frame,
            vec![json!({"do":"resolve-choice","choice":"execute"})],
            true,
        )?;
        choices.push(json!({"do":"resolve-choice","choice":"decline"}));
        self.prompt(
            frame,
            choices,
            json!({"resume":"optional","then":node["then"],"selection":node["selection"]}),
        );
        self.set_prompt_side(node, frame);
        Ok(())
    }

    pub(super) fn combine_selection_choices(
        &self,
        spec: &Value,
        frame: &Frame,
        choices: Vec<Value>,
        require_minimum: bool,
    ) -> Result<Vec<Value>> {
        if spec.is_null() {
            return Ok(choices);
        }
        let min = if require_minimum {
            usize::try_from(self.number(&spec["min"], frame)?.max(0)).map_err(invalid)?
        } else {
            0
        };
        let subsets = self.resolution_subsets(spec, frame)?;
        let key = if spec["order"] == true {
            "order"
        } else {
            "select"
        };
        let mut combined = Vec::new();
        for choice in choices {
            for selected in subsets.iter().filter(|selected| selected.len() >= min) {
                let mut parameterized = choice.clone();
                parameterized[key] = json!(selected);
                combined.push(parameterized);
            }
        }
        Ok(combined)
    }

    pub(super) fn bind_resolution_selection(
        spec: &Value,
        decision: &Value,
        by: &str,
        frame: &mut Frame,
    ) {
        if spec.is_null() {
            return;
        }
        let declined = decision["choice"] == "decline";
        let key = if spec["order"] == true {
            "order"
        } else {
            "select"
        };
        let selected = if declined {
            Vec::new()
        } else {
            list(&decision[key])
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        };
        let name = string(&spec["bind"]);
        frame.bindings.insert(name.into(), selected);
        if !declined && spec["order"] == true {
            frame.ordering.insert(name.into(), by.into());
        } else {
            frame.ordering.remove(name);
        }
    }

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
