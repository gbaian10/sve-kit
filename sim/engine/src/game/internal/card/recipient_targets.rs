use super::super::{
    DecisionToken, DecisionView, DistributionMember, InputDomain, TargetDomain, Wait,
};
use super::selection::{TargetCharacteristics, characteristics};
use super::{Execution, Game, Nat, Result, invalid, player_id, seat_name, unsupported};
use crate::ir::ObjectRef;
use crate::ir::{
    Binding, Cardinality, ChoiceDomain, DamageRecipient, Decision, PlayerRef, RecipientSet,
    RuntimeDamageRecipient,
};
use alloc::collections::BTreeSet;

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct RecipientWait {
    pub(in crate::game::internal) binding: Binding<Vec<DamageRecipient>>,
    members: Vec<RuntimeDamageRecipient>,
    bounds: Cardinality,
    pub(in crate::game::internal) characteristics: Vec<(ObjectRef, TargetCharacteristics)>,
}

pub(super) fn check(candidates: &RecipientSet, bounds: Cardinality) -> Result<()> {
    if bounds.maximum.is_none() {
        return Err(unsupported("unbounded recipient targets"));
    }
    match candidates {
        RecipientSet::Leaders { players } if players.relative_to == (PlayerRef::Controller {}) => {
            Ok(())
        }
        RecipientSet::Objects { objects } => super::selection::field_selector(objects),
        RecipientSet::Union { sets } if !sets.is_empty() => {
            for set in sets {
                check(set, bounds)?;
            }
            Ok(())
        }
        _ => Err(unsupported("recipient target candidates")),
    }
}

impl RecipientWait {
    pub(in crate::game::internal) fn validate(
        &self,
        handles: &[u64],
    ) -> Result<Vec<RuntimeDamageRecipient>> {
        if !super::super::contains(
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

    pub(in crate::game::internal) fn selections(&self) -> Result<Vec<Vec<RuntimeDamageRecipient>>> {
        if self.members.len() > 12 {
            return Err(unsupported(
                "recipient target feasibility exceeds both fields and leaders",
            ));
        }
        let end = 1_u64
            .checked_shl(u32::try_from(self.members.len()).map_err(invalid)?)
            .ok_or_else(|| invalid("target plan overflow"))?;
        let mut selections = vec![];
        for mask in 0..end {
            let handles = (0..self.members.len())
                .filter(|index| mask & (1_u64 << index) != 0)
                .map(|index| u64::try_from(index).map_err(invalid))
                .collect::<Result<Vec<_>>>()?;
            if super::super::contains(
                self.bounds,
                Nat(u64::try_from(handles.len()).map_err(invalid)?),
            ) {
                selections.push(self.validate(&handles)?);
            }
        }
        Ok(selections)
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Recipient targets share the formal play snapshot."
)]
impl Game {
    pub(in crate::game::internal) fn recipient_target_domain(
        &self,
        execution: &Execution,
        decision: &Decision,
    ) -> Result<(RecipientWait, TargetDomain)> {
        let ChoiceDomain::Recipients {
            candidates,
            cardinality,
            binding,
        } = &decision.domain
        else {
            return Err(unsupported("recipient target domain"));
        };
        check(candidates, *cardinality)?;
        let mut members = vec![];
        self.recipient_candidates(execution, candidates, &mut members)?;
        let saved = members
            .iter()
            .filter_map(|member| {
                if let RuntimeDamageRecipient::Object { object } = member {
                    Some(*object)
                } else {
                    None
                }
            })
            .map(|object| Ok((object, characteristics(self, execution.object_id(object)?)?)))
            .collect::<Result<Vec<_>>>()?;
        let public = members
            .iter()
            .enumerate()
            .map(|(index, member)| {
                let (player, card, name) = match member {
                    RuntimeDamageRecipient::Leader { player } => (*player, None, None),
                    RuntimeDamageRecipient::Object { object } => {
                        let id = execution.object_id(*object)?;
                        let current = self.object(id)?;
                        if current.generation != object.generation.0 || current.zone != "field" {
                            return Err(unsupported("stale recipient target"));
                        }
                        (
                            player_id(&current.controller)?,
                            Some(current.card.clone()),
                            Some(self.card_name(id)?),
                        )
                    }
                };
                Ok(DistributionMember {
                    handle: u64::try_from(index).map_err(invalid)?,
                    player,
                    card,
                    name,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok((
            RecipientWait {
                binding: *binding,
                members,
                bounds: *cardinality,
                characteristics: saved,
            },
            TargetDomain {
                members: public,
                cardinality: *cardinality,
                distinct_names: false,
            },
        ))
    }

    fn recipient_candidates(
        &self,
        execution: &Execution,
        candidates: &RecipientSet,
        members: &mut Vec<RuntimeDamageRecipient>,
    ) -> Result<()> {
        match candidates {
            RecipientSet::Leaders { .. } => {
                for member in execution.recipients(self, candidates)?.members() {
                    if !members.contains(member) {
                        members.push(*member);
                    }
                }
            }
            RecipientSet::Objects { objects } => {
                for object in execution.public_field_objects(self, objects)? {
                    let id = execution.object_id(object)?;
                    if self.targetable_by(id, seat_name(execution.context.controller)?)? {
                        if self.restricted(id, "choose")? {
                            return Err(unsupported("target permission"));
                        }
                        let member = RuntimeDamageRecipient::Object { object };
                        if !members.contains(&member) {
                            members.push(member);
                        }
                    }
                }
            }
            RecipientSet::Union { sets } => {
                for set in sets {
                    self.recipient_candidates(execution, set, members)?;
                }
            }
            _ => return Err(unsupported("recipient target candidates")),
        }
        Ok(())
    }

    pub(in crate::game::internal) fn internal_recipient_target(
        &self,
        execution: &mut Execution,
        decision: &Decision,
    ) -> Result<()> {
        let (wait, domain) = self.recipient_target_domain(execution, decision)?;
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
            Wait::RecipientTargets(wait),
        ));
        Ok(())
    }
}
