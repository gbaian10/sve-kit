#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON reads are total and writes use constructed maps."
)]
use super::{Frame, Game, ZONES, int, list, string};
use crate::{EngineFailure, Result, invalid};
use alloc::collections::BTreeSet;
use serde_json::{Value, json};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Random operations share authoritative state and private script cursors."
)]
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

    fn check_random_population(&self, select: &Value, frame: &Frame) -> Result<()> {
        for operator in ["union", "difference"] {
            for part in list(&select[operator]) {
                self.check_random_population(&part, frame)?;
            }
        }
        if let Some(from) = select.get("from") {
            return self.check_random_population(from, frame);
        }
        let zones = if select["zone"] == "any" {
            ZONES.to_vec()
        } else {
            vec![string(&select["zone"])]
        };
        for seat in self.seats(string(&select["side"]), frame) {
            for zone in &zones {
                if self.zone(&seat, zone).iter().any(|item| !item.is_string()) {
                    return Err(EngineFailure::Unsupported(
                        "random selection needs identities for anonymous zone members".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub(super) fn random_selection(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        let count = usize::try_from(self.number(&node["count"], frame)?.max(0)).map_err(invalid)?;
        let mut chosen = Vec::new();
        if count > 0 {
            self.check_random_population(&node["select"], frame)?;
            let mut candidates = self.select(&node["select"], frame)?;
            let count = count.min(candidates.len());
            if count > 0 {
                if let Some(script) = self.next_random_script("random_selections")? {
                    chosen = self.scripted_selection(&script, &candidates, count)?;
                } else {
                    for _ in 0..count {
                        let bound = u64::try_from(candidates.len()).map_err(invalid)?;
                        let index =
                            usize::try_from(self.bounded_random(bound)?).map_err(invalid)?;
                        chosen.push(candidates.remove(index));
                    }
                }
            }
        }
        frame.performed = i64::try_from(chosen.len()).map_err(invalid)?;
        frame.bindings.insert(string(&node["bind"]).into(), chosen);
        Ok(())
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

    pub(super) fn roll_effect(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        let count = self.number(&node["count"], frame)?.max(0);
        if count == 0 {
            frame.performed = 0;
            return Ok(());
        }
        if count != 1 {
            return Err(EngineFailure::Unsupported(
                "one dice node represents one roll; use per for successive rolls".into(),
            ));
        }
        let bind = string(&node["bind"]);
        let reroll = node["reroll"] == true;
        if reroll && !frame.values.get(bind).is_some_and(Value::is_i64) {
            return Err(invalid(
                "reroll requires a previous result in the same binding",
            ));
        }
        self.roll_once(bind, reroll, frame)?;
        if !reroll {
            let permitted = self.reroll_count(frame)?;
            self.offer_reroll(bind, permitted, frame);
        }
        Ok(())
    }

    fn roll_once(&mut self, bind: &str, reroll: bool, frame: &mut Frame) -> Result<()> {
        let result = match self.next_random_script("dice")? {
            Some(value) => value
                .as_i64()
                .filter(|n| (1..=6).contains(n))
                .ok_or_else(|| invalid("scripted die must be an integer from one to six"))?,
            None => i64::try_from(self.bounded_random(6)?.saturating_add(1)).map_err(invalid)?,
        };
        frame.values.insert(bind.into(), json!(result));
        frame.performed = 1;
        let group = self.group();
        self.emit(
            json!({"kind":"サイコロ","player":frame.controller,"result":result,"reroll":reroll}),
            &frame.cause,
            group,
        );
        Ok(())
    }

    fn reroll_count(&self, frame: &Frame) -> Result<i64> {
        let mut count = 0_i64;
        for source in self.ability_sources() {
            for code in self.abilities(&source)? {
                if code["kind"] != "static"
                    || code["body"]["op"] != "allow_reroll"
                    || !self.ability_zone(&source, &code)?
                {
                    continue;
                }
                let context = self.frame_for(&source)?;
                if self
                    .seats(string(&code["body"]["side"]), &context)
                    .contains(&frame.controller)
                {
                    count =
                        count.saturating_add(self.number(&code["body"]["count"], &context)?.max(0));
                }
            }
        }
        Ok(count)
    }

    fn offer_reroll(&mut self, bind: &str, remaining: i64, frame: &mut Frame) {
        if remaining > 0 {
            self.prompt(
                frame,
                vec![
                    json!({"do":"resolve-choice","choice":"execute"}),
                    json!({"do":"resolve-choice","choice":"decline"}),
                ],
                json!({"resume":"reroll","bind":bind,"remaining":remaining}),
            );
        }
    }

    pub(super) fn resume_reroll(
        &mut self,
        task: &Value,
        decision: &Value,
        frame: &mut Frame,
    ) -> Result<()> {
        if decision["choice"] == "execute" {
            let bind = string(&task["bind"]);
            self.roll_once(bind, true, frame)?;
            self.offer_reroll(bind, int(&task["remaining"]).saturating_sub(1), frame);
        }
        Ok(())
    }
}
