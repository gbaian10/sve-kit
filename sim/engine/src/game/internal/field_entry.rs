//! Retained entry facts belong to the current generation, independently of turn history.

use super::{Game, Result, invalid};
use crate::game::{Frame, Object};
use crate::ir::{EntryMethod, RuntimeAbilitySource, TraitId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::game) struct FieldEntry {
    generation: u64,
    method: RecordedMethod,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RecordedMethod {
    Play,
    AbilityPut,
    RulePut,
}
impl From<EntryMethod> for RecordedMethod {
    fn from(method: EntryMethod) -> Self {
        match method {
            EntryMethod::Play => Self::Play,
            EntryMethod::AbilityPut => Self::AbilityPut,
            EntryMethod::RulePut => Self::RulePut,
        }
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Movement commits entry facts alongside the new field generation."
)]
impl Game {
    pub(in crate::game::internal) fn internal_entry_effect_source(
        &self,
        frame: &Frame,
    ) -> Option<RuntimeAbilitySource> {
        // A suspended search may resume after the instruction's active-source marker was restored.
        frame
            .entry_source
            .as_ref()
            .map(|source| source.source)
            .or(self.card_runtime.active_source)
    }

    pub(in crate::game::internal) fn internal_entry_source_traits(
        &self,
        frame: &Frame,
        method: EntryMethod,
    ) -> Result<Option<Vec<TraitId>>> {
        if method != EntryMethod::AbilityPut {
            return Ok(None);
        }
        let Some(source) = &frame.entry_source else {
            return Ok(None);
        };
        if frame.cause["effect"]["execution"].as_u64() != Some(source.execution.0) {
            return Err(invalid("entry source disagrees with the executing ability"));
        }
        if let Some((id, _)) = self
            .state
            .ir_objects
            .iter()
            .find(|(_, number)| **number == source.source.holder.id.0)
            && self
                .state
                .objects
                .get(id)
                .is_some_and(|current| current.generation == source.source.holder.generation.0)
        {
            return Ok(Some(
                self.card_traits(id)?.into_iter().map(TraitId).collect(),
            ));
        }
        // A source leaving in this batch or an earlier instruction keeps its authority LKI.
        Ok(Some(source.traits.clone()))
    }

    pub(in crate::game) fn internal_record_field_entry_methods(
        &mut self,
        previous: &[Object],
        frame: &Frame,
    ) -> Result<()> {
        let method = if frame.cause["effect"]["execution"].as_u64().is_some() {
            Some(EntryMethod::AbilityPut)
        } else if frame.decision["do"] == "play" {
            Some(EntryMethod::Play)
        } else {
            None
        };
        for old in previous {
            let generation = self.object(&old.id)?.generation;
            self.state.ir_field_entries.remove(&old.id);
            if let Some(method) = method {
                self.state.ir_field_entries.insert(
                    old.id.clone(),
                    FieldEntry {
                        generation,
                        method: method.into(),
                    },
                );
            }
        }
        Ok(())
    }

    pub(in crate::game::internal) fn internal_field_entry_method(
        &self,
        id: &str,
        generation: u64,
        method: EntryMethod,
    ) -> Result<bool> {
        let current = self.object(id)?;
        Ok(current.zone == "field"
            && current.generation == generation
            && self
                .state
                .ir_field_entries
                .get(id)
                .is_some_and(|fact| fact.generation == generation && fact.method == method.into()))
    }
}
