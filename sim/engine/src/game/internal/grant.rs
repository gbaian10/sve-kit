//! Basic keyword grants belong to fixed physical generations.
use super::duration::EffectEnd;
use super::{Execution, Game, InstructionOutcome, Result, unsupported};
use crate::ir::{
    Action, DurationEnd, GrantedAbility, Instruction, KeywordId, ObjectOperand, ObjectView,
    TimeAnchor,
};
use alloc::collections::BTreeSet;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::game) struct KeywordState {
    generation: u64,
    grants: Vec<KeywordGrant>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct KeywordGrant {
    end: EffectEnd,
    keywords: BTreeSet<String>,
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Only the seven basic printed keywords can be granted."
)]
pub(super) const fn keyword(keyword: KeywordId) -> Option<&'static str> {
    match keyword {
        KeywordId::Ward => Some("guard"),
        KeywordId::Aura => Some("aura"),
        KeywordId::Storm => Some("storm"),
        KeywordId::Rush => Some("rush"),
        KeywordId::Bane => Some("bane"),
        KeywordId::Intimidate => Some("intimidate"),
        KeywordId::Assail => Some("assail"),
        _ => None,
    }
}

pub(super) fn check_action(action: &Action) -> Result<()> {
    let Action::Grant {
        abilities,
        duration,
        provider,
        ..
    } = action
    else {
        return Ok(());
    };
    let basic = !abilities.is_empty() && abilities.iter().all(|ability| {
        matches!(ability, GrantedAbility::Keyword { keyword: id } if keyword(*id).is_some())
    });
    let current_provider = matches!(&provider.provider, ObjectView::Current { object }
        if object.as_ref() == &ObjectOperand::SourceHolder {});
    let fixed_duration = duration.anchor == (TimeAnchor::Installation {})
        && duration.usage.is_none()
        && duration.intervals.is_empty()
        && matches!(
            duration.end,
            DurationEnd::NoTemporalExpiry {} | DurationEnd::CurrentTurnEnd {}
        );
    if !basic || !current_provider || !fixed_duration {
        return Err(unsupported("fixed basic keyword grant"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Keyword installation and expiry share generation-bound authority."
)]
impl Game {
    pub(in crate::game) fn internal_granted_keywords(
        &self,
        holder: &str,
    ) -> Result<BTreeSet<String>> {
        let object = self.object(holder)?;
        Ok(self
            .state
            .ir_keywords
            .get(holder)
            .filter(|state| object.zone == "field" && state.generation == object.generation)
            .into_iter()
            .flat_map(|state| &state.grants)
            .flat_map(|grant| grant.keywords.iter().cloned())
            .collect())
    }

    pub(super) fn internal_grant(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check_action(&instruction.action)?;
        let Action::Grant {
            subjects,
            abilities,
            duration,
            ..
        } = &instruction.action
        else {
            return Err(unsupported("keyword grant instruction"));
        };
        let end = EffectEnd::at(self, execution, &duration.end)?;
        let keywords = abilities
            .iter()
            .map(|ability| {
                let GrantedAbility::Keyword { keyword: id } = ability else {
                    return Err(unsupported("keyword grant definition"));
                };
                keyword(*id)
                    .map(str::to_owned)
                    .ok_or_else(|| unsupported("nonbasic keyword grant"))
            })
            .collect::<Result<BTreeSet<_>>>()?;
        let mut installed = vec![];
        let mut seen = BTreeSet::new();
        for member in execution.objects(self, subjects)? {
            if !seen.insert((member.id, member.generation)) {
                continue;
            }
            let id = execution.object_id(member)?.to_owned();
            let object = self.object(&id)?;
            if object.generation != member.generation.0 || object.zone != "field" {
                continue;
            }
            let generation = object.generation;
            let state = self
                .state
                .ir_keywords
                .entry(id)
                .or_insert_with(|| KeywordState {
                    generation,
                    grants: vec![],
                });
            if state.generation != generation {
                *state = KeywordState {
                    generation,
                    grants: vec![],
                };
            }
            state.grants.push(KeywordGrant {
                end,
                keywords: keywords.clone(),
            });
            installed.push(member);
        }
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Granted(installed));
        Ok(())
    }

    pub(in crate::game) fn internal_keywords_end_turn(&mut self, turn: u64) {
        self.state.ir_keywords.retain(|_, state| {
            state
                .grants
                .retain(|grant| grant.end != EffectEnd::Turn(turn));
            !state.grants.is_empty()
        });
    }
}
