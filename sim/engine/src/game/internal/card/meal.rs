//! Meal payment uses the same proper-play transaction and evolution-equivalent quota.
use super::super::{DistributionMember, Execution, Fact, InputDomain, InstructionOutcome, Wait};
use super::{
    Ability, AbilityOrigin, Action, CostSpec, Expr, Game, Int, KeywordId, Located, Nat, Node,
    Result, invalid, seat_name, unsupported,
};
use crate::game::{int, list};
use crate::ir::{
    AbilityKind, AssociationId, Cardinality, DecisionId, Instruction, InstructionContext,
    MealRaceOperation, ObjectOperand, ObjectRef, PlayerRef, Quantity, QuantityUnit, RaceEvent,
    Resource, ResourceOperation,
};
use alloc::collections::BTreeSet;
use serde_json::json;

const MAX_RACES: u64 = 10_000;

pub(super) const fn intrinsic(ability: &Ability) -> bool {
    matches!(
        ability.source.origin,
        AbilityOrigin::Keyword {
            keyword: KeywordId::Meal,
            ..
        }
    )
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct MealState {
    count: u64,
    pp: i64,
    pub(in crate::game::internal) selected: Option<MealSelection>,
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct MealSelection {
    resources: Vec<ObjectRef>,
    ep: bool,
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct MealWait {
    members: Vec<ObjectRef>,
    count: usize,
    ep_allowed: bool,
}

impl MealWait {
    pub(in crate::game::internal) fn validate(
        &self,
        handles: &[u64],
        ep: bool,
    ) -> Result<MealSelection> {
        if handles.len() != self.count || (ep && !self.ep_allowed) {
            return Err(invalid("invalid meal payment"));
        }
        let mut unique = BTreeSet::new();
        let resources = handles
            .iter()
            .map(|handle| {
                if !unique.insert(*handle) {
                    return Err(invalid("duplicate meal resource"));
                }
                self.members
                    .get(usize::try_from(*handle).map_err(invalid)?)
                    .copied()
                    .ok_or_else(|| invalid("unknown meal resource"))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(MealSelection { resources, ep })
    }
}

pub(super) fn payment(ability: &Ability) -> Result<Option<MealState>> {
    if !intrinsic(ability) {
        return Ok(None);
    }
    let CostSpec::Ordered { steps } = &ability
        .play
        .as_ref()
        .ok_or_else(|| invalid("missing meal play"))?
        .costs
    else {
        return Err(unsupported("meal costs"));
    };
    let [meal, pp] = steps.as_slice() else {
        return Err(unsupported("meal payment projection"));
    };
    let Action::MealRace {
        subject: ObjectOperand::SourceHolder {},
        operation: MealRaceOperation::Meal,
        amount: Expr::Literal(Nat(count)),
        resource_links,
    } = &meal.instruction.action
    else {
        return Err(unsupported("meal resource projection"));
    };
    let Action::Resource {
        player: PlayerRef::Controller {},
        resource: Resource::PpCurrent,
        operation: ResourceOperation::Pay,
        amount: Expr::Literal(Int(pp)),
    } = pp.instruction.action
    else {
        return Err(unsupported("meal PP projection"));
    };
    if !resource_links.is_empty() || *count == 0 || *count > MAX_RACES || pp < 0 {
        return Err(unsupported("meal amount or explicit links"));
    }
    Ok(Some(MealState {
        count: *count,
        pp,
        selected: None,
    }))
}

pub(super) fn check(located: &Located<Ability>) -> Result<()> {
    let ability = &located.node;
    let state = payment(ability)?.ok_or_else(|| invalid("missing meal projection"))?;
    let play = ability
        .play
        .as_ref()
        .ok_or_else(|| invalid("missing meal play"))?;
    let CostSpec::Ordered { steps } = &play.costs else {
        return Err(unsupported("meal cost alternatives"));
    };
    if ability.kind != AbilityKind::Activated
        || ability.trigger.is_some()
        || !play.parameters.is_empty()
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
        return Err(unsupported("meal play metadata"));
    }
    let body = ability
        .body
        .as_ref()
        .ok_or_else(|| invalid("missing meal body"))?;
    let [step] = body.steps.as_slice() else {
        return Err(unsupported("meal body projection"));
    };
    let Node::Instruction { instruction } = &step.node else {
        return Err(unsupported("meal body instruction"));
    };
    if instruction.source != ability.source
        || instruction.actor != (PlayerRef::Controller {})
        || instruction.precondition.is_some()
        || instruction.context != InstructionContext::Effect
        || step.source != located.source
        || instruction.action
            != (Action::MealRace {
                subject: ObjectOperand::SourceHolder {},
                operation: MealRaceOperation::Race,
                amount: Expr::Literal(Nat(state.count)),
                resource_links: vec![],
            })
    {
        return Err(unsupported("meal race projection"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Meal shares proper-play payment, movement and rollback authority."
)]
impl Game {
    pub(in crate::game) fn internal_meal_links_valid(&self, holder: &str) -> Result<bool> {
        let source = self.object(holder)?;
        let links = list(&source.state["links"]["出走"]);
        for linked in &links {
            let resource = linked
                .as_str()
                .ok_or_else(|| invalid("invalid meal link"))?;
            let object = self.object(resource)?;
            let (Some(source_id), Some(resource_id)) = (
                self.state.ir_objects.get(holder),
                self.state.ir_objects.get(resource),
            ) else {
                return Ok(false);
            };
            if source.zone != "field"
                || object.zone != "race"
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
        Ok(true)
    }

    pub(super) fn check_meal_available(&self, holder: &str) -> Result<()> {
        let object = self.object(holder)?;
        if object.zone != "field"
            || !list(&object.state["links"]["出走"]).is_empty()
            || self
                .state
                .counters
                .get(&format!("{}.evolve_played", object.controller))
                .copied()
                .unwrap_or_default()
                > 0
        {
            return Err(crate::EngineFailure::IllegalPlay(
                "meal or evolution quota unavailable".into(),
            ));
        }
        Ok(())
    }

    fn meal_candidates(&self, execution: &Execution) -> Result<Vec<ObjectRef>> {
        let mut members = Vec::new();
        // Missing rule bindings must fail even when the evolve deck is empty.
        self.catalog
            .rule_bindings
            .matches_resource("meal_item", &[])?;
        for id in self.zone_ids(seat_name(execution.context.controller)?, "evolve_deck") {
            if self.object(&id)?.state["face_up"] != true
                && self
                    .catalog
                    .rule_bindings
                    .matches_resource("meal_item", &self.card_names(&id)?)?
            {
                members.push(execution.reference(self, &id)?);
            }
        }
        Ok(members)
    }

    pub(super) fn internal_meal_question(
        &mut self,
        execution: &mut Execution,
        play: &super::play::PlayState,
    ) -> Result<bool> {
        let Some(meal) = &play.meal else {
            return Ok(false);
        };
        if meal.selected.is_some() {
            return Ok(false);
        }
        self.check_meal_available(&execution.context.source)?;
        let members = self.meal_candidates(execution)?;
        let count = usize::try_from(meal.count).map_err(invalid)?;
        let player = self.player(seat_name(execution.context.controller)?)?;
        let ep_allowed = player.ep > 0 && meal.pp > 0;
        if members.len() < count
            || int(&player.pp["current"]).saturating_add(i64::from(ep_allowed)) < meal.pp
        {
            return Err(crate::EngineFailure::IllegalPlay(
                "insufficient meal resources".into(),
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
            InputDomain::MealPayment {
                materials: super::TargetDomain {
                    distinct_names: false,
                    members: public,
                    cardinality: Cardinality {
                        minimum: Nat(meal.count),
                        maximum: Some(Nat(meal.count)),
                    },
                },
                ep_allowed,
            },
            Wait::MealPayment(MealWait {
                members,
                count,
                ep_allowed,
            }),
        )?;
        Ok(true)
    }

    pub(super) fn internal_meal_payment(
        &mut self,
        execution: &mut Execution,
        play: &super::play::PlayState,
    ) -> Result<()> {
        let meal = play
            .meal
            .as_ref()
            .ok_or_else(|| invalid("missing meal payment"))?;
        let chosen = meal
            .selected
            .as_ref()
            .ok_or_else(|| invalid("missing meal selection"))?;
        self.check_meal_available(&execution.context.source)?;
        let candidates = self.meal_candidates(execution)?;
        let seat = seat_name(execution.context.controller)?;
        if chosen
            .resources
            .iter()
            .any(|resource| !candidates.contains(resource))
            || (chosen.ep && (meal.pp == 0 || self.player(seat)?.ep == 0))
        {
            return Err(crate::EngineFailure::IllegalPlay(
                "meal payment changed".into(),
            ));
        }
        let paid = self.internal_pay_pp(
            execution.context.controller,
            Int(meal.pp.saturating_sub(i64::from(chosen.ep))),
        )?;
        let player = self.player_mut(seat)?;
        player.ep = player.ep.saturating_sub(i64::from(chosen.ep));
        let ids = chosen
            .resources
            .iter()
            .map(|resource| execution.object_id(*resource).map(str::to_owned))
            .collect::<Result<Vec<_>>>()?;
        self.move_objects(&ids, "race", None, None, &execution.frame())?;
        self.object_mut(&execution.context.source)?.state["links"]["出走"] = json!(ids);
        let source = self.internal_current_reference(&execution.context.source)?;
        for resource in &ids {
            let resource = self.internal_current_reference(resource)?;
            let next = AssociationId(
                u64::try_from(self.card_runtime.links.len())
                    .map_err(invalid)?
                    .checked_add(1)
                    .ok_or_else(|| invalid("meal association overflow"))?,
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
        self.bump(&format!("{seat}.evolve_played"), 1);
        let CostSpec::Ordered { steps } = &play.costs else {
            return Err(invalid("missing meal steps"));
        };
        for step in steps {
            let resources = matches!(step.instruction.action, Action::MealRace { .. });
            execution.receipts.insert(
                step.instruction.receipt,
                InstructionOutcome::Payment {
                    quantity: if resources {
                        Quantity {
                            amount: Int(i64::try_from(meal.count).map_err(invalid)?),
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
