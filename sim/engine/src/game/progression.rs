#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]

use serde_json::{Value, json};

use super::costs::CostKind;
use super::{Frame, Game, int, list, scalar, string};
use crate::{EngineFailure, Result, invalid};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Rule domains share one private state and are split into focused modules."
)]
impl Game {
    pub(super) fn evolution_actions(&self, seat: &str) -> Result<Vec<Value>> {
        let mut result = Vec::new();
        for source in self.zone_ids(seat, "field") {
            for code in self
                .abilities(&source)?
                .iter()
                .filter(|code| code["kind"] == "evolve")
            {
                for evolve in self.zone_ids(seat, "evolve_deck") {
                    let card = &self.object(&evolve)?.card;
                    let count = self
                        .catalog
                        .cards
                        .get(card)
                        .map_or(0, |card| card.faces.len());
                    for face in 0..count {
                        result.extend(self.evolution_options(&source, &evolve, face, code)?);
                    }
                }
            }
        }
        Ok(result)
    }

    fn evolution_options(
        &self,
        source: &str,
        evolve: &str,
        face: usize,
        code: &Value,
    ) -> Result<Vec<Value>> {
        if !self.evolution_pair(source, evolve, face, code)? {
            return Ok(Vec::new());
        }
        let frame = self.frame_for(source)?;
        let cost = self.evolution_pp(code, &frame)?;
        let seat = &frame.controller;
        let player = self.player(seat)?;
        let mut result = Vec::new();
        let mut non_pp = code.clone();
        non_pp["costs"] = json!(
            list(&code["costs"])
                .into_iter()
                .filter(|payment| payment["op"] != "pp")
                .collect::<Vec<_>>()
        );
        for ep in 0..=player.ep.min(1).min(cost) {
            let pp = cost.saturating_sub(ep);
            if pp > int(&player.pp["current"]) {
                continue;
            }
            for sep in 0..=i64::from(player.sep > 0 && self.can_super_evolve(seat)) {
                let mut decision = json!({"do":"evolve","source":source,"evolve_card":evolve,"ability":self.reference(source,code),"pay":{"pp":pp,"ep":ep,"sep":sep}});
                if face > 0 {
                    decision["face"] = json!(face);
                }
                result.extend(self.parameterize(decision, &non_pp, &frame)?);
            }
        }
        Ok(result)
    }

    fn evolution_pair(
        &self,
        source: &str,
        evolve: &str,
        face: usize,
        code: &Value,
    ) -> Result<bool> {
        let object = self.object(source)?;
        if code["kind"] != "evolve"
            || !self.evolution_resource_matches(source, evolve, face, &code["body"])?
            || (!self.unlimited_evolution(&object.controller)?
                && self
                    .state
                    .counters
                    .get(&format!("{}.evolve_played", object.controller))
                    .copied()
                    .unwrap_or_default()
                    > 0)
        {
            return Ok(false);
        }
        let frame = self.frame_for(source)?;
        code.get("play_if")
            .map_or(Ok(true), |condition| self.truth(condition, &frame))
    }

    fn can_receive_evolution(&self, source: &str) -> Result<bool> {
        let object = self.object(source)?;
        Ok(object.zone == "field"
            && object.state["evolved"] != true
            && !string(&self.face(source)?["card_type"]).contains("エボルヴ"))
    }

    fn evolution_resource_matches(
        &self,
        source: &str,
        evolve: &str,
        face: usize,
        node: &Value,
    ) -> Result<bool> {
        let object = self.object(source)?;
        let next = self.object(evolve)?;
        if !self.can_receive_evolution(source)?
            || next.zone != "evolve_deck"
            || next.state["face_up"] == true
            || object.controller != next.controller
        {
            return Ok(false);
        }
        let new = self.catalog.face(&next.card, face)?;
        if !string(&new["card_type"]).contains("エボルヴ") {
            return Ok(false);
        }
        let names = node.get("names").map_or_else(
            || {
                node["name"].as_str().map_or_else(
                    || self.card_name(source).map(|name| vec![json!(name)]),
                    |name| Ok(vec![json!(name)]),
                )
            },
            |names| Ok(list(names)),
        )?;
        Ok(names.contains(&new["name"]))
    }

    pub(super) fn evolution_effect(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        frame.performed = 0;
        let mut subjects = Vec::new();
        for subject in self.select(&node["subjects"], frame)? {
            if self.can_receive_evolution(&subject)? {
                subjects.push(subject);
            }
        }
        if subjects.len() > 1 {
            return Err(EngineFailure::Unsupported(
                "simultaneous evolution of multiple subjects".into(),
            ));
        }
        let Some(source) = subjects.first() else {
            return Ok(());
        };
        let mut choices = self.effect_evolution_choices(source, node)?;
        if choices.is_empty() {
            choices.push(json!({"do":"resolve-choice","select":[]}));
        }
        self.prompt(frame, choices, json!({"resume":"effect-evolve","source":source,"generation":self.object(source)?.generation,"node":node}));
        let controller = self.object(source)?.controller.clone();
        if let Some(prompt) = self.state.prompt.as_mut() {
            prompt.by = controller;
        }
        Ok(())
    }

    fn effect_evolution_choices(&self, source: &str, node: &Value) -> Result<Vec<Value>> {
        let mut choices = Vec::new();
        for resource in self.zone_ids(&self.object(source)?.controller, "evolve_deck") {
            let card = &self.object(&resource)?.card;
            let count = self
                .catalog
                .cards
                .get(card)
                .map_or(0, |card| card.faces.len());
            for face in 0..count {
                if self.evolution_resource_matches(source, &resource, face, node)? {
                    let mut choice = json!({"do":"resolve-choice","select":[resource]});
                    if count > 1 {
                        choice["face"] = json!(face);
                    }
                    choices.push(choice);
                }
            }
        }
        Ok(choices)
    }

    pub(super) fn resume_effect_evolution(
        &mut self,
        task: &Value,
        decision: &Value,
        frame: &mut Frame,
    ) -> Result<()> {
        frame.performed = 0;
        let selected = list(&decision["select"]);
        let Some(resource) = selected.first().and_then(Value::as_str) else {
            return Ok(());
        };
        let source = string(&task["source"]);
        let face = usize::try_from(int(&decision["face"])).map_err(invalid)?;
        if task["generation"].as_u64() != Some(self.object(source)?.generation)
            || !self.evolution_resource_matches(source, resource, face, &task["node"])?
        {
            return Err(invalid(
                "effect evolution no longer matches its saved subjects",
            ));
        }
        self.reveal(&[resource.into()], "all", frame, false);
        self.apply_evolution(source, resource, face, 0, frame)?;
        frame.performed = 1;
        Ok(())
    }

    fn evolution_pp(&self, code: &Value, frame: &Frame) -> Result<i64> {
        let base = list(&code["costs"])
            .iter()
            .filter(|cost| cost["op"] == "pp")
            .try_fold(0_i64, |sum, cost| {
                self.number(&cost["amount"], frame)
                    .map(|amount| sum.saturating_add(amount.max(0)))
            })?;
        self.adjusted_pp_cost(&frame.source, base, frame, CostKind::Evolve)
            .map(|cost| cost.max(0))
    }

    fn can_super_evolve(&self, seat: &str) -> bool {
        int(&self.state.turn["elapsed_turns"][seat])
            >= if self.state.turn["first_player"] == seat {
                7
            } else {
                6
            }
    }

    pub(super) fn evolve_action(&mut self, decision: &Value) -> Result<bool> {
        let source = string(&decision["source"]);
        let evolve = string(&decision["evolve_card"]);
        let face = usize::try_from(int(&decision["face"])).map_err(invalid)?;
        let code = if decision.get("ability").is_some() {
            self.ability(source, &decision["ability"])?
        } else {
            let Some(code) = self
                .abilities(source)?
                .into_iter()
                .find(|code| code["kind"] == "evolve")
            else {
                return Ok(false);
            };
            code
        };
        if self.object(source)?.controller != string(&decision["by"])
            || !self.evolution_pair(source, evolve, face, &code)?
        {
            return Ok(false);
        }
        let mut frame = self.start_frame(source, self.reference(source, &code), decision)?;
        self.freeze(&code, "play-start", &mut frame)?;
        let cost = self.evolution_pp(&code, &frame)?;
        let pp = int(&decision["pay"]["pp"]);
        let ep = int(&decision["pay"]["ep"]);
        let sep = int(&decision["pay"]["sep"]);
        let player = self.player(&frame.controller)?;
        if pp < 0
            || !(0..=1).contains(&ep)
            || !(0..=1).contains(&sep)
            || pp.saturating_add(ep) != cost
            || int(&player.pp["current"]) < pp
            || player.ep < ep
            || player.sep < sep
            || (sep > 0 && !self.can_super_evolve(&frame.controller))
            || !self.valid_parameters(&code, &frame)?
        {
            return Ok(false);
        }
        let mut non_pp = code.clone();
        non_pp["costs"] = json!(
            list(&code["costs"])
                .into_iter()
                .filter(|payment| payment["op"] != "pp")
                .collect::<Vec<_>>()
        );
        if !self.can_pay(&list(&non_pp["costs"]), &frame)? {
            return Ok(false);
        }
        self.consume_cost_adjustments(source, CostKind::Evolve)?;
        self.pay_costs(&non_pp, &mut frame)?;
        if self.object(source)?.zone != "field" {
            return Err(EngineFailure::Unsupported(
                "evolution cost moved its source".into(),
            ));
        }
        let resources = self.player_mut(&frame.controller)?;
        resources.pp["current"] = json!(int(&resources.pp["current"]).saturating_sub(pp));
        resources.ep = resources.ep.saturating_sub(ep);
        resources.sep = resources.sep.saturating_sub(sep);
        self.bump(&format!("{}.evolve_played", frame.controller), 1);
        self.apply_evolution(source, evolve, face, sep, &mut frame)?;
        Ok(true)
    }

    fn apply_evolution(
        &mut self,
        source: &str,
        evolve: &str,
        face: usize,
        sep: i64,
        frame: &mut Frame,
    ) -> Result<()> {
        let previous = self.object(source)?.clone();
        let old = self.face(source)?.clone();
        let new = self.catalog.face(&self.object(evolve)?.card, face)?.clone();
        self.move_objects(&[evolve.into()], "evolution", None, None, frame)?;
        self.object_mut(evolve)?.state["face"] = json!(face);
        let attrs = &mut self.object_mut(source)?.state;
        attrs["evolved"] = json!(true);
        attrs["evolved_with"] = json!(evolve);
        attrs["face"] = json!(face);
        for field in ["power", "hp"] {
            let delta = scalar(&new[field])
                .saturating_sub(scalar(&old[field]))
                .saturating_add(sep);
            attrs[field] = json!(int(&attrs[field]).saturating_add(delta));
            if field == "hp" {
                attrs["max_hp"] = json!(int(&attrs["max_hp"]).saturating_add(delta));
            }
        }
        self.bump(
            &format!("{}.evolutions", self.object(source)?.controller),
            1,
        );
        let group = self.group();
        let event = self.emit(
            json!({"kind":"進化","object":source,"with":evolve}),
            &frame.cause,
            group,
        );
        frame.cause = json!({"event":event});
        let affected = [self.object(source)?.clone()];
        let mut pending = self.stat_change_triggers(&[previous], &frame.cause)?;
        pending.extend(self.collect_triggers("evolve", &affected, &frame.cause)?);
        self.enqueue(pending);
        if sep > 0 {
            let super_group = self.group();
            self.emit(
                json!({"kind":"超進化","object":source}),
                &frame.cause,
                super_group,
            );
            let super_pending = self.collect_triggers("super_evolve", &affected, &frame.cause)?;
            self.enqueue(super_pending);
        }
        Ok(())
    }
}
