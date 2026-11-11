use super::super::observation::EffectOrigin;
use super::super::{Execution, InstructionOutcome, seat_name};
use super::{Ability, Context, Game, Generation, Located, PlayerId, Result, unsupported};
use crate::game::player::Viewer;
use crate::game::player::observation::InstalledEffect;
use crate::ir::play_restrictions::card_play_rule;
use crate::ir::{Action, Instruction, play_restrictions};
use crate::ir::{Bool, Expr, ZoneKind};

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct BanishPermission {
    turn: u64,
    origin: EffectOrigin,
}

pub(super) fn check(located: &Located<Ability>, ordinal: usize) -> Result<bool> {
    let Some(rule) = card_play_rule(&located.node) else {
        return Ok(false);
    };
    if ordinal != 0 {
        return Err(unsupported("play restriction requires base face"));
    }
    super::super::ordering::check_condition(rule.condition, &located.source)?;
    Ok(true)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Printed restrictions share the authoritative card play boundary."
)]
impl Game {
    pub(in crate::game::internal) fn observed_banish_permission(
        &self,
        perspective: Viewer,
        player: Viewer,
    ) -> Vec<InstalledEffect> {
        use crate::game::player::observation::{EffectExpiry, InstalledChange, InstalledEffect};
        self.card_runtime
            .banish_play
            .get(&player.player().seat)
            .filter(|permission| {
                perspective == player || permission.origin.observed(self, perspective).is_some()
            })
            .map(|permission| InstalledEffect {
                source: permission.origin.observed(self, perspective),
                expires: EffectExpiry::TurnEnd,
                main_live: true,
                intervals: vec![],
                active: true,
                change: InstalledChange::BanishPlay,
            })
            .into_iter()
            .collect()
    }

    pub(super) fn internal_card_play_forbidden(
        &self,
        holder: &str,
        actor: PlayerId,
    ) -> Result<bool> {
        self.internal_card_play_rule_matches(holder, actor, true)
    }

    pub(super) fn internal_card_cemetery_play_allowed(
        &self,
        holder: &str,
        actor: PlayerId,
    ) -> Result<bool> {
        self.internal_card_play_rule_matches(holder, actor, false)
    }

    fn internal_card_play_rule_matches(
        &self,
        holder: &str,
        actor: PlayerId,
        forbids: bool,
    ) -> Result<bool> {
        let object = self.object(holder)?;
        let zone = match object.zone.as_str() {
            "hand" => ZoneKind::Hand,
            "ex" => ZoneKind::Ex,
            "cemetery" => ZoneKind::Cemetery,
            "banish" => ZoneKind::Banish,
            _ => return Ok(false),
        };
        for located in self.internal_printed_abilities(holder)? {
            let ability = &located.node;
            let Some(rule) = card_play_rule(ability)
                .filter(|rule| rule.forbids == forbids && ability.active_zones.contains(&zone))
            else {
                continue;
            };
            if self.internal_card_play_rule_applies(holder, actor, ability, rule.condition)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn internal_card_play_rule_applies(
        &self,
        holder: &str,
        actor: PlayerId,
        ability: &Ability,
        predicate: &Expr<Bool>,
    ) -> Result<bool> {
        if let Expr::Literal(value) = predicate {
            return Ok(value.0);
        }
        // Legality probes must leave allocation clocks and ability instances untouched.
        let mut probe = self.clone();
        let mut execution = probe.new_card_execution(Context {
            source: holder.into(),
            generation: Generation(self.object(holder)?.generation),
            controller: actor,
        })?;
        let source = probe.internal_card_source(holder, ability.id, actor)?;
        execution.bind_source(&probe, source)?;
        Ok(execution.boolean(&probe, predicate)?.0)
    }
}

pub(in crate::game::internal) fn check_install(action: &Action) -> Result<()> {
    if !play_restrictions::is_banish_play_installation(action) {
        return Err(unsupported(
            "own banish play permission for the current turn",
        ));
    }
    Ok(())
}
#[expect(
    clippy::multiple_inherent_impl,
    reason = "Player play permissions share the authoritative play gate."
)]
impl Game {
    pub(in crate::game::internal) fn internal_install_banish_play(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check_install(&instruction.action)?;
        let turn = self.state.schedule.current()?;
        self.card_runtime.banish_play.insert(
            execution.context.controller.seat,
            BanishPermission {
                turn,
                origin: EffectOrigin::capture(self, execution, &instruction.source)?,
            },
        );
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Installed(vec![]));
        Ok(())
    }
    pub(in crate::game::internal) fn internal_card_banish_play_allowed(
        &self,
        holder: &str,
        actor: PlayerId,
    ) -> Result<bool> {
        let object = self.object(holder)?;
        let seat = seat_name(actor)?;
        Ok(object.zone == "banish"
            && object.owner == seat
            && object.controller == seat
            && self
                .card_runtime
                .banish_play
                .get(&actor.seat)
                .map(|permission| permission.turn)
                == Some(self.state.schedule.current()?))
    }

    pub(in crate::game) fn internal_player_play_end_turn(&mut self, turn: u64) {
        self.card_runtime
            .banish_play
            .retain(|_, permission| permission.turn != turn);
    }
}
