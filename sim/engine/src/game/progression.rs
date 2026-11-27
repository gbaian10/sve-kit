#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]

use serde_json::{Value, json};

use super::{Frame, Game, int, scalar, string};
use crate::ir::evolution_destination::Destination;
use crate::ir::{Ability, SnapshotId};
use crate::{EngineFailure, Result, invalid};

impl Game {
    pub(in crate::game) fn evolution_pair(
        &self,
        source: &str,
        evolve: &str,
        face: usize,
        destination: &Destination,
    ) -> Result<bool> {
        let object = self.object(source)?;
        if !self.evolution_resource_matches(source, evolve, face, destination)?
            || (!self.internal_unlimited_evolution(&object.controller)?
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
        Ok(true)
    }

    pub(in crate::game) fn can_receive_evolution(&self, source: &str) -> Result<bool> {
        let object = self.object(source)?;
        Ok(object.zone == "field"
            && object.state["evolved"] != true
            && !string(&self.face(source)?["card_type"]).contains("エボルヴ"))
    }

    pub(in crate::game) fn evolution_resource_matches(
        &self,
        source: &str,
        evolve: &str,
        face: usize,
        destination: &Destination,
    ) -> Result<bool> {
        let object = self.object(source)?;
        let next = self.object(evolve)?;
        let owned_resource = next.owner == next.controller;
        if !self.can_receive_evolution(source)?
            || next.zone != "evolve_deck"
            || next.state["face_up"] == true
            || object.controller != next.controller
            || !owned_resource
        {
            return Ok(false);
        }
        let new = self.catalog.face(&next.card, face)?;
        if !string(&new["card_type"]).contains("エボルヴ") {
            return Ok(false);
        }
        let is_evolve_follower = string(&new["card_type"]) == "フォロワー・エボルヴ";
        if matches!(destination, Destination::Contains(_)) && !is_evolve_follower {
            return Ok(false);
        }
        let source_names = if matches!(destination, Destination::SameName) {
            self.card_names(source)?
        } else {
            vec![]
        };
        self.card_name_matches_print(evolve, &next.card, face, |name| match destination {
            Destination::SameName => source_names
                .iter()
                .any(|source_name| destination.matches(source_name, name, is_evolve_follower)),
            Destination::Names(_) | Destination::Contains(_) => {
                destination.matches("", name, is_evolve_follower)
            }
        })
    }

    pub(in crate::game) fn evolution_pp(&self, base_pp: i64, frame: &Frame) -> Result<i64> {
        self.require_ability_sources("cost adjustment")?;
        self.internal_installed_evolution_cost(&frame.source, base_pp.max(0))
            .map(|cost| cost.max(0))
    }

    pub(in crate::game) fn can_super_evolve(&self, seat: &str) -> bool {
        self.player(seat)
            .ok()
            .and_then(|player| player.sep_available_from)
            .is_some_and(|turn| {
                u64::try_from(int(&self.state.turn["elapsed_turns"][seat]))
                    .is_ok_and(|elapsed| elapsed >= turn)
            })
    }

    pub(in crate::game) fn evolve_with_destination(
        &mut self,
        decision: &Value,
        destination: &Destination,
        base_pp: i64,
        intrinsic: &Ability,
        probe: bool,
    ) -> Result<bool> {
        let source = string(&decision["source"]);
        let evolve = string(&decision["evolve_card"]);
        let face = usize::try_from(int(&decision["face"])).map_err(invalid)?;
        if self.object(source)?.controller != string(&decision["by"])
            || !self.evolution_pair(source, evolve, face, destination)?
        {
            return Ok(false);
        }
        let mut frame = self.start_frame(source, self.reference(source, &Value::Null), decision)?;
        let cost = self.evolution_pp(base_pp, &frame)?;
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
        {
            return Ok(false);
        }
        self.numeric_evolution_allowed(source, true)?;
        if !self.internal_evolution_payment(intrinsic, decision)? {
            return Ok(false);
        }
        if probe {
            return Ok(true);
        }
        self.object_mut(evolve)?.state["face"] = json!(face);
        self.reveal(&[evolve.into()], "all", &frame, false)?;
        if self.object(source)?.zone != "field" {
            return Err(EngineFailure::Unsupported(
                "evolution cost moved its source".into(),
            ));
        }
        let resources = self.player_mut(&frame.controller)?;
        let before = int(&resources.pp["current"]);
        resources.pp["current"] = json!(before.saturating_sub(pp));
        resources.set_ep(resources.ep.saturating_sub(ep));
        resources.sep = resources.sep.saturating_sub(sep);
        self.record_player_resource(
            &frame.controller,
            "pp",
            before,
            before.saturating_sub(pp),
            &frame.cause,
        );
        self.bump(&format!("{}.evolve_played", frame.controller), 1);
        self.apply_evolution(source, evolve, face, sep, &mut frame)?;
        Ok(true)
    }

    pub(in crate::game) fn apply_evolution(
        &mut self,
        source: &str,
        evolve: &str,
        face: usize,
        sep: i64,
        frame: &mut Frame,
    ) -> Result<()> {
        self.numeric_evolution_allowed(source, true)?;
        let typed_evolution = self.internal_before_evolution(source, evolve, face)?;
        let evolution_cause = frame.cause.clone();
        let old = self.face(source)?.clone();
        let new = self.catalog.face(&self.object(evolve)?.card, face)?.clone();
        let printed_delta = |field: &str| {
            scalar(&new[field])
                .checked_sub(scalar(&old[field]))
                .ok_or_else(|| invalid("evolution delta overflow"))
        };
        self.object_mut(evolve)?.state["face"] = json!(face);
        self.move_objects(&[evolve.into()], "evolution", None, None, frame)?;
        self.object_mut(source)?.state["evolved"] = json!(true);
        if self.object(source)?.state["control_changed_this_turn"] == true {
            self.object_mut(source)?.state["evolved_after_control_change"] = json!(true);
        }
        self.object_mut(source)?.state["evolved_with"] = json!(evolve);
        self.object_mut(source)?.state["face"] = json!(face);
        self.numeric_add_base(source, printed_delta("power")?, printed_delta("hp")?)?;
        // Base replacement is not a stat-change event; only super evolution's explicit +1/+1 counts.
        let previous = self.object(source)?.clone();
        let stat_before = SnapshotId(self.state.ir_epoch);
        if sep > 0 {
            self.numeric_add_base(source, sep, sep)?;
            self.internal_super_stat_event(&previous, &evolution_cause, stat_before)?;
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
        self.player_step_actor(&event, &frame.controller);
        frame.cause = json!({"event":event});
        self.record_evolution_stat_change(&previous)?;
        self.require_ability_sources("event evolve subscriptions")?;
        self.group();
        let mut player_super_step = None;
        if sep > 0 {
            let super_group = self.group();
            let player_step = self.emit(
                json!({"kind":"超進化","object":source}),
                &frame.cause,
                super_group,
            );
            self.player_step_actor(&player_step, &frame.controller);
            player_super_step = Some(player_step);
            self.require_ability_sources("event super_evolve subscriptions")?;
            self.group();
        }
        if !self.card_runtime.is_empty() {
            self.object_mut(source)?.state["super_evolved"] = json!(sep > 0);
        }
        if let Some(step) = player_super_step {
            let subjects = self.player_subject_before(source);
            self.player_subject_after(&step, subjects);
        }
        self.internal_evolve_event(
            source,
            evolve,
            sep > 0,
            typed_evolution,
            &evolution_cause,
            frame,
        )?;
        self.stop_unadmitted_resource(evolve, face)?;
        Ok(())
    }
}
