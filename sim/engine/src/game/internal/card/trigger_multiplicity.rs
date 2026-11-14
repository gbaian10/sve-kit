use super::{Ability, Game, Located, Result, unsupported};
use crate::ir::static_declarations::{self, Family};
use crate::ir::trigger_multiplicity::projection;
use crate::ir::{
    AbilityKind, AbilityOrigin, AbilityQualification, Effect, ObjectOperand, Phase, PlayerId,
    PlayerRef, ZoneKind,
};

pub(super) fn check(located: &Located<Ability>, declarations: &[Effect]) -> Result<()> {
    let ability = &located.node;
    if ability.kind != AbilityKind::Static
        || !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        || ability.active_zones != [ZoneKind::Field]
        || ability.qualification != AbilityQualification::Ordinary
        || ability.source.holder != (ObjectOperand::SourceHolder {})
        || ability.source.controller != (PlayerRef::Controller {})
        || ability.play.is_some()
        || ability.trigger.is_some()
        || ability.body.is_some()
        || !ability.event_captures.is_empty()
    {
        return Err(unsupported("static trigger multiplicity header"));
    }
    for effect in declarations {
        let shape = projection(effect).ok_or_else(|| unsupported("static trigger multiplicity"))?;
        if shape.source != &ability.source {
            return Err(unsupported("trigger multiplicity source"));
        }
        super::super::ordering::check_condition(shape.condition, &located.source)?;
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "The phase producer reads live rule providers before admitting occurrences."
)]
impl Game {
    pub(in crate::game::internal) fn additional_phase_triggers(
        &self,
        phase: Phase,
        controller: PlayerId,
    ) -> Result<u64> {
        let mut count = 0_u64;
        for provider in self.field_ids() {
            for located in self.internal_effective_abilities(&provider)? {
                if located.node.active_zones != [ZoneKind::Field]
                    || !static_declarations::contains(&located.node, Family::TriggerMultiplicity)
                {
                    continue;
                }
                let execution = self.provider_execution(&provider, None)?;
                for effect in &located.node.declarations {
                    let Some(shape) = projection(effect) else {
                        continue;
                    };
                    if shape.phase == phase
                        && execution.player(self, shape.controller)? == controller
                        && execution.boolean(self, shape.condition)?.0
                    {
                        count = count
                            .checked_add(shape.additional.0)
                            .ok_or_else(|| crate::invalid("trigger multiplicity overflow"))?;
                    }
                }
            }
        }
        Ok(count)
    }
}
