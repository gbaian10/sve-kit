#![expect(
    clippy::indexing_slicing,
    reason = "Nested continuations use validated JSON programs and constructed decisions."
)]
use super::{Frame, Game, string};
use crate::{EngineFailure, Result, invalid};
use serde_json::{Value, json};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Nested plays use the same payment, targeting and execution engine."
)]
impl Game {
    pub(super) fn nested_play(&self, node: &Value, frame: &mut Frame) -> Result<()> {
        let ids = self.select(&node["subjects"], frame)?;
        let mut steps = Vec::new();
        for id in ids {
            let mut task = node.clone();
            task["object"] = json!(id);
            task["op"] = json!("_nested_prepare");
            task["kind"] = node["op"].clone();
            steps.push(task);
        }
        Self::prepend(frame, steps);
        Ok(())
    }

    pub(super) fn prepare_nested(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        let source = string(&node["object"]);
        let codes = self
            .abilities(source)?
            .into_iter()
            .filter(|code| {
                if node["kind"] == "play_card" {
                    matches!(string(&code["kind"]), "spell" | "play")
                } else {
                    code["event"] == node["event"]
                }
            })
            .collect::<Vec<_>>();
        if node["kind"] == "play_ability" && codes.len() != 1 {
            return Err(EngineFailure::Unsupported(
                "nested ability selection is not unique".into(),
            ));
        }
        let initial = self.frame_for(source)?;
        let mut options = vec![json!({"do":"resolve-choice"})];
        for code in &codes {
            let mut next = Vec::new();
            for option in options {
                next.extend(self.parameterize(option, code, &initial)?);
            }
            options = next;
        }
        if options.is_empty() {
            return Ok(());
        }
        if options.len() == 1 && options.first() == Some(&json!({"do":"resolve-choice"})) {
            return self.start_nested(node, &json!({}), frame);
        }
        self.prompt(frame, options, json!({"resume":"nested","node":node}));
        Ok(())
    }

    pub(super) fn start_nested(
        &mut self,
        node: &Value,
        decision: &Value,
        outer: &mut Frame,
    ) -> Result<()> {
        let source = string(&node["object"]);
        let mut request = decision.clone();
        request["by"] = json!(outer.controller);
        request["at"] = json!("resolve");
        let mut inner = if node["kind"] == "play_card" {
            request["do"] = json!("play");
            request["card"] = json!(source);
            let cost = node.get("set_cost").map_or_else(
                || self.play_cost_context(source, &self.frame_for(source)?),
                |amount| self.number(amount, outer),
            )?;
            let Some(frame) = self.prepare_card_play(&request, Some(cost))? else {
                return Ok(());
            };
            frame
        } else {
            let code = self
                .abilities(source)?
                .into_iter()
                .find(|code| code["event"] == node["event"])
                .ok_or_else(|| invalid("nested ability disappeared"))?;
            request["do"] = json!("activate");
            request["ability"] = self.reference(source, &code);
            let mut frame = self.start_frame(source, self.reference(source, &code), &request)?;
            if !self.prepare_additional(&code, &mut frame)?
                || !self.valid_parameters(&code, &frame)?
                || !self.can_pay(&Self::payment_nodes(&code, &frame), &frame)?
            {
                return Ok(());
            }
            self.pay_costs(&code, &mut frame)?;
            self.prepare_ability(&mut frame, &code)?;
            frame
        };
        inner.occurrence = outer.occurrence;
        let mut restore = outer.clone();
        restore.todo.clear();
        inner
            .todo
            .push(json!({"op":"_restore_frame","frame":restore}));
        inner.todo.extend(outer.todo.clone());
        *outer = inner;
        Ok(())
    }

    pub(super) fn restore_frame(node: &Value, frame: &mut Frame) -> Result<()> {
        let mut saved: Frame = serde_json::from_value(node["frame"].clone()).map_err(invalid)?;
        saved.occurrence = frame.occurrence;
        saved.todo.clone_from(&frame.todo);
        *frame = saved;
        Ok(())
    }
}
