use super::{Game, list, string};
use crate::{Result, invalid};
use alloc::collections::BTreeSet;
use serde_json::Value;

impl Game {
    fn next_random_script(&mut self, kind: &str) -> Result<Option<Value>> {
        let Some(script) = self.state.random.get(kind) else {
            return Ok(None);
        };
        let index = self.state.random_cursors.entry(kind.into()).or_default();
        let value = script
            .as_array()
            .and_then(|values| values.get(*index))
            .ok_or_else(|| invalid(format!("missing or exhausted {kind} script")))?
            .clone();
        *index = index.saturating_add(1);
        Ok(Some(value))
    }

    fn bounded_random(&mut self, bound: u64) -> Result<u64> {
        self.state.rng.bounded(bound)
    }

    pub(super) fn random_members(
        &mut self,
        mut candidates: Vec<String>,
        count: usize,
    ) -> Result<Vec<String>> {
        let count = count.min(candidates.len());
        if count == 0 {
            return Ok(vec![]);
        }
        if let Some(script) = self.next_random_script("random_selections")? {
            return self.scripted_selection(&script, &candidates, count);
        }
        let mut chosen = Vec::new();
        for _ in 0..count {
            let bound = u64::try_from(candidates.len()).map_err(invalid)?;
            let index = usize::try_from(self.bounded_random(bound)?).map_err(invalid)?;
            chosen.push(candidates.remove(index));
        }
        Ok(chosen)
    }

    fn scripted_selection(
        &self,
        script: &Value,
        candidates: &[String],
        count: usize,
    ) -> Result<Vec<String>> {
        if script["n"].as_u64()
            != self
                .state
                .random_cursors
                .get("random_selections")
                .and_then(|n| u64::try_from(*n).ok())
        {
            return Err(invalid("random selection script occurrence mismatch"));
        }
        for id in candidates {
            let object = self.object(id)?;
            if script["from"] != format!("{}.{}", object.controller, object.zone) {
                return Err(invalid("random selection script origin mismatch"));
            }
        }
        let result = list(&script["result"]);
        let mut seen = BTreeSet::new();
        if result.len() != count
            || result.iter().any(|id| {
                !id.as_str()
                    .is_some_and(|id| candidates.iter().any(|candidate| candidate == id))
                    || !seen.insert(id.to_string())
            })
        {
            return Err(invalid("random selection script is not a valid subset"));
        }
        Ok(result.iter().map(|id| string(id).to_owned()).collect())
    }

    pub(super) fn roll_die(&mut self) -> Result<i64> {
        match self.next_random_script("dice")? {
            Some(value) => value
                .as_i64()
                .filter(|n| (1..=6).contains(n))
                .ok_or_else(|| invalid("scripted die must be an integer from one to six")),
            None => i64::try_from(self.bounded_random(6)?.saturating_add(1)).map_err(invalid),
        }
    }
}
