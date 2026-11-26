use super::{Ability, Execution, Game, Located, Result, invalid, player_id, unsupported};
use crate::game::player::{SelectionGroup, SelectionQuestion};
use crate::ir::{
    AbilityKind, AbilityOrigin, AbilityQualification, Cardinality, Effect, Nat, ObjectOperand,
    ObjectRef, PlayerId, PlayerRef, ZoneKind, target_priority,
};

pub(super) fn check(located: &Located<Ability>, declarations: &[Effect]) -> Result<()> {
    let ability = &located.node;
    if ability.kind != AbilityKind::Static
        || !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        || ability.qualification != AbilityQualification::Ordinary
        || ability.active_zones != [ZoneKind::Field]
        || ability.source.holder != (ObjectOperand::SourceHolder {})
        || ability.source.controller != (PlayerRef::Controller {})
        || ability.play.is_some()
        || ability.trigger.is_some()
        || ability.body.is_some()
        || !ability.event_captures.is_empty()
    {
        return Err(unsupported("required targeting header"));
    }
    for effect in declarations {
        if !target_priority::is_requirement(effect) {
            return Err(unsupported("required targeting permission"));
        }
    }
    Ok(())
}

impl Game {
    pub(super) fn target_requirements(
        &self,
        execution: &Execution,
        actor: PlayerId,
        candidates: &[ObjectRef],
    ) -> Result<Vec<(ObjectRef, usize)>> {
        let mut requirements = Vec::new();
        for candidate in candidates {
            let id = execution.object_id(*candidate)?;
            let object = self.object(id)?;
            if object.zone != "field"
                || object.generation != candidate.generation.0
                || player_id(&object.controller)? == actor
            {
                continue;
            }
            let (card, _) = self.internal_effective_print(id)?;
            if !self.card_runtime.is_loaded(&card) {
                continue;
            }
            let count = self
                .internal_printed_abilities(id)?
                .iter()
                .filter(|ability| ability.node.active_zones == [ZoneKind::Field])
                .flat_map(|ability| &ability.node.declarations)
                .filter(|effect| target_priority::is_requirement(effect))
                .count();
            if count != 0 {
                requirements.push((*candidate, count));
            }
        }
        Ok(requirements)
    }
}

/// Equal-weight requirements fit the existing disjoint selection-group protocol.
pub(super) fn expose(
    question: &mut SelectionQuestion,
    references: &[Option<ObjectRef>],
    requirements: &[(ObjectRef, usize)],
    score: usize,
) -> Result<()> {
    if score == 0
        || question.groups.len() > 1
        || requirements.iter().any(|(_, weight)| *weight != 1)
    {
        return Ok(());
    }
    let count = u64::try_from(score).map_err(invalid)?;
    let (required, others): (Vec<_>, Vec<_>) =
        references
            .iter()
            .zip(&question.members)
            .partition(|(reference, _)| {
                reference.is_some_and(|object| {
                    requirements
                        .iter()
                        .any(|(candidate, _)| *candidate == object)
                })
            });
    question.groups = vec![
        SelectionGroup {
            handles: required
                .into_iter()
                .map(|(_, member)| member.handle)
                .collect(),
            cardinality: Cardinality {
                minimum: Nat(count),
                maximum: Some(Nat(count)),
            },
        },
        SelectionGroup {
            handles: others
                .into_iter()
                .map(|(_, member)| member.handle)
                .collect(),
            cardinality: Cardinality {
                minimum: Nat(question.cardinality.minimum.0.saturating_sub(count)),
                maximum: question
                    .cardinality
                    .maximum
                    .map(|maximum| Nat(maximum.0.saturating_sub(count))),
            },
        },
    ];
    question.cardinality.minimum = Nat(question.cardinality.minimum.0.max(count));
    Ok(())
}
