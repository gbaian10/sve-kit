//! Body posture changes share the ordinary mutation and emit facts only for changed members.
use super::observation::EffectOrigin;
use crate::game::player::Viewer;
use crate::game::player::observation::InstalledEffect;
use crate::ir::PlayerId;

use super::{Execution, Fact, Game, InstructionOutcome, Result, invalid, unsupported};
use crate::ir::{
    Ability, AbilityQualification, Action, ActionPattern, Bool, DurationEnd, Edge, Effect, Expr,
    InstallationSubjects, InstallationTime, Instruction, Located, ObjectOperand, ObjectRef,
    PermissionSubject, Phase, PlayerRef, Polarity, Posture, PostureEvent, Selector, StandScope,
    TimeAnchor, ZoneKind, stand_restrictions,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::game) struct StandLock {
    origin: EffectOrigin,
    object: u64,
    generation: u64,
    player: u32,
    after: u64,
}

impl StandLock {
    const fn applies(&self, object: u64, generation: u64, player: u32, turn: u64) -> bool {
        self.object == object
            && self.generation == generation
            && self.player == player
            && turn > self.after
    }
}

pub(super) fn check_install(action: &Action) -> Result<()> {
    let Action::InstallEffect {
        subjects: InstallationSubjects::Objects { affected, .. },
        capture: InstallationTime::InstructionStart,
        effect,
        duration,
    } = action
    else {
        return Err(unsupported("no-stand installation"));
    };
    let Effect::Permission {
        subject: PermissionSubject::Objects { objects },
        action:
            ActionPattern::Stand {
                objects: action_objects,
                scope: StandScope::RuleStart { player },
            },
        polarity: Polarity::Forbid,
        condition: Expr::Literal(Bool(true)),
        duration: inner,
        ..
    } = effect.as_ref()
    else {
        return Err(unsupported("rule-start permission"));
    };
    if *objects
        != (Selector::Bound {
            reference: *affected,
        })
        || objects != action_objects
        || inner != duration
        || !matches!(&duration.end, DurationEnd::NextPhaseEdge { player: end_player, phase: Phase::Start, edge: Edge::End } if end_player == player)
        || duration.anchor != (TimeAnchor::Installation {})
        || duration.usage.is_some()
        || !duration.intervals.is_empty()
    {
        return Err(unsupported("next start permission lifetime"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Posture instructions mutate the same authoritative Game."
)]
impl Game {
    pub(super) fn observed_stand_locks(
        &self,
        perspective: Viewer,
        holder: &str,
    ) -> Result<Vec<InstalledEffect>> {
        use crate::game::player::observation::{
            EffectExpiry, InstalledChange, InstalledEffect, viewer,
        };
        let object = self.object(holder)?;
        let Some(number) = self.state.ir_objects.get(holder) else {
            return Ok(vec![]);
        };
        let now = super::player_id(self.active())
            .ok()
            .map(|player| player.seat)
            .zip(self.state.schedule.current().ok());
        self.state
            .ir_no_stand
            .iter()
            .filter(|lock| {
                object.zone == "field"
                    && lock.object == *number
                    && lock.generation == object.generation
            })
            .map(|lock| {
                let player = viewer(super::seat_name(PlayerId { seat: lock.player })?)?;
                Ok(InstalledEffect {
                    source: lock.origin.observed(self, perspective),
                    expires: EffectExpiry::NextStartEnd(player),
                    main_live: true,
                    intervals: vec![],
                    active: now.is_some_and(|(active_player, turn)| {
                        lock.applies(*number, object.generation, active_player, turn)
                    }),
                    change: InstalledChange::NoStand { player },
                })
            })
            .collect()
    }

    pub(super) fn internal_posture(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        objects: &Selector,
        posture: Posture,
    ) -> Result<()> {
        let members = execution.objects(self, objects)?;
        self.internal_posture_members(execution, instruction, members, posture)
    }

    pub(super) fn internal_posture_members(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        members: Vec<ObjectRef>,
        posture: Posture,
    ) -> Result<()> {
        let acted = posture == Posture::Acted;
        let mut planned = Vec::new();
        for member in members {
            let id = execution.object_id(member)?;
            let object = self.object(id)?;
            if object.generation != member.generation.0 {
                continue;
            }
            if object.zone != "field" {
                return Err(unsupported("body posture requires a public field object"));
            }
            let before = object
                .state
                .get("acted")
                .and_then(serde_json::Value::as_bool)
                .ok_or_else(|| invalid("invalid field posture"))?;
            if before != acted {
                planned.push((
                    id.to_owned(),
                    PostureEvent {
                        object: member,
                        before: if acted {
                            Posture::Standing
                        } else {
                            Posture::Acted
                        },
                        after: posture,
                    },
                ));
            }
        }
        let ids = planned.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>();
        self.change_posture(&ids, acted, &execution.frame().cause)?;
        let changes = planned
            .into_iter()
            .map(|(_, event)| event)
            .collect::<Vec<_>>();
        for event in &changes {
            execution.record_fact(self, Fact::Posture(*event));
        }
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Posture(changes));
        Ok(())
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Rule stand restrictions are separate from effect posture changes."
)]
impl Game {
    pub(super) fn internal_install_no_stand(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check_install(&instruction.action)?;
        let Action::InstallEffect {
            subjects: InstallationSubjects::Objects { objects, .. },
            duration,
            ..
        } = &instruction.action
        else {
            return Err(unsupported("no-stand subjects"));
        };
        let DurationEnd::NextPhaseEdge { player, .. } = &duration.end else {
            return Err(unsupported("no-stand expiry"));
        };
        let player = execution.player(self, player)?.seat;
        let after = self.state.schedule.current()?;
        let mut installed = Vec::new();
        for member in execution.objects(self, objects)? {
            let object = self.object(execution.object_id(member)?)?;
            if object.generation != member.generation.0 || object.zone != "field" {
                continue;
            }
            self.state.ir_no_stand.push(StandLock {
                origin: EffectOrigin::capture(self, execution, &instruction.source)?,
                object: member.id.0,
                generation: member.generation.0,
                player,
                after,
            });
            installed.push(member);
        }
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Installed(installed),
        );
        Ok(())
    }

    pub(in crate::game) fn internal_rule_stand_blocked(&self, id: &str) -> Result<bool> {
        if self.internal_boxed(id) {
            return Ok(true);
        }
        let object = self.object(id)?;
        if object.zone == "field" && object.controller == self.active() {
            let (card, _) = self.internal_effective_print(id)?;
            if self.card_runtime.is_loaded(&card)
                && self
                    .internal_printed_abilities(id)?
                    .iter()
                    .any(|ability| stand_restrictions::is_static(&ability.node))
            {
                return Ok(true);
            }
        }
        let Some(number) = self.state.ir_objects.get(id) else {
            return Ok(false);
        };
        let player = super::player_id(self.active())?.seat;
        let turn = self.state.schedule.current()?;
        Ok(self
            .state
            .ir_no_stand
            .iter()
            .any(|lock| lock.applies(*number, object.generation, player, turn)))
    }

    pub(in crate::game) fn internal_finish_rule_stand(&mut self) -> Result<()> {
        let player = super::player_id(self.active())?.seat;
        let turn = self.state.schedule.current()?;
        self.state
            .ir_no_stand
            .retain(|lock| lock.player != player || turn <= lock.after);
        Ok(())
    }
}

pub(super) fn check_static(located: &Located<Ability>) -> Result<()> {
    let ability = &located.node;
    if !stand_restrictions::is_static(ability)
        || ability.qualification != AbilityQualification::Ordinary
        || ability.active_zones != [ZoneKind::Field]
        || ability.source.holder != (ObjectOperand::SourceHolder {})
        || ability.source.controller != (PlayerRef::Controller {})
        || ability.play.is_some()
        || ability.trigger.is_some()
        || ability.body.is_some()
        || !ability.event_captures.is_empty()
    {
        return Err(unsupported("printed holder rule-start stand restriction"));
    }
    Ok(())
}
