//! Entry matching runs after the whole successful batch has installed its effective information.
use super::super::super::cost::printed_cost;
use super::delayed::DelayedEntry;
use super::{
    Ability, AbilityKind, AbilityOrigin, Cause, EventPattern, EventPayload, EventSnapshotPolicy,
    Game, PlayerId, Result, SnapshotId, SourceInformation, TriggerAggregation, TriggerCondition,
    ZoneKind, invalid, player_id, unsupported,
};
use crate::game::internal::admission::PlannedAbility;
use crate::game::internal::conditions::subject as subject_condition;
use crate::game::internal::movement::ENTRY_WITHOUT_FANFARE;
use crate::game::{Frame, Object};
use crate::ir::events::subject::{SubjectFields, SubjectFilter, SubjectInformation};
use crate::ir::optional_payment;
use crate::ir::{EnterFieldEvent, EntryMethod, EntryOrigin, ExecutionId, ObjectRef};
use crate::ir::{KeywordId, LimitSpec};

pub(super) fn check_header(ability: &Ability) -> Result<bool> {
    let Some(trigger) = &ability.trigger else {
        return Ok(false);
    };
    let TriggerCondition::Event {
        pattern: EventPattern::EnterField { binding, predicate },
    } = &trigger.on
    else {
        return Ok(false);
    };
    if !matches!(ability.source.origin, AbilityOrigin::Printed { .. }) {
        return Ok(false);
    }
    if ability.kind != AbilityKind::Triggered
        || !optional_payment::trigger_play(ability)
        || trigger.active_zones != ability.active_zones
        || !(trigger.active_zones == [ZoneKind::Field]
            || (trigger.active_zones == [ZoneKind::Cemetery]
                && super::played::cemetery_body(ability)))
        || trigger.view != EventSnapshotPolicy::AfterEntry
        || trigger.aggregation != (TriggerAggregation::PerFact {})
        || trigger
            .limit
            .is_some_and(|limit| !LimitSpec::is_triggered(limit))
        || SubjectFilter::parse(SubjectFields::Entry(*binding), predicate).is_none()
    {
        return Err(unsupported("printed per-fact entry header"));
    }
    Ok(true)
}

/// 「自分の場に…が出たとき」 printed on a card that waits in its owner's cemetery.
pub(in crate::game::internal) fn cemetery_header(ability: &Ability) -> bool {
    ability.active_zones == [ZoneKind::Cemetery] && filter(ability).is_some()
}

fn filter(ability: &Ability) -> Option<SubjectFilter> {
    let TriggerCondition::Event {
        pattern: EventPattern::EnterField { binding, predicate },
    } = &ability.trigger.as_ref()?.on
    else {
        return None;
    };
    SubjectFilter::parse(SubjectFields::Entry(*binding), predicate)
}

/// 「それの{ファンファーレ}能力を使えない」 removes only the entering card's own Fanfare, before it
/// becomes pending or spends a trigger limit; other observers of the same entry still see it.
fn own_fanfare_suppressed(frame: &Frame, holder: &str, old: &Object, ability: &Ability) -> bool {
    holder == old.id
        && matches!(
            ability.source.origin,
            AbilityOrigin::Keyword {
                keyword: KeywordId::Fanfare,
                ..
            }
        )
        && frame.values.get(ENTRY_WITHOUT_FANFARE) == Some(&serde_json::Value::Bool(true))
}

fn entry_origin(game: &Game, previous: &Object) -> Result<EntryOrigin> {
    if previous.zone == "void"
        && previous.generation == 0
        && matches!(
            game.catalog.face(&previous.card, 0)?["card_type"].as_str(),
            Some("フォロワー・トークン" | "アミュレット・トークン")
        )
    {
        return Ok(EntryOrigin::Other {});
    }
    let from = if previous.zone == "resolution" {
        previous.state["entered_from"]
            .as_str()
            .ok_or_else(|| invalid("missing original play zone"))?
    } else {
        &previous.zone
    };
    let zone = match from {
        "hand" => Ok(ZoneKind::Hand),
        "deck" => Ok(ZoneKind::Deck),
        "ex" => Ok(ZoneKind::Ex),
        "cemetery" => Ok(ZoneKind::Cemetery),
        "banish" => Ok(ZoneKind::Banish),
        "evolve_deck" => Ok(ZoneKind::Evolve),
        _ => Err(unsupported("unadmitted entry origin")),
    }?;
    Ok(EntryOrigin::Zone { zone })
}

fn entry_cause(frame: &Frame) -> Result<(Cause, PlayerId, EntryMethod)> {
    Ok(
        if let Some(execution) = frame.cause["effect"]["execution"].as_u64() {
            (
                Cause::Effect {
                    execution: ExecutionId(execution),
                },
                player_id(&frame.controller)?,
                EntryMethod::AbilityPut,
            )
        } else if frame.decision["do"] == "play" {
            (
                Cause::PlayerAction {
                    player: player_id(&frame.controller)?,
                },
                player_id(&frame.controller)?,
                EntryMethod::Play,
            )
        } else {
            return Err(unsupported("entry without admitted authority cause"));
        },
    )
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "The shared movement boundary owns entry event admission."
)]
impl Game {
    /// Cemetery cards observe only through abilities active in the cemetery.
    fn entry_observers(&self) -> Result<Vec<(String, Vec<PlannedAbility>)>> {
        let mut holders = self.field_ids();
        holders.extend(self.zone_ids("P1", "cemetery"));
        holders.extend(self.zone_ids("P2", "cemetery"));
        holders
            .into_iter()
            .map(|id| {
                let zone = if self.object(&id)?.zone == "field" {
                    ZoneKind::Field
                } else {
                    ZoneKind::Cemetery
                };
                let abilities = if zone == ZoneKind::Field
                    || self
                        .card_runtime
                        .is_loaded(&self.internal_effective_print(&id)?.0)
                {
                    self.internal_printed_abilities(&id)?
                        .into_iter()
                        .filter(|ability| {
                            ability.node.active_zones == [zone] && filter(&ability.node).is_some()
                        })
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                Ok((id, abilities))
            })
            .collect()
    }

    /// The entered object, which must be exactly the next generation of the moved card.
    fn committed_entry(&mut self, old: &Object) -> Result<ObjectRef> {
        let object = self.internal_current_reference(&old.id)?;
        if object.generation.0
            != old
                .generation
                .checked_add(1)
                .ok_or_else(|| invalid("entry generation overflow"))?
        {
            return Err(invalid("entry did not commit"));
        }
        Ok(object)
    }

    /// The entering card's one-based field slot orders its simultaneous occurrences.
    fn entry_position(&self, controller: PlayerId, id: &str) -> Result<u64> {
        u64::try_from(
            self.zone_ids(super::super::seat_name(controller)?, "field")
                .iter()
                .position(|entered| entered == id)
                .ok_or_else(|| invalid("entry has no field position"))?,
        )
        .map_err(invalid)?
        .checked_add(1)
        .ok_or_else(|| invalid("entry position overflow"))
    }

    pub(in crate::game) fn internal_field_events(
        &mut self,
        previous: &[Object],
        before: SnapshotId,
        frame: &Frame,
    ) -> Result<()> {
        if previous.is_empty()
            || (!self.has_delayed_entries()
                && !self.card_runtime.definitions.cards.values().any(|program| {
                    program
                        .card()
                        .abilities
                        .iter()
                        .any(|ability| filter(&ability.node).is_some())
                }))
        {
            return Ok(());
        }
        let observers = self.entry_observers()?;
        let reads_original_cost = self.delayed_entry_reads_original_cost()
            || observers.iter().any(|(_, abilities)| {
                abilities.iter().any(|ability| {
                    filter(&ability.node).is_some_and(|plan| plan.original_cost.is_some())
                })
            });
        let active = player_id(self.active())?;
        let effect_source = self.card_runtime.active_source;
        let (cause, actor, method) = entry_cause(frame)?;
        let mut candidates = Vec::new();
        for old in previous {
            let object = self.committed_entry(old)?;
            let entry_controller = player_id(&self.object(&old.id)?.controller)?;
            let characters = self.internal_characteristics(&old.id)?;
            let original_cost = if reads_original_cost {
                Some(printed_cost(&self.internal_cost_print(&old.id)?["cost"])?)
            } else {
                None
            };
            let header = self.pending_header(actor, cause, effect_source, before)?;
            let token = self.object_type(&old.id)?.contains("トークン");
            let subject = SubjectInformation::from_characters(&characters, original_cost);
            let mut matched = Vec::new();
            for (holder, abilities) in &observers {
                let controller = player_id(&self.object(holder)?.controller)?;
                for ability in abilities {
                    let plan =
                        filter(&ability.node).ok_or_else(|| invalid("missing entry filter"))?;
                    if !own_fanfare_suppressed(frame, holder, old, &ability.node)
                        && subject_condition::matches(
                            &plan,
                            &subject,
                            *holder == old.id,
                            controller,
                            entry_controller,
                            active,
                            token,
                        )?
                        && !self.internal_trigger_forbidden(&ability.node, controller)?
                    {
                        matched.push((holder, ability, controller));
                    }
                }
            }
            let observed = DelayedEntry {
                method,
                controller: entry_controller,
                active,
                token,
            };
            let installed = self.delayed_entry_matches(&subject, &observed)?;
            if matched.is_empty() && installed.is_empty() {
                continue;
            }
            // Only a matching occurrence publishes the origin; unadmitted zones still reject it.
            let event = EnterFieldEvent {
                object,
                from: entry_origin(self, old)?,
                controller: entry_controller,
                characters,
                method,
                play_receipt: None,
            };
            let position = self.entry_position(event.controller, &old.id)?;
            candidates.extend(self.delayed_entry_pending(&installed, &event, &header, position)?);
            for (holder, ability, controller) in matched {
                let source = self.internal_card_source(holder, ability.node.id, controller)?;
                let information = SourceInformation::current(self, source)?;
                let mut pending = self.new_pending(
                    holder,
                    source,
                    ability.clone(),
                    information,
                    header.clone(),
                    None,
                )?;
                pending.source_position = position;
                pending.timing = Some(EventPayload::EnterField {
                    event: event.clone(),
                });
                candidates.push(pending);
            }
        }
        self.internal_admit_pending(candidates)
    }
}
