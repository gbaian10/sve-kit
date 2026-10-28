//! Cost materials use their own strict selection, without target permissions.
use super::{Execution, Game, Result, invalid, seat_name, unsupported};
use crate::game::internal::{
    DecisionToken, DecisionView, DistributionMember, InputDomain, Slot, Wait, search,
};
use crate::ir::{
    AbilityOrigin, Action, Binding, Cardinality, ChoiceDomain, CostSpec, Decision, DecisionKind,
    Disjointness, ExecutionRef, InputMode, Instruction, Int, KeywordId, MoveDestination,
    MoveMethod, MoveReason, Nat, ObjectOperand, ObjectRef, OwnerZone, PlaySlot, PlaySpec,
    PlayerRef, PlayerRelation, Posture, PostureEvent, Quantity, QuantityUnit, SelectionSnapshot,
    SelectionView, Selector, Visibility, WellFormedCardProgram, ZoneKind, ZoneRef,
};
use alloc::collections::BTreeSet;

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct MaterialWait {
    binding: Binding<Selector>,
    members: Vec<ObjectRef>,
    count: usize,
    field: bool,
}
impl MaterialWait {
    pub(in crate::game::internal) fn validate(&self, handles: &[u64]) -> Result<Vec<ObjectRef>> {
        if handles.len() != self.count {
            return Err(invalid("material cardinality"));
        }
        let mut seen = BTreeSet::new();
        handles
            .iter()
            .map(|handle| {
                if !seen.insert(*handle) {
                    return Err(invalid("duplicate material handle"));
                }
                self.members
                    .get(usize::try_from(*handle).map_err(invalid)?)
                    .copied()
                    .ok_or_else(|| invalid("foreign material handle"))
            })
            .collect()
    }
    pub(in crate::game::internal) fn bind(
        &self,
        execution: &mut Execution,
        members: Vec<ObjectRef>,
    ) {
        execution.slots.insert(
            self.binding.key(),
            Slot::Objects {
                members,
                on_field_at_capture: self.field,
            },
        );
    }
}

pub(super) const fn is_parameter(decision: &Decision) -> bool {
    matches!(
        decision.kind,
        DecisionKind::PlayParameter {
            slot: PlaySlot::CostMaterials,
            ..
        }
    )
}

fn zone(selector: &Selector) -> Result<ZoneKind> {
    match selector {
        Selector::Zone {
            players,
            zone: zone @ (ZoneKind::Field | ZoneKind::Ex | ZoneKind::Cemetery),
            view: SelectionView::Current {},
            predicate,
            binding,
        } if players.relation == PlayerRelation::SelfPlayer
            && players.relative_to == (PlayerRef::Controller {})
            && players.cardinality
                == (Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                }) =>
        {
            search::check_predicate(predicate, *binding)?;
            Ok(*zone)
        }
        Selector::Difference { input, except }
            if matches!(except.as_ref(),
            Selector::Objects { objects } if objects == &[ObjectOperand::SourceHolder {}]) =>
        {
            zone(input)
        }
        _ => Err(unsupported("own public material selector")),
    }
}

pub(super) fn check_parameter(decision: &Decision) -> Result<()> {
    let ChoiceDomain::Objects {
        constraint,
        target_roles: None,
        ..
    } = &decision.domain
    else {
        return Err(unsupported("material choice domain"));
    };
    let [group] = constraint.groups.as_slice() else {
        return Err(unsupported("one material group"));
    };
    if !decision.captures.is_empty()
        || decision.actor != (PlayerRef::Controller {})
        || decision.visibility != (Visibility::Public {})
        || decision.kind
            != (DecisionKind::PlayParameter {
                slot: PlaySlot::CostMaterials,
                selection_snapshot: SelectionSnapshot::AtDecision,
                play_scope: ExecutionRef::Current {},
                input: InputMode::Inline,
            })
        || constraint.cardinality.minimum.0 == 0
        || constraint.cardinality.maximum != Some(constraint.cardinality.minimum)
        || group.cardinality != constraint.cardinality
        || constraint.distinct_by.is_some()
        || constraint.disjoint != Disjointness::Disjoint
        || !constraint.aggregate_bounds.is_empty()
    {
        return Err(unsupported("strict single material group"));
    }
    zone(&group.selector)?;
    Ok(())
}

pub(super) const fn cost_objects(action: &Action) -> Option<&Selector> {
    match action {
        Action::Banish { objects }
        | Action::Posture {
            objects,
            posture: Posture::Acted,
        } => Some(objects),
        Action::Move {
            objects,
            movement,
            reason: MoveReason::Put,
        } if (matches!(
            movement.to,
            MoveDestination::Owners {
                zone: OwnerZone::Cemetery | OwnerZone::Hand
            }
        ) || matches!(
            movement.to,
            MoveDestination::Shared {
                zone: ZoneRef {
                    player: PlayerRef::Controller {},
                    kind: ZoneKind::Ex
                }
            }
        )) && movement.position.is_none()
            && movement.order.is_none()
            && movement.initial_posture.is_none()
            && movement.entry_effects.is_empty() =>
        {
            Some(objects)
        }
        _ => None,
    }
}

pub(super) fn check_cost(action: &Action, play: &PlaySpec) -> bool {
    let Some(objects) = cost_objects(action) else {
        return false;
    };
    let Selector::Bound { reference } = objects else {
        return false;
    };
    play.parameters.iter().any(|decision| {
        check_parameter(decision).is_ok()
            && matches!(&decision.domain, ChoiceDomain::Objects { binding, constraint, .. }
            if binding == reference && constraint.groups.first().is_some_and(|group|
                zone(&group.selector).is_ok_and(|zone| cost_zone(action, zone))))
    })
}

fn cost_zone(action: &Action, zone: ZoneKind) -> bool {
    match action {
        Action::Banish { .. } => true,
        Action::Posture { .. } => zone == ZoneKind::Field,
        Action::Move { movement, .. } if matches!(movement.to, MoveDestination::Shared { .. }) => {
            zone == ZoneKind::Field
        }
        Action::Move { .. } => zone != ZoneKind::Cemetery,
        _ => false,
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Material choice and payment share the play transaction."
)]
impl Game {
    fn material_candidates(
        &self,
        execution: &Execution,
        selector: &Selector,
        standing: bool,
    ) -> Result<Vec<ObjectRef>> {
        if let Selector::Difference { input, .. } = selector {
            let source = execution.object(self, &ObjectOperand::SourceHolder {})?;
            return Ok(self
                .material_candidates(execution, input, standing)?
                .into_iter()
                .filter(|object| *object != source)
                .collect());
        }
        let Selector::Zone {
            zone, predicate, ..
        } = selector
        else {
            return Err(unsupported("material selector"));
        };
        let mut members = vec![];
        for id in self.zone_ids(
            seat_name(execution.context.controller)?,
            if *zone == ZoneKind::Field {
                "field"
            } else if *zone == ZoneKind::Ex {
                "ex"
            } else {
                "cemetery"
            },
        ) {
            if standing && self.object(&id)?.state["acted"] != false {
                continue;
            }
            if search::matches_candidate(self, &id, predicate)? {
                members.push(execution.reference(self, &id)?);
            }
        }
        Ok(members)
    }

    pub(super) fn internal_material_parameter(
        &self,
        execution: &mut Execution,
        decision: &Decision,
        standing: bool,
    ) -> Result<()> {
        check_parameter(decision)?;
        let ChoiceDomain::Objects {
            constraint,
            binding,
            ..
        } = &decision.domain
        else {
            return Err(unsupported("material domain"));
        };
        let selector = &constraint
            .groups
            .first()
            .ok_or_else(|| invalid("material group"))?
            .selector;
        let members = self.material_candidates(execution, selector, standing)?;
        let count = usize::try_from(constraint.cardinality.minimum.0).map_err(invalid)?;
        if members.len() < count {
            return Err(crate::EngineFailure::IllegalPlay(
                "insufficient materials".into(),
            ));
        }
        let public = members
            .iter()
            .enumerate()
            .map(|(index, member)| {
                let id = execution.object_id(*member)?;
                Ok(DistributionMember {
                    handle: u64::try_from(index).map_err(invalid)?,
                    player: execution.context.controller,
                    card: Some(self.object(id)?.card.clone()),
                    name: Some(self.card_name(id)?),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        execution.waiting = Some((
            DecisionView {
                token: DecisionToken {
                    execution: execution.id,
                    decision: decision.id,
                    epoch: 0,
                },
                actor: execution.context.controller,
                domain: InputDomain::CostMaterials(super::TargetDomain {
                    members: public,
                    cardinality: constraint.cardinality,
                    distinct_names: false,
                }),
            },
            Wait::MaterialParameter(MaterialWait {
                binding: *binding,
                members,
                count,
                field: zone(selector)? == ZoneKind::Field,
            }),
        ));
        Ok(())
    }

    pub(super) fn internal_material_payment(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        objects: &Selector,
    ) -> Result<(Quantity, Vec<ObjectRef>)> {
        let members = match objects {
            Selector::Bound { reference } => {
                let Some(Slot::Objects { members, .. }) = execution.slots.get(&reference.key())
                else {
                    return Err(invalid("missing material selection"));
                };
                members.clone()
            }
            Selector::Objects { objects } if objects == &[ObjectOperand::SourceHolder {}] => {
                vec![execution.object(self, &ObjectOperand::SourceHolder {})?]
            }
            _ => return Err(unsupported("bound or source materials")),
        };
        for member in &members {
            let id = execution.object_id(*member)?;
            let object = self.object(id)?;
            if object.generation != member.generation.0
                || !matches!(object.zone.as_str(), "field" | "ex" | "cemetery")
                || object.controller != seat_name(execution.context.controller)?
                || (matches!(instruction.action, Action::Posture { .. })
                    && matches!(objects, Selector::Bound { .. })
                    && object.zone != "field")
            {
                return Err(crate::EngineFailure::IllegalPlay(
                    "materials changed".into(),
                ));
            }
        }
        if matches!(instruction.action, Action::Posture { .. }) {
            let ids = members
                .iter()
                .map(|member| execution.object_id(*member).map(str::to_owned))
                .collect::<Result<Vec<_>>>()?;
            if ids.iter().any(|id| {
                self.object(id)
                    .is_ok_and(|object| object.state["acted"] != false)
            }) {
                let reason = if matches!(objects, Selector::Objects { .. }) {
                    "whole-card posture cost requires a standing source"
                } else {
                    "material posture cost requires standing materials"
                };
                return Err(crate::EngineFailure::IllegalPlay(reason.into()));
            }
            if self.change_posture(&ids, true, &execution.frame().cause)? != ids {
                return Err(crate::EngineFailure::IllegalPlay(
                    "incomplete material posture cost".into(),
                ));
            }
            for member in &members {
                execution.facts.push(super::Fact::Posture(PostureEvent {
                    object: *member,
                    before: Posture::Standing,
                    after: Posture::Acted,
                }));
            }
        } else {
            let (destination, kind, method) = cost_destination(&instruction.action)?;
            if kind == ZoneKind::Ex {
                let available = 5_i64
                    .saturating_sub(self.zone_count(seat_name(execution.context.controller)?, "ex"))
                    .max(0);
                if i64::try_from(members.len()).map_err(invalid)? > available {
                    return Err(crate::EngineFailure::IllegalPlay(
                        "insufficient EX capacity for cost".into(),
                    ));
                }
            }
            let (moved, _) =
                self.internal_move_members(execution, &members, destination, kind, method, false)?;
            if moved.len() != members.len() || moved.iter().any(|result| result.after.is_none()) {
                return Err(crate::EngineFailure::IllegalPlay(
                    "incomplete material movement cost".into(),
                ));
            }
        }
        Ok((
            Quantity {
                amount: Int(i64::try_from(members.len()).map_err(invalid)?),
                unit: QuantityUnit::Cards,
            },
            members,
        ))
    }
}

pub(super) fn posture_parameter(costs: &CostSpec, decision: &Decision) -> bool {
    let ChoiceDomain::Objects { binding, .. } = &decision.domain else {
        return false;
    };
    let CostSpec::Ordered { steps } = costs else {
        return false;
    };
    steps.iter().any(|step| {
        matches!(&step.instruction.action,
        Action::Posture { objects: Selector::Bound { reference }, .. } if reference == binding)
    })
}

pub(super) fn check_card(card: &WellFormedCardProgram) -> Result<()> {
    let material = card.card().abilities.iter().any(|ability| {
        ability
            .node
            .play
            .as_ref()
            .is_some_and(|play| play.parameters.iter().any(is_parameter))
    });
    if material
        && card.card().abilities.iter().any(|ability| {
            matches!(
                ability.node.source.origin,
                AbilityOrigin::Keyword {
                    keyword: KeywordId::Quick,
                    ..
                }
            )
        })
    {
        return Err(unsupported("material fee Quick preflight"));
    }
    for ability in &card.card().abilities {
        if ability.node.play.as_ref().is_some_and(|play| {
            play.parameters
                .iter()
                .filter(|decision| is_parameter(decision))
                .count()
                > 1
        }) {
            return Err(unsupported("one public material group"));
        }
    }
    Ok(())
}

fn cost_destination(action: &Action) -> Result<(&'static str, ZoneKind, MoveMethod)> {
    match action {
        Action::Banish { .. } => Ok(("banish", ZoneKind::Banish, MoveMethod::Banish)),
        Action::Move { movement, .. }
            if movement.to
                == (MoveDestination::Owners {
                    zone: OwnerZone::Hand,
                }) =>
        {
            Ok(("hand", ZoneKind::Hand, MoveMethod::Put))
        }
        Action::Move { movement, .. }
            if matches!(
                movement.to,
                MoveDestination::Shared {
                    zone: ZoneRef {
                        player: PlayerRef::Controller {},
                        kind: ZoneKind::Ex
                    }
                }
            ) =>
        {
            Ok(("ex", ZoneKind::Ex, MoveMethod::Put))
        }
        Action::Move { .. } => Ok(("cemetery", ZoneKind::Cemetery, MoveMethod::Put)),
        _ => Err(unsupported("material payment action")),
    }
}
