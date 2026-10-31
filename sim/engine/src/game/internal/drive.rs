//! Complete Drive checks share typed continuations and the authority's ordinary mutations.
use super::{
    Execution, Fact, Game, InputDomain, InstructionOutcome, OrderCard, Result, Wait, invalid,
    player_id, seat_name, unsupported,
};
use crate::ir::{
    Action, Bool, CharacteristicPatch, CheckId, DecisionId, DriveCheckedEvent, DriveTriggerEvent,
    Duration, DurationEnd, EffectReference, Expr, Instruction, Int, Nat, NumericChange, ObjectRef,
    PlayerId, PlayerRef, Posture, RuntimeReceiptId, RuntimeZoneRef, Selector, Stat, TimeAnchor,
    TriggerIcon, ZoneKind,
};
use core::slice::from_ref;
use serde::{Deserialize, Serialize};

/// Encode a question symbol without serializing authority-only IR.
pub(super) const fn icon_name(icon: TriggerIcon) -> &'static str {
    match icon {
        TriggerIcon::Critical => "critical",
        TriggerIcon::Draw => "draw",
        TriggerIcon::Stand => "stand",
        TriggerIcon::Heal => "heal",
        TriggerIcon::Front => "front",
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::game) struct LeaderAttackLock {
    generation: u64,
    turn: u64,
}

#[derive(Clone, Debug)]
struct Run {
    instruction: Box<Instruction>,
    player: PlayerId,
    remaining: u64,
    checks: Vec<DriveCheckedEvent>,
}

#[derive(Clone, Debug)]
pub(super) struct DriveWait {
    run: Run,
    check: CheckId,
    checked: ObjectRef,
    physical: String,
    icon: Option<TriggerIcon>,
    candidates: Vec<ObjectRef>,
}

impl DriveWait {
    pub(super) fn validate(&self, accept: bool, handle: Option<u64>) -> Result<Option<ObjectRef>> {
        if !accept {
            return if handle.is_none() {
                Ok(None)
            } else {
                Err(invalid("declined trigger has a target"))
            };
        }
        if matches!(self.icon, Some(TriggerIcon::Critical | TriggerIcon::Stand)) {
            // CR 1.3.2 skips impossible designations; these are not play-time targets.
            if self.candidates.is_empty() && handle.is_none() {
                return Ok(None);
            }
            let handle =
                handle.ok_or_else(|| invalid("trigger requires one designated follower"))?;
            self.candidates
                .get(usize::try_from(handle).map_err(invalid)?)
                .copied()
                .map(Some)
                .ok_or_else(|| invalid("foreign Drive designation"))
        } else if matches!(self.icon, Some(TriggerIcon::Draw | TriggerIcon::Heal))
            && handle.is_none()
        {
            Ok(None)
        } else {
            Err(invalid("invalid Drive trigger answer"))
        }
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Drive's rule procedure shares the authoritative typed execution."
)]
impl Game {
    pub(super) fn internal_drive(
        &mut self,
        execution: &mut Execution,
        instruction: Box<Instruction>,
    ) -> Result<()> {
        let Action::DriveCheck {
            player: PlayerRef::Controller {},
            repetitions: Nat(count),
            source,
        } = &instruction.action
        else {
            return Err(unsupported("controller Drive check"));
        };
        if !matches!(count, 1 | 2) || *source != instruction.source || execution.ability.is_none() {
            return Err(unsupported("complete printed Drive check"));
        }
        let player = execution.context.controller;
        let remaining = *count;
        self.internal_drive_next(
            execution,
            Run {
                instruction,
                player,
                remaining,
                checks: vec![],
            },
        )
    }

    fn internal_drive_next(&mut self, execution: &mut Execution, mut run: Run) -> Result<()> {
        while run.remaining > 0 {
            let seat = seat_name(run.player)?;
            let deck = self.zone(seat, "deck");
            let Some(top) = deck.first() else { break };
            let id = top
                .as_str()
                .ok_or_else(|| unsupported("unidentified Drive top"))?
                .to_owned();
            self.object(&id)?
                .generation
                .checked_add(2)
                .ok_or_else(|| invalid("Drive movement generations exhausted"))?;
            let icon = self.internal_drive_icon(seat, &id)?;
            self.move_objects(
                from_ref(&id),
                "trigger",
                Some(seat),
                None,
                &execution.frame(),
            )?;
            self.state.ir_epoch = self
                .state
                .ir_epoch
                .checked_add(1)
                .ok_or_else(|| invalid("Drive identities exhausted"))?;
            let checked = execution.reference(self, &id)?;
            run.remaining = run
                .remaining
                .checked_sub(1)
                .ok_or_else(|| invalid("Drive count underflow"))?;
            let mut wait = DriveWait {
                run,
                check: CheckId(self.state.ir_epoch),
                checked,
                physical: id,
                icon,
                candidates: vec![],
            };
            if let Some(icon) = icon {
                let mut targets = Vec::new();
                if matches!(icon, TriggerIcon::Critical | TriggerIcon::Stand) {
                    for candidate in self.zone_ids(seat, "field") {
                        if self.object_type(&candidate)?.contains("フォロワー") {
                            let handle = u64::try_from(wait.candidates.len()).map_err(invalid)?;
                            wait.candidates.push(execution.reference(self, &candidate)?);
                            targets.push(OrderCard {
                                handle,
                                card: self.object(&candidate)?.card.clone(),
                                name: self.card_name(&candidate)?,
                            });
                        }
                    }
                }
                return self.internal_wait(
                    execution,
                    DecisionId(wait.run.instruction.id.0),
                    wait.run.player,
                    InputDomain::DriveTrigger { icon, targets },
                    Wait::Drive(Box::new(wait)),
                );
            }
            run = self.internal_drive_finish(execution, wait, false)?;
        }
        execution.receipts.insert(
            run.instruction.receipt,
            InstructionOutcome::DriveChecks(run.checks),
        );
        Ok(())
    }

    fn internal_drive_icon(&self, seat: &str, id: &str) -> Result<Option<TriggerIcon>> {
        if self.player(seat)?.construction != "title" {
            return Ok(None);
        }
        let card = &self.object(id)?.card;
        let facts = &self.state.facts[card];
        if facts["verified"] != true {
            return Err(unsupported("Drive requires verified icon facts"));
        }
        match facts["trigger_icon"].as_str() {
            Some("none") => Ok(None),
            Some("draw") => Ok(Some(TriggerIcon::Draw)),
            Some("heal") => Ok(Some(TriggerIcon::Heal)),
            Some("critical") => Ok(Some(TriggerIcon::Critical)),
            Some("stand") => Ok(Some(TriggerIcon::Stand)),
            _ => Err(unsupported("unverified Drive icon")),
        }
    }

    pub(super) fn internal_drive_answer(
        &mut self,
        execution: &mut Execution,
        wait: DriveWait,
        accept: bool,
        selected: Option<ObjectRef>,
    ) -> Result<()> {
        if accept {
            self.internal_drive_effect(execution, &wait, selected)?;
        }
        let run = self.internal_drive_finish(execution, wait, accept)?;
        self.internal_drive_next(execution, run)
    }

    fn internal_drive_effect(
        &mut self,
        execution: &mut Execution,
        wait: &DriveWait,
        selected: Option<ObjectRef>,
    ) -> Result<()> {
        let mut effect = (*wait.run.instruction).clone();
        match wait.icon {
            Some(TriggerIcon::Draw) => {
                self.internal_draw(execution, &effect, wait.run.player, Nat(1))?;
            }
            Some(TriggerIcon::Heal) => {
                effect.action = Action::Life {
                    player: PlayerRef::Controller {},
                    change: NumericChange::Add {
                        amount: Expr::Literal(Int(3)),
                    },
                };
                self.internal_life(execution, &effect)?;
                execution.receipts.remove(&effect.receipt);
            }
            Some(TriggerIcon::Critical) => {
                effect.action = Action::Modify {
                    subjects: Selector::Objects { objects: vec![] },
                    patch: CharacteristicPatch::Numeric {
                        stat: Stat::Attack,
                        change: NumericChange::Add {
                            amount: Expr::Literal(Int(2)),
                        },
                    },
                    expires: Duration {
                        end: DurationEnd::NoTemporalExpiry {},
                        anchor: TimeAnchor::Installation {},
                        usage: None,
                        intervals: vec![],
                    },
                    source: EffectReference::CurrentInstallation {},
                };
                self.internal_modify_chosen(execution, &effect, selected.into_iter().collect())?;
                execution.receipts.remove(&effect.receipt);
            }
            Some(TriggerIcon::Stand) => {
                self.internal_posture_members(
                    execution,
                    &effect,
                    selected.into_iter().collect(),
                    Posture::Standing,
                )?;
                execution.receipts.remove(&effect.receipt);
                if let Some(selected) = selected {
                    let id = execution.object_id(selected)?.to_owned();
                    self.state.ir_no_leader_attack.insert(
                        id,
                        LeaderAttackLock {
                            generation: selected.generation.0,
                            turn: self.state.schedule.current()?,
                        },
                    );
                }
            }
            _ => return Err(invalid("missing executable Drive icon")),
        }
        execution
            .facts
            .push(Fact::DriveTrigger(Box::new(DriveTriggerEvent {
                checked_card: wait.checked,
                player: wait.run.player,
                icon: wait.icon.ok_or_else(|| invalid("Drive icon"))?,
                check: wait.check,
                effect_receipt: RuntimeReceiptId {
                    execution: execution.id,
                    slot: effect.receipt,
                },
            })));
        Ok(())
    }

    fn internal_drive_finish(
        &mut self,
        execution: &mut Execution,
        mut wait: DriveWait,
        accepted: bool,
    ) -> Result<Run> {
        let destination = if accepted { "cemetery" } else { "deck" };
        let position = (!accepted).then(|| serde_json::json!("bottom"));
        self.move_objects(
            from_ref(&wait.physical),
            destination,
            Some(seat_name(wait.run.player)?),
            position.as_ref(),
            &execution.frame(),
        )?;
        let placed = self.object(&wait.physical)?;
        let final_destination = RuntimeZoneRef {
            player: player_id(&placed.controller)?,
            kind: match placed.zone.as_str() {
                "deck" => ZoneKind::Deck,
                "cemetery" => ZoneKind::Cemetery,
                "banish" => ZoneKind::Banish,
                _ => return Err(unsupported("Drive check final destination")),
            },
        };
        let event = DriveCheckedEvent {
            check: wait.check,
            player: wait.run.player,
            holder: execution
                .ability
                .ok_or_else(|| invalid("missing Drive holder"))?,
            checked_card: wait.checked,
            trigger_accepted: Bool(accepted),
            final_destination,
            receipt: RuntimeReceiptId {
                execution: execution.id,
                slot: wait.run.instruction.receipt,
            },
        };
        execution.facts.push(Fact::DriveChecked(Box::new(event)));
        wait.run.checks.push(event);
        Ok(wait.run)
    }

    pub(in crate::game) fn internal_leader_attack_locked(&self, id: &str) -> Result<bool> {
        let Some(lock) = self.state.ir_no_leader_attack.get(id) else {
            return Ok(false);
        };
        let object = self.object(id)?;
        Ok(object.zone == "field"
            && object.generation == lock.generation
            && self.state.schedule.current()? == lock.turn)
    }

    pub(in crate::game) fn internal_drive_end_turn(&mut self, turn: u64) {
        self.state
            .ir_no_leader_attack
            .retain(|_, lock| lock.turn != turn);
    }
}
