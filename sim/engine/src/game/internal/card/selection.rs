use super::super::{DecisionToken, DecisionView, InputDomain, Slot, Wait, contains};
use super::{Execution, Game, Nat, ObjectRef, Result, invalid, player_id, unsupported};
use crate::ir::{
    Binding, Cardinality, ChoiceDomain, Decision, Disjointness, ScopeId, SelectionConstraint,
    Selector,
};
use crate::ir::{Bool, Expr, SelectionView, ZoneKind};
use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use serde_json::{Value, json};

/// A public target candidate, addressed only within the current question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetDomain {
    /// Token-scoped field objects with public printed identities.
    pub members: Vec<super::super::DistributionMember>,
    /// Strict play-time bounds; never clamped down to available targets.
    pub cardinality: Cardinality,
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct TargetWait {
    pub(in crate::game::internal) decision: Decision,
    binding: Binding<Selector>,
    members: Vec<ObjectRef>,
    bounds: Cardinality,
}

impl TargetWait {
    pub(in crate::game::internal) fn validate(&self, handles: &[u64]) -> Result<Vec<ObjectRef>> {
        if !contains(
            self.bounds,
            Nat(u64::try_from(handles.len()).map_err(invalid)?),
        ) {
            return Err(invalid("target cardinality"));
        }
        let mut unique = BTreeSet::new();
        handles
            .iter()
            .map(|handle| {
                if !unique.insert(*handle) {
                    return Err(invalid("duplicate target handle"));
                }
                self.members
                    .get(usize::try_from(*handle).map_err(invalid)?)
                    .copied()
                    .ok_or_else(|| invalid("foreign target handle"))
            })
            .collect()
    }
}

pub(super) fn check(constraint: &SelectionConstraint) -> Result<()> {
    let [group] = constraint.groups.as_slice() else {
        return Err(unsupported("one selector group per target decision"));
    };
    if constraint.distinct_by.is_some()
        || !constraint.aggregate_bounds.is_empty()
        || constraint.disjoint != Disjointness::Disjoint
    {
        return Err(unsupported("target properties or aggregate constraints"));
    }
    field_selector(&group.selector)
}

fn field_selector(selector: &Selector) -> Result<()> {
    match selector {
        // Resolution re-validates only the target generation; rechecking a live predicate is unspecified.
        Selector::Zone {
            zone: ZoneKind::Field,
            view: SelectionView::Current {},
            predicate,
            ..
        } if **predicate == Expr::Literal(Bool(true)) => Ok(()),
        Selector::Captured { .. } | Selector::Bound { .. } | Selector::Objects { .. } => Ok(()),
        Selector::Union { sets } => {
            for set in sets {
                field_selector(set)?;
            }
            Ok(())
        }
        _ => Err(unsupported("public field target selector")),
    }
}

pub(in crate::game::internal) fn packet(domain: &TargetDomain) -> Value {
    json!({"kind":"targets","minimum":domain.cardinality.minimum.0,"maximum":domain.cardinality.maximum.map(|n| n.0),"members":domain.members.iter().map(|member| json!({"handle":member.handle,"player":member.player.seat,"card":member.card,"name":member.name})).collect::<Vec<_>>()})
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Target evaluation uses the play's frozen authority view."
)]
impl Game {
    pub(super) fn internal_target(
        &self,
        execution: &mut Execution,
        decision: &Decision,
    ) -> Result<()> {
        let (wait, domain) = self.target_domain(execution, decision)?;
        if wait.bounds.minimum.0 > u64::try_from(wait.members.len()).map_err(invalid)? {
            return Err(crate::EngineFailure::IllegalPlay(
                "no legal play targets".into(),
            ));
        }
        execution.waiting = Some((
            DecisionView {
                token: DecisionToken {
                    execution: execution.id,
                    decision: decision.id,
                    epoch: 0,
                },
                actor: execution.context.controller,
                domain: InputDomain::Targets(domain),
            },
            Wait::Targets(wait),
        ));
        Ok(())
    }

    fn target_domain(
        &self,
        execution: &Execution,
        decision: &Decision,
    ) -> Result<(TargetWait, TargetDomain)> {
        let ChoiceDomain::Objects {
            constraint,
            binding,
            ..
        } = &decision.domain
        else {
            return Err(unsupported("target domain"));
        };
        let [group] = constraint.groups.as_slice() else {
            return Err(unsupported("target groups"));
        };
        let members = execution.objects(self, &group.selector)?;
        let bounds = Cardinality {
            minimum: Nat(constraint
                .cardinality
                .minimum
                .0
                .max(group.cardinality.minimum.0)),
            maximum: constraint
                .cardinality
                .maximum
                .into_iter()
                .chain(group.cardinality.maximum)
                .min_by_key(|n| n.0),
        };
        let public = members
            .iter()
            .enumerate()
            .map(|(index, reference)| {
                let id = execution.object_id(*reference)?;
                let object = self.object(id)?;
                if object.zone != "field"
                    || object.generation != reference.generation.0
                    || self.object_type(id)? != "フォロワー"
                {
                    return Err(unsupported("target is not a current public field follower"));
                }
                if self.restricted(id, "choose")? {
                    return Err(unsupported("target permission"));
                }
                Ok(super::super::DistributionMember {
                    handle: u64::try_from(index).map_err(invalid)?,
                    player: player_id(&object.controller)?,
                    card: Some(object.card.clone()),
                    name: Some(self.card_name(id)?),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok((
            TargetWait {
                decision: decision.clone(),
                binding: *binding,
                members,
                bounds,
            },
            TargetDomain {
                members: public,
                cardinality: bounds,
            },
        ))
    }

    pub(super) fn internal_feasible_targets(
        &self,
        execution: &Execution,
        slots: &BTreeMap<(ScopeId, u64), Slot>,
        parameters: &VecDeque<Decision>,
    ) -> Result<bool> {
        let mut candidate = execution.clone();
        candidate.slots.clone_from(slots);
        candidate.targets.clear();
        let first = parameters
            .front()
            .ok_or_else(|| invalid("missing target restart"))?;
        let (wait, _) = self.target_domain(&candidate, first)?;
        // Both public fields have a rules-defined capacity of five, so this proof is finite.
        if wait.members.len() > 10 {
            return Err(unsupported("target feasibility exceeds both fields"));
        }
        let end = 1_u64
            .checked_shl(u32::try_from(wait.members.len()).map_err(invalid)?)
            .ok_or_else(|| invalid("target plan overflow"))?;
        for mask in 0..end {
            let handles = (0..wait.members.len())
                .filter(|index| mask & (1_u64 << index) != 0)
                .map(|index| u64::try_from(index).map_err(invalid))
                .collect::<Result<Vec<_>>>()?;
            if !contains(
                wait.bounds,
                Nat(u64::try_from(handles.len()).map_err(invalid)?),
            ) {
                continue;
            }
            let mut candidate = candidate.clone();
            candidate.bind_targets(&wait, wait.validate(&handles)?);
            candidate.captures(self, &first.captures)?;
            let mut feasible = true;
            for decision in parameters.iter().skip(1) {
                let ChoiceDomain::Distribution {
                    recipients, total, ..
                } = &decision.domain
                else {
                    return Err(unsupported("feasibility parameter order"));
                };
                let recipients = candidate.recipients(self, recipients)?;
                let total = candidate.natural(self, total)?;
                if !recipients.members().is_empty()
                    && u64::try_from(recipients.members().len()).map_err(invalid)? > total.0
                {
                    feasible = false;
                    break;
                }
                candidate.captures(self, &decision.captures)?;
            }
            if feasible {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Selected targets retain a checked producer separate from installation locals."
)]
impl Execution {
    pub(in crate::game::internal) fn bind_targets(
        &mut self,
        wait: &TargetWait,
        members: Vec<ObjectRef>,
    ) {
        self.targets.insert(wait.binding.key());
        self.slots.insert(
            wait.binding.key(),
            Slot::Objects {
                members,
                on_field_at_capture: true,
            },
        );
    }
}
