//! Fusion uses the existing strict material question and the complete play transaction.
use super::material::MaterialWait;
use super::{Execution, Game, Result, invalid, seat_name, unsupported};
use crate::game::internal::source::SourceInformation;
use crate::game::internal::{
    DecisionToken, DecisionView, DistributionMember, Fact, InputDomain, Wait, contains, search,
};
use crate::ir::fusion;
use crate::ir::{
    Ability, AbilityKind, AbilityQualification, Action, Characteristics, CostSpec, DecisionId,
    FusedEvent, Instruction, Int, MoveMethod, Nat, ObjectOperand, ObjectRef, Quantity,
    QuantityUnit, RuntimeObjectView, RuntimeReceiptId, Selector, TraitId, ZoneKind,
};

struct FusionMaterials {
    hand: Vec<ObjectRef>,
    ex: Vec<ObjectRef>,
    views: Vec<RuntimeObjectView>,
    traits: Vec<TraitId>,
}

pub(super) fn check_cost(action: &Action) -> bool {
    let Some(materials) = fusion::materials(action) else {
        return false;
    };
    let Selector::Difference { input, .. } = &materials.groups[0].selector else {
        return false;
    };
    let Selector::Union { sets } = input.as_ref() else {
        return false;
    };
    sets.iter().all(|selector| matches!(selector,
        Selector::Zone { binding, predicate, .. } if search::check_predicate(predicate, *binding).is_ok()))
}

pub(super) fn check_header(ability: &Ability) -> bool {
    let Some(play) = &ability.play else {
        return false;
    };
    let CostSpec::Ordered { steps } = &play.costs else {
        return false;
    };
    ability.kind == AbilityKind::Activated
        && ability.qualification == AbilityQualification::Fusion
        && ability.active_zones == [ZoneKind::Hand]
        && play.limit.is_none()
        && play.parameters.is_empty()
        && play.captures.is_empty()
        && matches!(steps.as_slice(), [step] if check_cost(&step.instruction.action))
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Fusion shares material choices and atomic card payment."
)]
impl Game {
    pub(super) fn internal_fusion_materials(
        &self,
        execution: &mut Execution,
        costs: &CostSpec,
    ) -> Result<bool> {
        let CostSpec::Ordered { steps } = costs else {
            return Err(unsupported("ordered Fusion cost"));
        };
        for step in steps {
            if !matches!(step.instruction.action, Action::Fusion { .. })
                || execution
                    .cost_materials
                    .contains_key(&step.instruction.receipt)
            {
                continue;
            }
            self.internal_hand_source(execution)?;
            let materials = fusion::materials(&step.instruction.action)
                .ok_or_else(|| unsupported("Fusion material constraint"))?;
            let selector = &materials.groups[0].selector;
            let members = self.material_candidates(execution, selector, false)?;
            let mut bounds = materials.cardinality;
            if members.is_empty() && bounds.minimum == Nat(0) {
                execution
                    .cost_materials
                    .insert(step.instruction.receipt, vec![]);
                continue;
            }
            bounds.minimum = Nat(bounds.minimum.0.max(1));
            if u64::try_from(members.len()).map_err(invalid)? < bounds.minimum.0 {
                return Err(crate::EngineFailure::IllegalPlay(
                    "insufficient Fusion materials".into(),
                ));
            }
            let candidates = members
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
                        decision: DecisionId(step.instruction.id.0),
                        epoch: 0,
                    },
                    actor: execution.context.controller,
                    domain: InputDomain::CostMaterials(super::TargetDomain {
                        members: candidates,
                        cardinality: bounds,
                        distinct_names: false,
                        roles: vec![],
                    }),
                },
                Wait::MaterialParameter(MaterialWait::cost(
                    step.instruction.receipt,
                    members,
                    bounds,
                    selector.clone(),
                )),
            ));
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn internal_fusion_payment(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<(Quantity, Vec<ObjectRef>, Vec<Characteristics>)> {
        self.internal_hand_source(execution)?;
        let materials =
            fusion::materials(&instruction.action).ok_or_else(|| unsupported("Fusion payment"))?;
        let eligible = self.material_candidates(execution, &materials.groups[0].selector, false)?;
        let members = execution
            .cost_materials
            .remove(&instruction.receipt)
            .ok_or_else(|| invalid("missing Fusion selection"))?;
        let count = u64::try_from(members.len()).map_err(invalid)?;
        if !contains(materials.cardinality, Nat(count))
            || (members.is_empty() && !eligible.is_empty())
            || members.iter().any(|member| !eligible.contains(member))
        {
            return Err(crate::EngineFailure::IllegalPlay(
                "Fusion materials changed".into(),
            ));
        }
        let source = execution.object(self, &ObjectOperand::SourceHolder {})?;
        let FusionMaterials {
            hand,
            ex,
            views: material_views,
            traits,
        } = self.prepare_fusion_materials(execution, &members)?;
        if self
            .zone_count(seat_name(execution.context.controller)?, "ex")
            .saturating_sub(i64::try_from(ex.len()).map_err(invalid)?)
            >= 5
        {
            return Err(crate::EngineFailure::IllegalPlay(
                "insufficient EX capacity for Fusion".into(),
            ));
        }
        if !hand.is_empty() {
            let discarded = self.internal_discard_members(execution, &hand)?;
            if discarded.moves.len() != hand.len()
                || discarded.moves.iter().any(|m| m.after.is_none())
            {
                return Err(crate::EngineFailure::IllegalPlay(
                    "incomplete Fusion discard".into(),
                ));
            }
        }
        if !ex.is_empty() {
            let (moved, _) = self.internal_move_members(
                execution,
                &ex,
                "cemetery",
                ZoneKind::Cemetery,
                MoveMethod::Put,
                false,
                None,
            )?;
            if moved.len() != ex.len() || moved.iter().any(|m| m.after.is_none()) {
                return Err(crate::EngineFailure::IllegalPlay(
                    "incomplete Fusion EX payment".into(),
                ));
            }
        }
        let after = self.move_fusion_source(execution, source)?;
        execution.facts.push(Fact::Fused(FusedEvent {
            execution: execution.id,
            controller: execution.context.controller,
            subject_before: source,
            subject_after: after,
            materials: material_views,
            material_traits: traits,
            payment: RuntimeReceiptId {
                execution: execution.id,
                slot: instruction.receipt,
            },
        }));
        Ok((
            Quantity {
                amount: Int(i64::try_from(count).map_err(invalid)?),
                unit: QuantityUnit::Cards,
            },
            members,
            vec![],
        ))
    }
    fn prepare_fusion_materials(
        &self,
        execution: &Execution,
        members: &[ObjectRef],
    ) -> Result<FusionMaterials> {
        let mut hand = Vec::new();
        let mut ex = Vec::new();
        let mut material_views = Vec::new();
        let mut traits = Vec::new();
        for member in members {
            let id = execution.object_id(*member)?;
            let object = self.object(id)?;
            let seat = seat_name(execution.context.controller)?;
            if object.generation != member.generation.0
                || object.owner != seat
                || object.controller != seat
            {
                return Err(crate::EngineFailure::IllegalPlay(
                    "foreign Fusion material".into(),
                ));
            }
            let snapshot = self.information_snapshot(*member)?;
            material_views.push(snapshot.view());
            for trait_id in snapshot.subject.traits {
                if !traits.contains(&trait_id) {
                    traits.push(trait_id);
                }
            }
            match object.zone.as_str() {
                "hand" => hand.push(*member),
                "ex" => ex.push(*member),
                _ => {
                    return Err(crate::EngineFailure::IllegalPlay(
                        "Fusion material zone".into(),
                    ));
                }
            }
        }
        Ok(FusionMaterials {
            hand,
            ex,
            views: material_views,
            traits,
        })
    }

    fn move_fusion_source(
        &mut self,
        execution: &mut Execution,
        source: ObjectRef,
    ) -> Result<ObjectRef> {
        let (moved, _) = self.internal_move_members(
            execution,
            &[source],
            "ex",
            ZoneKind::Ex,
            MoveMethod::Put,
            false,
            None,
        )?;
        let [movement] = moved.as_slice() else {
            return Err(invalid("Fusion source movement"));
        };
        let after = movement.after.ok_or_else(|| {
            crate::EngineFailure::IllegalPlay("Fusion source did not enter EX".into())
        })?;
        execution.context.generation = after.generation;
        let mut followed = execution
            .ability
            .ok_or_else(|| invalid("Fusion source ability"))?;
        followed.holder = after;
        execution.source_information = Some(SourceInformation::current(self, followed)?);
        Ok(after)
    }
}
