//! Evolution-equivalent abilities share EP substitution and the ordinary evolution quota.
use super::super::{DistributionMember, Execution, Fact, InputDomain, InstructionOutcome, Wait};
use super::{
    Ability, AbilityOrigin, Action, CostSpec, Expr, Game, Int, KeywordId, Located, Nat, Node,
    Result, invalid, seat_name, unsupported,
};
use crate::game::{int, list};
use crate::ir::ride;
use crate::ir::{
    AbilityKind, AssociationId, Cardinality, CostStep, DecisionId, Instruction, InstructionContext,
    MealRaceOperation, ObjectOperand, ObjectRef, PlayerRef, Quantity, QuantityUnit, RaceEvent,
    Resource, ResourceOperation, ScopeId, Selector, ZoneKind,
};
use alloc::collections::BTreeSet;
use serde_json::json;

const MAX_RACES: u64 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EntryKind {
    Meal,
    Ride,
    Advance,
}

const fn kind(ability: &Ability) -> Option<EntryKind> {
    match ability.source.origin {
        AbilityOrigin::Keyword {
            keyword: KeywordId::Meal,
            ..
        } => Some(EntryKind::Meal),
        AbilityOrigin::Keyword {
            keyword: KeywordId::Ride,
            ..
        } => Some(EntryKind::Ride),
        AbilityOrigin::Keyword {
            keyword: KeywordId::AdvanceActivate,
            ..
        } => Some(EntryKind::Advance),
        _ => None,
    }
}

pub(super) const fn intrinsic(ability: &Ability) -> bool {
    kind(ability).is_some()
}

pub(super) fn advance(ability: &Ability) -> bool {
    kind(ability) == Some(EntryKind::Advance)
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct ResourceEntryState {
    kind: EntryKind,
    scope: ScopeId,
    count: u64,
    pp: i64,
    pub(in crate::game::internal) selected: Option<ResourceEntrySelection>,
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct ResourceEntrySelection {
    resources: Vec<ObjectRef>,
    ep: bool,
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct ResourceEntryWait {
    members: Vec<ObjectRef>,
    count: usize,
    ep_allowed: bool,
}

impl ResourceEntryWait {
    pub(in crate::game::internal) fn validate(
        &self,
        handles: &[u64],
        ep: bool,
    ) -> Result<ResourceEntrySelection> {
        if handles.len() != self.count || (ep && !self.ep_allowed) {
            return Err(invalid("invalid resource entry payment"));
        }
        let mut unique = BTreeSet::new();
        let resources = handles
            .iter()
            .map(|handle| {
                if !unique.insert(*handle) {
                    return Err(invalid("duplicate entry resource"));
                }
                self.members
                    .get(usize::try_from(*handle).map_err(invalid)?)
                    .copied()
                    .ok_or_else(|| invalid("unknown resource entry resource"))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(ResourceEntrySelection { resources, ep })
    }
}

pub(super) fn payment(ability: &Ability) -> Result<Option<ResourceEntryState>> {
    if !intrinsic(ability) {
        return Ok(None);
    }
    let CostSpec::Ordered { steps } = &ability
        .play
        .as_ref()
        .ok_or_else(|| invalid("missing resource entry play"))?
        .costs
    else {
        return Err(unsupported("entry costs"));
    };
    let kind = kind(ability).ok_or_else(|| invalid("missing entry kind"))?;
    if kind == EntryKind::Advance {
        return advance_payment(ability, steps).map(Some);
    }
    let [entry, pp] = steps.as_slice() else {
        return Err(unsupported("resource entry payment projection"));
    };
    let ride = kind == EntryKind::Ride;
    let count = if ride {
        if entry.instruction.action != ride::attachment(ability.scope) {
            return Err(unsupported("ride resource projection"));
        }
        1
    } else {
        let Action::MealRace {
            subject: ObjectOperand::SourceHolder {},
            operation: MealRaceOperation::Meal,
            amount: Expr::Literal(Nat(count)),
            resource_links,
        } = &entry.instruction.action
        else {
            return Err(unsupported("resource entry projection"));
        };
        if !resource_links.is_empty() {
            return Err(unsupported("explicit entry links"));
        }
        *count
    };
    let Action::Resource {
        player: PlayerRef::Controller {},
        resource: Resource::PpCurrent,
        operation: ResourceOperation::Pay,
        amount: Expr::Literal(Int(pp)),
    } = pp.instruction.action
    else {
        return Err(unsupported("resource entry PP projection"));
    };
    if count == 0 || count > MAX_RACES || pp < 0 {
        return Err(unsupported("resource entry amount or explicit links"));
    }
    Ok(Some(ResourceEntryState {
        kind,
        scope: ability.scope,
        count,
        pp,
        selected: None,
    }))
}

fn advance_payment(ability: &Ability, steps: &[CostStep]) -> Result<ResourceEntryState> {
    let play = ability
        .play
        .as_ref()
        .ok_or_else(|| invalid("Advance play"))?;
    let [pp, rest @ ..] = steps else {
        return Err(unsupported("Advance PP cost"));
    };
    let Action::Resource {
        player: PlayerRef::Controller {},
        resource: Resource::PpCurrent,
        operation: ResourceOperation::Pay,
        amount: Expr::Literal(Int(amount)),
    } = pp.instruction.action
    else {
        return Err(unsupported("Advance printed PP cost"));
    };
    if amount < 0
        || !play.parameters.is_empty()
        || !play.captures.is_empty()
        || play.limit.is_some()
        || rest.len() > 1
        || steps.iter().any(|step| step.materials.is_some())
        || rest.iter().any(|step| {
            step.instruction.action
                != (Action::Banish {
                    objects: Selector::Objects {
                        objects: vec![ObjectOperand::SourceHolder {}],
                    },
                })
        })
    {
        return Err(unsupported("Advance printed remaining cost"));
    }
    Ok(ResourceEntryState {
        kind: EntryKind::Advance,
        scope: ability.scope,
        count: 0,
        pp: amount,
        selected: None,
    })
}

pub(super) fn check(located: &Located<Ability>) -> Result<()> {
    let ability = &located.node;
    let state = payment(ability)?.ok_or_else(|| invalid("missing resource entry projection"))?;
    if state.kind == EntryKind::Advance {
        if ability.kind != AbilityKind::Activated
            || ability.active_zones != [ZoneKind::Field]
            || ability.trigger.is_some()
        {
            return Err(unsupported("Advance activation header"));
        }
        return Ok(());
    }
    let play = ability
        .play
        .as_ref()
        .ok_or_else(|| invalid("missing resource entry play"))?;
    let CostSpec::Ordered { steps } = &play.costs else {
        return Err(unsupported("entry cost alternatives"));
    };
    if ability.kind != AbilityKind::Activated
        || ability.trigger.is_some()
        || (if state.kind == EntryKind::Ride {
            play.parameters != [ride::parameter(ability.scope)]
        } else {
            !play.parameters.is_empty()
        })
        || !play.captures.is_empty()
        || play.limit.is_some()
        || steps.iter().any(|step| {
            step.materials.is_some()
                || step.instruction.source != ability.source
                || step.instruction.actor != (PlayerRef::Controller {})
                || step.instruction.context != InstructionContext::Cost
                || step.instruction.precondition.is_some()
        })
    {
        return Err(unsupported("resource entry play metadata"));
    }
    let body = ability
        .body
        .as_ref()
        .ok_or_else(|| invalid("missing resource entry body"))?;
    let [step] = body.steps.as_slice() else {
        return Err(unsupported("resource entry body projection"));
    };
    let Node::Instruction { instruction } = &step.node else {
        return Err(unsupported("resource entry body instruction"));
    };
    if instruction.source != ability.source
        || instruction.actor != (PlayerRef::Controller {})
        || instruction.precondition.is_some()
        || instruction.context != InstructionContext::Effect
        || step.source != located.source
        || instruction.action
            != (if state.kind == EntryKind::Ride {
                ride::gain_drive()
            } else {
                Action::MealRace {
                    subject: ObjectOperand::SourceHolder {},
                    operation: MealRaceOperation::Race,
                    amount: Expr::Literal(Nat(state.count)),
                    resource_links: vec![],
                }
            })
    {
        return Err(unsupported("resource entry result projection"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Meal shares proper-play payment, movement and rollback authority."
)]
impl Game {
    pub(in crate::game) fn internal_resource_links_valid(&self, holder: &str) -> Result<bool> {
        let source = self.object(holder)?;
        for (kind, zone) in [("出走", "race"), ("憑依", "drive")] {
            let links = list(&source.state["links"][kind]);
            for linked in &links {
                let resource = linked
                    .as_str()
                    .ok_or_else(|| invalid("invalid resource entry link"))?;
                let object = self.object(resource)?;
                let (Some(source_id), Some(resource_id)) = (
                    self.state.ir_objects.get(holder),
                    self.state.ir_objects.get(resource),
                ) else {
                    return Ok(false);
                };
                if source.zone != "field"
                    || object.zone != zone
                    || !self.card_runtime.links.contains_key(&(
                        *source_id,
                        source.generation,
                        *resource_id,
                        object.generation,
                    ))
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    pub(super) fn check_resource_entry_available(
        &self,
        holder: &str,
        ability: &Ability,
    ) -> Result<()> {
        self.check_resource_entry_kind_available(
            holder,
            kind(ability).ok_or_else(|| invalid("entry kind"))?,
        )
    }

    fn check_resource_entry_kind_available(&self, holder: &str, kind: EntryKind) -> Result<()> {
        let object = self.object(holder)?;
        if object.zone != "field"
            || (match kind {
                EntryKind::Ride => object.state["ride_used"] == true,
                EntryKind::Meal => !list(&object.state["links"]["出走"]).is_empty(),
                EntryKind::Advance => false,
            })
            || self
                .state
                .counters
                .get(&format!("{}.evolve_played", object.controller))
                .copied()
                .unwrap_or_default()
                > 0
        {
            return Err(crate::EngineFailure::IllegalPlay(
                "resource entry or evolution quota unavailable".into(),
            ));
        }
        Ok(())
    }

    fn resource_entry_candidates(
        &self,
        execution: &Execution,
        kind: EntryKind,
    ) -> Result<Vec<ObjectRef>> {
        if kind == EntryKind::Advance {
            return Ok(vec![]);
        }
        let ride = kind == EntryKind::Ride;
        let mut members = Vec::new();
        // Missing rule bindings must fail even when the evolve deck is empty.
        self.catalog
            .rule_bindings
            .matches_resource(if ride { "drive_point" } else { "meal_item" }, &[])?;
        for id in self.zone_ids(seat_name(execution.context.controller)?, "evolve_deck") {
            if self.object(&id)?.state["face_up"] != true
                && self.catalog.rule_bindings.matches_resource(
                    if ride { "drive_point" } else { "meal_item" },
                    &self.card_names(&id)?,
                )?
            {
                members.push(execution.reference(self, &id)?);
            }
        }
        Ok(members)
    }

    pub(super) fn internal_resource_entry_question(
        &mut self,
        execution: &mut Execution,
        play: &super::play::PlayState,
    ) -> Result<bool> {
        let Some(entry) = &play.resource_entry else {
            return Ok(false);
        };
        if entry.selected.is_some() {
            return Ok(false);
        }
        self.check_resource_entry_kind_available(&execution.context.source, entry.kind)?;
        let members = self.resource_entry_candidates(execution, entry.kind)?;
        let count = usize::try_from(entry.count).map_err(invalid)?;
        let player = self.player(seat_name(execution.context.controller)?)?;
        let ep_allowed = player.ep > 0 && entry.pp > 0;
        if members.len() < count
            || int(&player.pp["current"]).saturating_add(i64::from(ep_allowed)) < entry.pp
        {
            return Err(crate::EngineFailure::IllegalPlay(
                "insufficient entry resources".into(),
            ));
        }
        let public = members
            .iter()
            .enumerate()
            .map(|(index, reference)| {
                let id = execution.object_id(*reference)?;
                Ok(DistributionMember {
                    handle: u64::try_from(index).map_err(invalid)?,
                    player: execution.context.controller,
                    card: Some(self.object(id)?.card.clone()),
                    name: Some(self.card_name(id)?),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.internal_wait(
            execution,
            DecisionId(0),
            execution.context.controller,
            InputDomain::ResourceEntryPayment {
                materials: super::TargetDomain {
                    distinct_names: false,
                    members: public,
                    cardinality: Cardinality {
                        minimum: Nat(entry.count),
                        maximum: Some(Nat(entry.count)),
                    },
                },
                ep_allowed,
            },
            Wait::ResourceEntryPayment(ResourceEntryWait {
                members,
                count,
                ep_allowed,
            }),
        )?;
        Ok(true)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "The transaction keeps resource movement, associations and payment receipts atomic."
    )]
    pub(super) fn internal_resource_entry_payment(
        &mut self,
        execution: &mut Execution,
        play: &super::play::PlayState,
    ) -> Result<()> {
        let entry = play
            .resource_entry
            .as_ref()
            .ok_or_else(|| invalid("missing resource entry payment"))?;
        let chosen = entry
            .selected
            .as_ref()
            .ok_or_else(|| invalid("missing resource entry selection"))?;
        self.check_resource_entry_kind_available(&execution.context.source, entry.kind)?;
        if entry.kind == EntryKind::Ride {
            execution.slots.insert(
                (entry.scope, 0),
                super::super::Slot::Objects {
                    members: chosen.resources.clone(),
                    on_field_at_capture: false,
                },
            );
        }
        let candidates = self.resource_entry_candidates(execution, entry.kind)?;
        let seat = seat_name(execution.context.controller)?;
        if chosen
            .resources
            .iter()
            .any(|resource| !candidates.contains(resource))
            || (chosen.ep && (entry.pp == 0 || self.player(seat)?.ep == 0))
        {
            return Err(crate::EngineFailure::IllegalPlay(
                "resource entry payment changed".into(),
            ));
        }
        if entry.kind == EntryKind::Advance {
            let mut costs = play.costs.clone();
            let CostSpec::Ordered { steps } = &mut costs else {
                return Err(invalid("Advance costs"));
            };
            let pp = steps
                .first_mut()
                .ok_or_else(|| invalid("Advance PP cost"))?;
            let Action::Resource { amount, .. } = &mut pp.instruction.action else {
                return Err(invalid("Advance PP action"));
            };
            *amount = Expr::Literal(Int(entry.pp.saturating_sub(i64::from(chosen.ep))));
            self.internal_card_payment(execution, &costs, false)?;
            return self.commit_entry_payment(execution, chosen.ep);
        }
        let paid = self.internal_pay_pp(
            execution.context.controller,
            Int(entry.pp.saturating_sub(i64::from(chosen.ep))),
        )?;
        self.pay_entry_ep(execution, chosen.ep)?;
        let ids = chosen
            .resources
            .iter()
            .map(|resource| execution.object_id(*resource).map(str::to_owned))
            .collect::<Result<Vec<_>>>()?;
        self.move_objects(
            &ids,
            if entry.kind == EntryKind::Ride {
                "drive"
            } else {
                "race"
            },
            None,
            None,
            &execution.frame(),
        )?;
        self.object_mut(&execution.context.source)?.state["links"][if entry.kind == EntryKind::Ride
        {
            "憑依"
        } else {
            "出走"
        }] = json!(ids);
        if entry.kind == EntryKind::Ride {
            self.object_mut(&execution.context.source)?.state["ride_used"] = json!(true);
        }
        let source = self.internal_current_reference(&execution.context.source)?;
        for resource in &ids {
            let resource = self.internal_current_reference(resource)?;
            let next = AssociationId(
                u64::try_from(self.card_runtime.links.len())
                    .map_err(invalid)?
                    .checked_add(1)
                    .ok_or_else(|| invalid("entry association overflow"))?,
            );
            self.card_runtime
                .links
                .entry((
                    source.id.0,
                    source.generation.0,
                    resource.id.0,
                    resource.generation.0,
                ))
                .or_insert(next);
        }
        self.consume_entry_quota(execution)?;
        let CostSpec::Ordered { steps } = &play.costs else {
            return Err(invalid("missing resource entry steps"));
        };
        for step in steps {
            let resources = matches!(
                step.instruction.action,
                Action::MealRace { .. } | Action::RideDrive { .. }
            );
            execution.receipts.insert(
                step.instruction.receipt,
                InstructionOutcome::Payment {
                    characters: vec![],
                    quantity: if resources {
                        Quantity {
                            amount: Int(i64::try_from(entry.count).map_err(invalid)?),
                            unit: QuantityUnit::Cards,
                        }
                    } else {
                        paid
                    },
                    subjects: if resources {
                        chosen.resources.clone()
                    } else {
                        vec![]
                    },
                },
            );
        }
        Ok(())
    }

    fn commit_entry_payment(&mut self, execution: &Execution, use_ep: bool) -> Result<()> {
        self.pay_entry_ep(execution, use_ep)?;
        self.consume_entry_quota(execution)
    }

    fn pay_entry_ep(&mut self, execution: &Execution, use_ep: bool) -> Result<()> {
        let seat = seat_name(execution.context.controller)?;
        let player = self.player_mut(seat)?;
        player.ep = player
            .ep
            .checked_sub(i64::from(use_ep))
            .filter(|ep| *ep >= 0)
            .ok_or_else(|| invalid("entry EP changed"))?;
        Ok(())
    }

    fn consume_entry_quota(&mut self, execution: &Execution) -> Result<()> {
        let seat = seat_name(execution.context.controller)?;
        self.bump(&format!("{seat}.evolve_played"), 1);
        Ok(())
    }

    pub(in crate::game::internal) fn internal_race(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        let Action::MealRace {
            subject,
            operation: MealRaceOperation::Race,
            amount,
            resource_links,
        } = &instruction.action
        else {
            return Err(unsupported("race instruction"));
        };
        if !resource_links.is_empty() {
            return Err(unsupported("explicit race links"));
        }
        let count = execution.natural(self, amount)?.0;
        if count == 0 {
            return Ok(());
        }
        if count > MAX_RACES {
            return Err(unsupported("race count exceeds execution limit"));
        }
        let reference = execution.object(self, subject)?;
        let holder = execution.object_id(reference)?.to_owned();
        if self.object(&holder)?.zone != "field"
            || self.object(&holder)?.generation != reference.generation.0
        {
            return Ok(());
        }
        let links = list(&self.object(&holder)?.state["links"]["出走"]);
        let resource = links
            .first()
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| unsupported("race without meal association"))?;
        let resource = self.internal_current_reference(resource)?;
        let next = AssociationId(
            u64::try_from(self.card_runtime.links.len())
                .map_err(invalid)?
                .checked_add(1)
                .ok_or_else(|| invalid("meal association overflow"))?,
        );
        let link = *self
            .card_runtime
            .links
            .entry((
                reference.id.0,
                reference.generation.0,
                resource.id.0,
                resource.generation.0,
            ))
            .or_insert(next);
        self.object_mut(&holder)?.state["racing"] = json!(true);
        for ordinal in 1..=count {
            let event = RaceEvent {
                subject: reference,
                controller: execution.context.controller,
                ordinal: Nat(ordinal),
                link,
            };
            execution.facts.push(Fact::Raced(event));
            self.internal_race_event(&holder, event, execution)?;
        }
        Ok(())
    }
}
