//! Installed subscriptions retain frozen captures and feed the common pending queue.
use super::{
    Ability, EventHeader, EventPayload, Game, LeaveFieldEvent, Located, PendingOccurrence,
    PlayerId, Result, RuntimeAbilitySource, ScopeId, Slot, SourceInformation, invalid, unsupported,
};
use crate::game::internal::conditions::subject as subject_condition;
use crate::game::internal::{Execution, InstructionOutcome};
use crate::game::player::Viewer;
use crate::game::player::observation::InstalledEffect;
use crate::game::{Frame, Object};
use crate::ir::delayed as delayed_ir;
use crate::ir::events::subject::SubjectInformation;
use crate::ir::{
    AbilityId, AbilityInstanceId, AbilityKind, AbilityQualification, Action, CostSpec, Instruction,
    PlaySpec, SnapshotId,
};
use crate::ir::{EnterFieldEvent, EntryMethod, EventPattern, TriggerCondition};
use crate::ir::{RuntimeZoneRef, ZoneKind};

/// What installed entry observers read before the entry event is published.
pub(super) struct DelayedEntry {
    pub(super) method: EntryMethod,
    pub(super) controller: PlayerId,
    pub(super) active: PlayerId,
    pub(super) token: bool,
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct Installed {
    origin: super::EffectOrigin,
    turn: u64,
    holder: String,
    source: RuntimeAbilitySource,
    /// The installing ability, which provides the effects its callback creates.
    parent: RuntimeAbilitySource,
    ability: super::super::super::admission::PlannedAbility,
    information: SourceInformation,
    card: String,
    position: u64,
    captures: Vec<((ScopeId, u64), Slot)>,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Installed observers join the common trigger machinery."
)]
impl Game {
    pub(in crate::game::internal) fn observed_delayed(
        &self,
        perspective: Viewer,
        player: Viewer,
    ) -> Vec<InstalledEffect> {
        use crate::game::player::observation::{
            EffectExpiry, InstalledChange, InstalledEffect, TriggerTiming,
        };
        self.card_runtime
            .delayed
            .iter()
            .filter(|record| {
                record.source.controller == player.player()
                    && (perspective == player || record.information.publicly_identified())
            })
            .map(|record| InstalledEffect {
                source: record.origin.observed(self, perspective),
                expires: EffectExpiry::TurnEnd,
                main_live: true,
                intervals: vec![],
                active: true,
                change: InstalledChange::Delayed {
                    timing: if record.entry_plan().is_some() {
                        TriggerTiming::EnterField
                    } else {
                        TriggerTiming::LeaveField
                    },
                },
            })
            .collect()
    }

    pub(in crate::game::internal) fn internal_install_delay(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        if !delayed_ir::valid(&instruction.action) {
            return Err(unsupported("delayed installation"));
        }
        let Action::Delay { captures, .. } = &instruction.action else {
            return Err(unsupported("delayed action"));
        };
        let original = execution.ability.ok_or_else(|| invalid("delayed source"))?;
        let mut information = execution
            .source_information
            .clone()
            .ok_or_else(|| invalid("delayed information"))?;
        information.seal(Some(information.snapshot(self, original)?));
        let mut copied = Vec::new();
        for capture in captures {
            let (_, from, into) =
                delayed_ir::transfer(capture).ok_or_else(|| invalid("delayed capture"))?;
            let slot = execution
                .slots
                .get(&from.binding.key())
                .ok_or_else(|| invalid("missing delayed capture"))?;
            if !matches!(slot, Slot::Objects { .. }) {
                return Err(invalid("delayed slot type"));
            }
            copied.push((into.binding.key(), slot.clone()));
        }
        let holder = execution.context.source.clone();
        let (card, _) = self.internal_effective_print(&holder)?;
        let position = u64::try_from(
            self.zone_ids(
                super::super::seat_name(original.controller)?,
                &self.object(&holder)?.zone,
            )
            .iter()
            .position(|id| id == &holder)
            .ok_or_else(|| invalid("delayed source position"))?,
        )
        .map_err(invalid)?;
        self.card_runtime.next_instance = self
            .card_runtime
            .next_instance
            .checked_add(1)
            .ok_or_else(|| invalid("delayed instance overflow"))?;
        let source = RuntimeAbilitySource {
            instance: AbilityInstanceId(self.card_runtime.next_instance),
            definition: AbilityId(instruction.id.0),
            ..original
        };
        let ability = execution
            .plan
            .as_ref()
            .ok_or_else(|| invalid("delayed execution has no admitted plan"))?
            .delayed(instruction, callback(instruction, source)?)?;
        self.card_runtime.delayed.push(Installed {
            origin: super::EffectOrigin::capture(self, execution, &instruction.source)?,
            turn: self.state.schedule.current()?,
            holder,
            source,
            parent: original,
            ability,
            information,
            card,
            position: position
                .checked_add(1)
                .ok_or_else(|| invalid("delayed position overflow"))?,
            captures: copied,
        });
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Installed(vec![]));
        Ok(())
    }

    fn installed_pending(
        &mut self,
        record: &Installed,
        header: &EventHeader,
        leave: Option<LeaveFieldEvent>,
        timing: Option<EventPayload>,
    ) -> Result<PendingOccurrence> {
        self.card_runtime.next_pending = self
            .card_runtime
            .next_pending
            .checked_add(1)
            .ok_or_else(|| invalid("pending identities exhausted"))?;
        let mut pending = PendingOccurrence {
            origin: record.origin.clone(),
            id: self.card_runtime.next_pending,
            holder: record.holder.clone(),
            source: record.source,
            ability: record.ability.clone(),
            information: record.information.clone(),
            subject_information: None,
            header: header.clone(),
            leave,
            movement_after: None,
            card: record.card.clone(),
            timing,
            source_position: record.position,
            captures: record.captures.clone(),
            installed: true,
        };
        pending.capture_subject_information(self)?;
        Ok(pending)
    }

    pub(super) fn delayed_departures(
        &mut self,
        object: &Object,
        destination: &str,
        before: SnapshotId,
        frame: &Frame,
        observed_header: Option<&EventHeader>,
    ) -> Result<Vec<PendingOccurrence>> {
        if self.card_runtime.delayed.is_empty()
            || object.zone != "field"
            || destination != "cemetery"
        {
            return Ok(vec![]);
        }
        let reference = self.internal_current_reference(&object.id)?;
        let records: Vec<_> = self.card_runtime.delayed.iter().filter(|record| {
            record.captures.iter().any(|(_, slot)| matches!(slot, Slot::Objects { members, .. } if members.contains(&reference)))
        }).cloned().collect();
        if records.is_empty() {
            return Ok(vec![]);
        }
        let header = if let Some(header) = observed_header {
            header.clone()
        } else {
            self.movement_header(&object.id, before, frame, super::player_id(self.active())?)?
        };
        let leave = LeaveFieldEvent {
            object_before: reference,
            controller: super::player_id(&object.controller)?,
            lki: self.information_snapshot(reference)?.at(before).view(),
            to: RuntimeZoneRef {
                player: super::player_id(&object.owner)?,
                kind: ZoneKind::Cemetery,
            },
        };
        records
            .iter()
            .map(|record| self.installed_pending(record, &header, Some(leave), None))
            .collect()
    }

    /// The ability that installed the delayed callback running as `instance`.
    pub(in crate::game::internal) fn delayed_parent(
        &self,
        instance: AbilityInstanceId,
    ) -> Option<RuntimeAbilitySource> {
        self.card_runtime
            .delayed
            .iter()
            .find(|record| record.source.instance == instance)
            .map(|record| record.parent)
    }

    /// Installed entry observers that read the entering card's printed original cost.
    pub(super) fn delayed_entry_reads_original_cost(&self) -> bool {
        self.card_runtime.delayed.iter().any(|record| {
            record
                .entry_plan()
                .is_some_and(|plan| plan.filter.original_cost.is_some())
        })
    }

    pub(super) fn has_delayed_entries(&self) -> bool {
        self.card_runtime
            .delayed
            .iter()
            .any(|record| record.entry_plan().is_some())
    }

    /// 「次に自分の場に…が出たとき」 installations matching one committed entry.
    pub(super) fn delayed_entry_matches(
        &self,
        subject: &SubjectInformation<'_>,
        observed: &DelayedEntry,
    ) -> Result<Vec<Installed>> {
        let mut matched = Vec::new();
        for record in &self.card_runtime.delayed {
            let Some(plan) = record.entry_plan() else {
                continue;
            };
            if plan.method.is_some_and(|method| method != observed.method) {
                continue;
            }
            if subject_condition::matches(
                &plan.filter,
                subject,
                false,
                record.source.controller,
                observed.controller,
                observed.active,
                observed.token,
            )? {
                matched.push(record.clone());
            }
        }
        Ok(matched)
    }

    pub(super) fn delayed_entry_pending(
        &mut self,
        records: &[Installed],
        event: &EnterFieldEvent,
        header: &EventHeader,
        position: u64,
    ) -> Result<Vec<PendingOccurrence>> {
        records
            .iter()
            .map(|record| {
                let mut pending = self.installed_pending(
                    record,
                    header,
                    None,
                    Some(EventPayload::EnterField {
                        event: event.clone(),
                    }),
                )?;
                pending.source_position = position;
                Ok(pending)
            })
            .collect()
    }

    pub(in crate::game) fn internal_delayed_end_turn(&mut self, turn: u64) {
        self.card_runtime
            .delayed
            .retain(|record| record.turn != turn);
    }
}

impl Installed {
    fn entry_plan(&self) -> Option<delayed_ir::EntryPlan> {
        let Some(TriggerCondition::Event {
            pattern: EventPattern::EnterField { binding, predicate },
        }) = self
            .ability
            .node
            .trigger
            .as_ref()
            .map(|trigger| &trigger.on)
        else {
            return None;
        };
        delayed_ir::entry(*binding, predicate)
    }
}

fn callback(instruction: &Instruction, source: RuntimeAbilitySource) -> Result<Located<Ability>> {
    let Action::Delay {
        trigger,
        body,
        source: origin,
        ..
    } = &instruction.action
    else {
        return Err(invalid("delayed callback"));
    };
    Ok(Located {
        source: body
            .steps
            .first()
            .ok_or_else(|| invalid("empty delayed body"))?
            .source
            .clone(),
        node: Ability {
            id: source.definition,
            scope: body.scope,
            source: origin.clone(),
            kind: AbilityKind::Triggered,
            qualification: AbilityQualification::Ordinary,
            active_zones: vec![],
            event_captures: vec![],
            play: Some(PlaySpec {
                limit: None,
                captures: vec![],
                parameters: vec![],
                costs: CostSpec::Ordered { steps: vec![] },
            }),
            trigger: Some(trigger.clone()),
            body: Some(body.clone()),
            declarations: vec![],
        },
    })
}
