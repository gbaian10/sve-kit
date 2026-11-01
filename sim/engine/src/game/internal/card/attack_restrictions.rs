use super::super::duration::{EffectEnd, TurnInterval};
use super::super::{Execution, InstructionOutcome, Slot};
use crate::ir::{
    Bool, DurationEnd, InstallationSubjects, InstallationTime, Interval, Selector, TimeAnchor,
};
use serde::{Deserialize, Serialize};

use super::super::{Context, admission, player_id};
use super::{Ability, Game, Located, Result, unsupported};
use crate::ir::attack_restrictions::{self, projection};
use crate::ir::{
    AbilityQualification, Action, BatchPolicy, Control, DamageAmount, DamageFlags, DamageRecipient,
    Expr, Generation, Instruction, InstructionContext, InstructionId, Int, Node, ObjectId,
    ObjectOperand, ObjectView, PlayerRef, Program, ReceiptId, RecipientSet, RuntimeDamageRecipient,
    ZoneKind,
};

pub(super) fn is_static(ability: &Ability) -> bool {
    attack_restrictions::is_static(ability)
}

pub(super) fn check(located: &Located<Ability>) -> Result<()> {
    let ability = &located.node;
    if !is_static(ability)
        || ability.active_zones != [ZoneKind::Field]
        || ability.qualification != AbilityQualification::Ordinary
        || ability.source.holder != (ObjectOperand::SourceHolder {})
        || ability.source.controller != (PlayerRef::Controller {})
        || ability.play.is_some()
        || ability.trigger.is_some()
        || ability.body.is_some()
        || !ability.event_captures.is_empty()
    {
        return Err(unsupported("static attack permission header"));
    }
    for effect in &ability.declarations {
        let shape = projection(effect).ok_or_else(|| unsupported("attack permission shape"))?;
        let mut sets = vec![RecipientSet::Objects {
            objects: shape.attacker.clone(),
        }];
        sets.extend(shape.target.cloned());
        let body = Program {
            scope: ability.scope,
            steps: sets
                .into_iter()
                .zip(1_u64..)
                .map(|(recipients, id)| Located {
                    source: located.source.clone(),
                    node: Node::Instruction {
                        instruction: Box::new(Instruction {
                            id: InstructionId(id),
                            actor: PlayerRef::Controller {},
                            source: ability.source.clone(),
                            precondition: None,
                            context: InstructionContext::Effect,
                            receipt: ReceiptId(id),
                            action: Action::Damage {
                                recipients,
                                amount: DamageAmount::Uniform {
                                    amount: Expr::Literal(Int(0)),
                                },
                                flags: DamageFlags::Ability,
                                source: ObjectView::SourceInformation {},
                                batch: BatchPolicy::Instruction {},
                            },
                        }),
                    },
                })
                .collect(),
        };
        let program = Program {
            scope: ability.scope,
            steps: vec![Located {
                source: located.source.clone(),
                node: Node::Control {
                    control: Control::If {
                        condition: shape.condition.clone(),
                        then_body: body,
                        else_body: None,
                    },
                },
            }],
        };
        // Reuse selector and predicate admission without executing the zero-damage probe.
        admission::check_card(&program, &located.source, &ability.source, None)?;
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Attack permissions use the authoritative combat position."
)]
impl Game {
    pub(in crate::game) fn internal_attack_restricted(
        &self,
        attacker: &str,
        target: &str,
    ) -> Result<bool> {
        if self.installed_attack_restricted(attacker, target)? {
            return Ok(true);
        }
        for provider in self.field_ids() {
            let (card, _) = self.internal_effective_print(&provider)?;
            if !self.card_runtime.is_loaded(&card) {
                continue;
            }
            let abilities = self
                .internal_printed_abilities(&provider)?
                .into_iter()
                .filter(|ability| is_static(&ability.node))
                .collect::<Vec<_>>();
            if abilities.is_empty() {
                continue;
            }
            let object = self.object(&provider)?;
            let ids = self
                .state
                .objects
                .keys()
                .cloned()
                .zip(1_u64..)
                .map(|(id, number)| (ObjectId(number), id))
                .collect();
            let execution = self.card_execution(
                Context {
                    source: provider,
                    generation: Generation(object.generation),
                    controller: player_id(&object.controller)?,
                },
                ids,
            )?;
            let attacker = execution.reference(self, attacker)?;
            let recipient = if let Some(seat) = target.strip_suffix(".leader") {
                RuntimeDamageRecipient::Leader {
                    player: player_id(seat)?,
                }
            } else {
                RuntimeDamageRecipient::Object {
                    object: execution.reference(self, target)?,
                }
            };
            for ability in abilities {
                for effect in &ability.node.declarations {
                    let shape =
                        projection(effect).ok_or_else(|| unsupported("attack permission shape"))?;
                    if !execution.boolean(self, shape.condition)?.0
                        || !execution.objects(self, shape.attacker)?.contains(&attacker)
                    {
                        continue;
                    }
                    if let Some(allowed_targets) = shape.target
                        && !execution
                            .recipients(self, allowed_targets)?
                            .members()
                            .contains(&recipient)
                    {
                        continue;
                    }
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::game) struct AttackLock {
    id: String,
    generation: u64,
    leaders: Option<Vec<u32>>,
    end: EffectEnd,
    interval: Option<TurnInterval>,
}

pub(in crate::game::internal) fn check_install(action: &Action) -> Result<()> {
    let Action::InstallEffect {
        subjects: InstallationSubjects::Objects { affected, .. },
        capture: InstallationTime::InstructionStart,
        effect,
        duration,
    } = action
    else {
        return Err(unsupported("attack permission installation"));
    };
    let shape = projection(effect).ok_or_else(|| unsupported("installed attack permission"))?;
    let valid_interval = match duration.intervals.as_slice() {
        [] => true,
        [Interval::DuringNextTurn { player }] => {
            matches!(&duration.end, DurationEnd::NextTurnEnd { player: end } if player == end)
        }
        _ => false,
    };
    if !valid_interval
        || *shape.attacker
            != (Selector::Bound {
                reference: *affected,
            })
        || shape.duration != duration
        || *shape.condition != Expr::Literal(Bool(true))
        || duration.anchor != (TimeAnchor::Installation {})
        || duration.usage.is_some()
        || !matches!(
            duration.end,
            DurationEnd::CurrentTurnEnd {}
                | DurationEnd::NextTurnEnd { .. }
                | DurationEnd::NoTemporalExpiry {}
        )
        || duration.intervals.len() > 1
        || duration
            .intervals
            .iter()
            .any(|interval| !matches!(interval, Interval::DuringNextTurn { .. }))
        || shape.target.is_some_and(|target| match target {
            RecipientSet::Leaders { .. } => false,
            RecipientSet::Explicit { recipients } => recipients
                .iter()
                .any(|recipient| !matches!(recipient, DamageRecipient::Leader { .. })),
            _ => true,
        })
    {
        return Err(unsupported("attack permission capture, target or lifetime"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Installed attack restrictions share generation-bound authority."
)]
impl Game {
    pub(in crate::game::internal) fn internal_install_attack_restriction(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check_install(&instruction.action)?;
        let Action::InstallEffect {
            subjects: InstallationSubjects::Objects { objects, affected },
            effect,
            duration,
            ..
        } = &instruction.action
        else {
            return Err(unsupported("attack permission subjects"));
        };
        let shape = projection(effect).ok_or_else(|| unsupported("attack permission"))?;
        let leaders = shape
            .target
            .map(|target| {
                execution.recipients(self, target).map(|recipients| {
                    recipients
                        .members()
                        .iter()
                        .map(|recipient| match recipient {
                            RuntimeDamageRecipient::Leader { player } => Ok(player.seat),
                            RuntimeDamageRecipient::Object { .. } => {
                                Err(unsupported("installed attack target"))
                            }
                        })
                        .collect::<Result<Vec<_>>>()
                })
            })
            .transpose()?
            .transpose()?;
        let members = execution.objects(self, objects)?;
        execution.slots.insert(
            affected.key(),
            Slot::Objects {
                members: members.clone(),
                on_field_at_capture: true,
            },
        );
        let end = EffectEnd::at(self, execution, &duration.end)?;
        let interval = duration
            .intervals
            .first()
            .map(|interval| TurnInterval::at(self, execution, interval))
            .transpose()?;
        let mut installed = Vec::new();
        for member in members {
            let id = execution.object_id(member)?.to_owned();
            let object = self.object(&id)?;
            if object.zone != "field" || object.generation != member.generation.0 {
                continue;
            }
            self.state.ir_attack_restrictions.push(AttackLock {
                id,
                generation: member.generation.0,
                leaders: leaders.clone(),
                end,
                interval,
            });
            installed.push(member);
        }
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Installed(installed),
        );
        Ok(())
    }

    fn installed_attack_restricted(&self, attacker: &str, target: &str) -> Result<bool> {
        let object = self.object(attacker)?;
        let leader = target
            .strip_suffix(".leader")
            .map(player_id)
            .transpose()?
            .map(|player| player.seat);
        Ok(self.state.ir_attack_restrictions.iter().any(|lock| {
            lock.id == attacker
                && lock.generation == object.generation
                && object.zone == "field"
                && lock.interval.is_none_or(TurnInterval::active)
                && !lock.end.source_expired(self)
                && lock
                    .leaders
                    .as_ref()
                    .is_none_or(|leaders| leader.is_some_and(|leader| leaders.contains(&leader)))
        }))
    }

    pub(in crate::game) fn internal_attack_restrictions_end_turn(&mut self, turn: u64) {
        self.state.ir_attack_restrictions.retain_mut(|lock| {
            if let Some(interval) = lock.interval.as_mut() {
                interval.finish(turn);
            }
            lock.end != EffectEnd::Turn(turn) && lock.interval.is_none_or(TurnInterval::pending)
        });
    }
    pub(in crate::game) fn internal_attack_restrictions_begin_turn(&mut self) -> Result<()> {
        let turn = self.state.schedule.current()?;
        let player = player_id(self.active())?.seat;
        for lock in &mut self.state.ir_attack_restrictions {
            lock.end.begin(turn, player);
            if let Some(interval) = lock.interval.as_mut() {
                interval.begin(turn, player);
            }
        }
        Ok(())
    }
}
