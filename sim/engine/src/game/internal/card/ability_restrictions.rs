use super::super::duration::EffectEnd;
use super::super::{Execution, InstructionOutcome};
use super::{Ability, Game, Result, unsupported};
use crate::EngineFailure;
use crate::ir::{
    AbilityKind, Action, ActionPattern, Bool, DurationEnd, Effect, Expr, InstallationSubjects,
    InstallationTime, Instruction, KeywordId, ObjectOperand, PermissionSubject, Polarity, Selector,
    TimeAnchor,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::game) struct AbilityLock {
    holder: String,
    generation: u64,
    end: EffectEnd,
    except_evolution: bool,
}

pub(in crate::game::internal) fn check_install(action: &Action) -> Result<()> {
    let Action::InstallEffect {
        subjects: InstallationSubjects::Objects { objects, affected },
        capture: InstallationTime::InstructionStart,
        effect,
        duration,
    } = action
    else {
        return Err(unsupported("activation permission installation"));
    };
    let Effect::Permission {
        subject: PermissionSubject::Objects { objects: subject },
        action: ActionPattern::PlayAbility { predicate },
        polarity: Polarity::Forbid,
        condition: Expr::Literal(Bool(true)),
        duration: inner,
        ..
    } = effect.as_ref()
    else {
        return Err(unsupported("activation permission"));
    };
    if !matches!(objects, Selector::Objects { objects } if objects == &[ObjectOperand::SourceHolder {}])
        || *subject
            != (Selector::Bound {
                reference: *affected,
            })
        || predicate.kind != Some(AbilityKind::Activated)
        || predicate.keyword.is_some()
        || predicate.holder.is_some()
        || predicate.origin.is_some()
        || !matches!(predicate.except.as_slice(), [] | [KeywordId::Evolution])
        || inner != duration
        || duration.end != (DurationEnd::CurrentTurnEnd {})
        || duration.anchor != (TimeAnchor::Installation {})
        || duration.usage.is_some()
        || !duration.intervals.is_empty()
    {
        return Err(unsupported("own activated permission until turn end"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Installed permissions share authoritative activation and rule movements."
)]
impl Game {
    pub(in crate::game::internal) fn internal_install_ability_restriction(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check_install(&instruction.action)?;
        let Action::InstallEffect {
            effect, duration, ..
        } = &instruction.action
        else {
            return Err(unsupported("activation installation"));
        };
        let Effect::Permission {
            action: ActionPattern::PlayAbility { predicate },
            ..
        } = effect.as_ref()
        else {
            return Err(unsupported("activation predicate"));
        };
        let member = execution.object(self, &ObjectOperand::SourceHolder {})?;
        let holder = execution.object_id(member)?.to_owned();
        let object = self.object(&holder)?;
        let mut installed = vec![];
        if object.zone == "field" && object.generation == member.generation.0 {
            let end = EffectEnd::at(self, execution, &duration.end)?;
            self.state.ir_ability_restrictions.push(AbilityLock {
                holder,
                generation: member.generation.0,
                end,
                except_evolution: !predicate.except.is_empty(),
            });
            installed.push(member);
        }
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Installed(installed),
        );
        Ok(())
    }

    pub(in crate::game::internal) fn internal_ability_forbidden(
        &self,
        holder: &str,
        ability: &Ability,
    ) -> Result<bool> {
        if ability.kind != AbilityKind::Activated {
            return Ok(false);
        }
        let object = self.object(holder)?;
        Ok(object.zone == "field"
            && self.state.ir_ability_restrictions.iter().any(|lock| {
                lock.holder == holder
                    && lock.generation == object.generation
                    && !(lock.except_evolution && super::evolution::intrinsic(ability))
            }))
    }

    pub(in crate::game::internal) fn internal_require_ability_permission(
        &self,
        holder: &str,
        ability: &Ability,
    ) -> Result<()> {
        if self.internal_ability_forbidden(holder, ability)? {
            return Err(EngineFailure::IllegalPlay(
                "ability play is forbidden".into(),
            ));
        }
        Ok(())
    }

    pub(in crate::game) fn internal_ability_restrictions_end_turn(&mut self, turn: u64) {
        self.state
            .ir_ability_restrictions
            .retain(|lock| lock.end != EffectEnd::Turn(turn));
    }

    pub(in crate::game::internal) fn internal_ability_restrictions_rule_carry(
        &mut self,
        holder: &str,
        before: u64,
        generation: u64,
    ) {
        for lock in &mut self.state.ir_ability_restrictions {
            if lock.holder == holder && lock.generation == before {
                lock.generation = generation;
            }
        }
    }
}
