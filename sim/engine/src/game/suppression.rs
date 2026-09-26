#![expect(
    clippy::indexing_slicing,
    reason = "Suppression uses validated declarations and constructed event contexts."
)]
use serde_json::{Value, json};

use super::{Frame, Game, int, list, string};
use crate::{EngineFailure, Result, invalid};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Trigger permission must precede usage recording and pending creation."
)]
impl Game {
    pub(super) fn permit_trigger(
        &mut self,
        frame: &Frame,
        code: &Value,
        event: &str,
        cause: &Value,
        fanfare_reason: Option<&str>,
    ) -> Result<bool> {
        if !matches!(event, "enter" | "evolve")
            || code["subject"] != "self"
            || frame.event["subject_id"] != frame.source
        {
            return Ok(true);
        }
        let reason = if event == "enter" && fanfare_reason.is_some() {
            fanfare_reason.map(str::to_owned)
        } else {
            self.trigger_suppression(&frame.controller, event)?
        };
        let Some(reason) = reason else {
            return Ok(true);
        };
        let group = self.group();
        self.emit(
            json!({"kind":"抑制","ability":self.reference(&frame.source,code),"reason":reason}),
            cause,
            group,
        );
        Ok(false)
    }

    fn trigger_suppression(&self, controller: &str, event: &str) -> Result<Option<String>> {
        let (name, label) = if event == "enter" {
            ("fanfare", "ファンファーレ")
        } else {
            ("on_evolve", "進化時")
        };
        for source in self.field_ids() {
            for code in self.abilities(&source)? {
                let body = &code["body"];
                if code["kind"] != "static"
                    || !self.ability_zone(&source, &code)?
                    || body["op"] != "restrict"
                    || body["action"] != "trigger"
                    || !list(&body["events"]).contains(&json!(name))
                {
                    continue;
                }
                let frame = self.frame_for(&source)?;
                if let Some(condition) = body.get("condition")
                    && !self.truth(condition, &frame)?
                {
                    continue;
                }
                if self
                    .select(&body["subjects"], &frame)?
                    .contains(&format!("{controller}.leader"))
                {
                    return self
                        .suppression_sentence(&self.reference(&source, &code), label)
                        .map(Some);
                }
            }
        }
        Ok(None)
    }

    pub(super) fn suppression_sentence(&self, reference: &Value, label: &str) -> Result<String> {
        let object = self.object(string(&reference["source"]))?;
        let number = reference["card"].as_str().unwrap_or(&object.card);
        let face = self.catalog.face(
            number,
            usize::try_from(int(&reference["face"])).map_err(invalid)?,
        )?;
        let text = reference["section"]
            .as_u64()
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| face["sections"].get(index))
            .unwrap_or_else(|| &face["text"]);
        let line = usize::try_from(int(&reference["line"]).saturating_sub(1)).map_err(invalid)?;
        let sentences = string(text)
            .lines()
            .nth(line)
            .into_iter()
            .flat_map(|line| line.split_inclusive('。'))
            .filter(|sentence| sentence.contains(label) && sentence.contains("誘発しない"))
            .collect::<Vec<_>>();
        if let [sentence] = sentences.as_slice() {
            Ok((*sentence).into())
        } else {
            Err(EngineFailure::Unsupported(
                "suppression needs one original sentence at its source reference".into(),
            ))
        }
    }
}
